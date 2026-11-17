//! Released-package first run, with the real home hidden by the caller's mount
//! namespace. No development add-on, MCP override or preinstalled skill is used.
use super::*;
use base64::Engine;
use sha2::{Digest, Sha256};
use std::path::Path;

fn fingerprint(path: &Path) -> Result<Option<String>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(format!("{:x}", Sha256::digest(bytes)))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

// Re-exec only this test. The system is read-only, the actual home is hidden,
// and an existing login is a read-only mount rather than a credential copy.
fn isolated_run() -> Result<()> {
    let real_home = PathBuf::from(env::var("HOME")?);
    let out = PathBuf::from(env::var("FIREFOX_FIRST_RUN_ARTIFACTS")?);
    ensure!(
        out.is_absolute() && out.starts_with("/tmp"),
        "Use an absolute /tmp artifact directory"
    );
    let package = PathBuf::from(env::var("FIREFOX_FIRST_RUN_PACKAGE")?).canonicalize()?;
    ensure!(
        package.starts_with("/tmp"),
        "Extract the distribution outside the repository under /tmp"
    );
    let codex = PathBuf::from(env::var("FIREFOX_FIRST_RUN_CODEX")?).canonicalize()?;
    let gecko = PathBuf::from(env::var("FIREFOX_GECKODRIVER")?).canonicalize()?;
    let login = real_home.join(".codex/auth.json");
    ensure!(
        login.is_file(),
        "A logged-in Codex file credential is required for this model test"
    );
    fs::create_dir_all(&out)?;
    let out = out.canonicalize()?;
    ensure!(
        out.starts_with("/tmp"),
        "Artifact directory must resolve under /tmp"
    );
    ensure!(
        !out.join("first-run.json").exists(),
        "Use a fresh artifact directory for each run"
    );
    let account = tempfile::Builder::new()
        .prefix("account-")
        .tempdir_in(&out)?;
    fs::create_dir(account.path().join(".codex"))?;
    fs::create_dir(account.path().join("tmp"))?;
    fs::create_dir(account.path().join("runtime"))?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(
        account.path().join("runtime"),
        fs::Permissions::from_mode(0o700),
    )?;
    let files = [
        ".codex/config.toml",
        ".codex/AGENTS.md",
        ".codex/skills/firefox-browser/SKILL.md",
        ".codex/auth.json",
        ".mozilla/native-messaging-hosts/firefox_codex_mcp.json",
        ".local/lib/firefox-codex-mcp/firefox-codex-mcp",
        ".local/lib/firefox-codex-mcp/native-host",
        ".config/mozilla/firefox/profiles.ini",
        ".mozilla/firefox/profiles.ini",
    ];
    let before: Vec<_> = files
        .iter()
        .map(|p| fingerprint(&real_home.join(p)))
        .collect::<Result<_>>()?;
    let status = Command::new("bwrap")
        .args(["--ro-bind", "/", "/", "--bind"])
        .arg(&out)
        .arg(&out)
        .args(["--tmpfs", "/run", "--ro-bind"])
        .arg(env::current_exe()?)
        .arg("/run/firefox-first-run-test")
        .arg("--bind")
        .arg(account.path())
        .arg(&real_home)
        .arg("--ro-bind")
        .arg(&login)
        .arg(&login)
        .args([
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--unshare-pid",
            "--unshare-ipc",
            "--die-with-parent",
            "--new-session",
            "--chdir",
        ])
        .arg(&real_home)
        .args([
            "/run/firefox-first-run-test",
            "first_run_e2e::clean_distribution_first_web_task",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env_clear()
        .env("HOME", &real_home)
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "C.UTF-8")
        .env("TMPDIR", real_home.join("tmp"))
        .env("XDG_RUNTIME_DIR", real_home.join("runtime"))
        .env("FIREFOX_FIRST_RUN_ISOLATED", "1")
        .env("FIREFOX_FIRST_RUN_ARTIFACTS", &out)
        .env("FIREFOX_FIRST_RUN_PACKAGE", package)
        .env("FIREFOX_FIRST_RUN_CODEX", codex)
        .env("FIREFOX_GECKODRIVER", gecko)
        .status()?;
    let after: Vec<_> = files
        .iter()
        .map(|p| fingerprint(&real_home.join(p)))
        .collect::<Result<_>>()?;
    account.close()?;
    fs::write(
        out.join("isolation.json"),
        serde_json::to_vec_pretty(&json!({
            "systemReadOnly":true,"realHomeHidden":true,"loginMountedReadOnly":true,
            "credentialsCopied":false,"originalSettingsUnchanged":before==after,
            "filesChecked":files,"testAccountRemovedOnExit":true
        }))?,
    )?;
    ensure!(
        before == after,
        "Original files changed while the isolated test was running"
    );
    ensure!(
        status.success(),
        "Isolated first-run test failed; inspect first-run.json"
    );
    Ok(())
}

fn capture(driver: &Driver, out: &Path, seq: u64, view: &str) -> Result<()> {
    save_desktop_screenshot(driver, out, seq, view)?;
    let path = out.join(format!("step-{seq}-{view}.png"));
    let encoded = fs::read_to_string(path.with_extension("png.b64"))?;
    fs::write(
        path,
        base64::engine::general_purpose::STANDARD.decode(encoded.trim())?,
    )?;
    Ok(())
}

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn checkpoint(out: &Path, report: &mut Value, name: &str, value: Value) -> Result<()> {
    report[name] = value;
    fs::write(
        out.join("first-run.json"),
        serde_json::to_vec_pretty(report)?,
    )?;
    Ok(())
}

fn connect_launched_firefox(out: &Path) -> Result<(Driver, Process)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let log = fs::File::create(out.join("geckodriver.log"))?;
    let daemon = Process(
        Command::new(env::var("FIREFOX_GECKODRIVER")?)
            .args([
                "--connect-existing",
                "--marionette-port",
                "2828",
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--allow-system-access",
            ])
            .stdout(log.try_clone()?)
            .stderr(log)
            .spawn()?,
    );
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(40))
        .build()?;
    let base = format!("http://127.0.0.1:{port}");
    let deadline = Instant::now() + Duration::from_secs(25);
    while client.get(format!("{base}/status")).send().is_err() {
        ensure!(Instant::now() < deadline, "geckodriver did not start");
        thread::sleep(Duration::from_millis(100));
    }
    let value: Value = client
        .post(format!("{base}/session"))
        .json(&json!({"capabilities":{"alwaysMatch":{"browserName":"firefox"}}}))
        .send()?
        .json()?;
    let session = value["value"]["sessionId"]
        .as_str()
        .with_context(|| format!("Cannot attach to installer-launched Firefox: {value}"))?
        .to_owned();
    fs::write(
        out.join("browser-version.json"),
        serde_json::to_vec_pretty(&json!({
            "browserVersion":value["value"]["capabilities"]["browserVersion"],
            "platformName":value["value"]["capabilities"]["platformName"],
            "headless":true,
            "launchedByUnmodifiedInstallScript":true
        }))?,
    )?;
    Ok((
        Driver {
            client,
            base,
            session,
            popup_open: Cell::new(false),
        },
        daemon,
    ))
}

fn windows(driver: &Driver) -> Result<Vec<Value>> {
    let value: Value = driver
        .client
        .get(format!(
            "{}/session/{}/window/handles",
            driver.base, driver.session
        ))
        .send()?
        .json()?;
    Ok(value["value"].as_array().context("window handles")?.clone())
}

fn run_first_task(home: &Path, out: &Path, url: &str) -> Result<Value> {
    let work = home.join("first-web-task");
    fs::create_dir(&work)?;
    let prompt = format!(
        "이 페이지를 브라우저 화면에서 열어 주세요: {url}\n\
         화면의 Observation code를 읽고, Count 0 버튼을 딱 한 번 누른 뒤 \
         Message 입력란에 그 코드를 입력해 주세요. 화면에 Count updated to 1이 \
         표시되는지 확인하고 결과를 알려 주세요. 셸이나 스크립트로 대신 조작하지 마세요."
    );
    fs::write(out.join("first-task-prompt.txt"), &prompt)?;
    let log = fs::File::create(out.join("codex.stderr.log"))?;
    let mut codex = Process(
        Command::new(env::var("FIREFOX_FIRST_RUN_CODEX")?)
            .args([
                "exec",
                "--ephemeral",
                "--skip-git-repo-check",
                // Non-interactive exec otherwise defaults to refusing required
                // approvals. Keep automatic review, never bypass approvals.
                "--approve-for-me",
                "--json",
                "--color",
                "never",
            ])
            .arg(prompt)
            .current_dir(work)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(log)
            .spawn()?,
    );
    let stdout = codex.0.stdout.take().context("Codex stdout")?;
    let (send, receive) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if send.send(line).is_err() {
                break;
            }
        }
    });
    let mut transcript = fs::File::create(out.join("codex.jsonl"))?;
    let mut calls = Vec::new();
    let mut done = false;
    let mut skill_read = false;
    let mut answer = Value::Null;
    let deadline = Instant::now() + Duration::from_secs(240);
    loop {
        ensure!(
            Instant::now() < deadline,
            "First model-driven task timed out"
        );
        match receive.recv_timeout(Duration::from_millis(500)) {
            Ok(line) => {
                let line = line?;
                writeln!(transcript, "{line}")?;
                let event: Value = serde_json::from_str(&line)?;
                ensure!(
                    event["type"] != "error" && event["type"] != "turn.failed",
                    "First-task failure: {event}"
                );
                if event["type"] == "item.completed" {
                    let item = &event["item"];
                    if item["type"] == "mcp_tool_call" {
                        calls.push(json!({"server":item["server"],"tool":item["tool"],
                            "status":item["status"],"error":item["error"],
                            "isError":item["result"]["isError"],
                            "failureContent":if item["status"] == "failed" {item["result"]["content"].clone()}else{Value::Null}}));
                    }
                    if item["type"] == "command_execution"
                        && item["command"]
                            .as_str()
                            .is_some_and(|s| s.contains(".codex/skills/firefox-browser/SKILL.md"))
                        && item["exit_code"] == 0
                    {
                        skill_read = true;
                    }
                    if item["type"] == "agent_message" {
                        answer = item["text"].clone();
                    }
                }
                if event["type"] == "turn.completed" {
                    done = true;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    ensure!(codex.0.wait()?.success() && done, "Codex did not finish");
    let result = json!({"calls":calls,"answer":answer,"completed":done,
        "mcpConfigurationOverridden":false,"toolNamedInPrompt":false,
        "installedSkillRead":skill_read,
        "approvalMode":"approve-for-me (automatic review)"});
    fs::write(
        out.join("model-result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    for tool in [
        "firefox_open_tab",
        "firefox_snapshot",
        "firefox_click",
        "firefox_type",
    ] {
        ensure!(
            calls.iter().any(|c| c["tool"] == tool
                && c["status"] == "completed"
                && c["error"].is_null()
                && c["isError"] != true),
            "No successful {tool}: {result}"
        );
    }
    Ok(result)
}

#[test]
#[ignore = "requires a disposable mount namespace, public extracted package and read-only Codex login"]
fn clean_distribution_first_web_task() -> Result<()> {
    if env::var_os("FIREFOX_FIRST_RUN_ISOLATED").is_none() {
        return isolated_run();
    }
    ensure!(
        env::var("FIREFOX_FIRST_RUN_ISOLATED").as_deref() == Ok("1"),
        "Run only with the real home hidden by a disposable mount namespace"
    );
    let home = PathBuf::from(env::var("HOME")?);
    let out = PathBuf::from(env::var("FIREFOX_FIRST_RUN_ARTIFACTS")?);
    let package = PathBuf::from(env::var("FIREFOX_FIRST_RUN_PACKAGE")?);
    fs::create_dir_all(&out)?;
    let mut report = json!({"status":"running","packageOnly":true,"login":"read-only mount",
        "instrumentation":"Headless Firefox with Marionette; normal installer and Firefox permission controls"});
    let started = Instant::now();
    let result = (|| -> Result<()> {
        for path in [
            ".codex/config.toml",
            ".codex/AGENTS.md",
            ".codex/skills",
            ".local/lib/firefox-codex-mcp",
            ".mozilla/native-messaging-hosts/firefox_codex_mcp.json",
        ] {
            ensure!(
                !home.join(path).exists(),
                "First-run state is not clean: {path}"
            );
        }
        ensure!(
            env::var_os("CODEX_HOME").is_none(),
            "Do not override CODEX_HOME"
        );
        ensure!(
            env::vars_os().all(|(k, _)| !k.to_string_lossy().starts_with("FIREFOX_MCP_")),
            "Do not use product-specific environment overrides"
        );
        checkpoint(
            &out,
            &mut report,
            "initialState",
            json!({"mcp":false,"skill":false,"guidance":false,"nativeHost":false}),
        )?;
        let check_port =
            TcpListener::bind("127.0.0.1:2828").context("Marionette test port is occupied")?;
        drop(check_port);
        let mut setup = Process(
            Command::new(package.join("install.sh"))
                .current_dir(&package)
                // Only instrumentation: use the ordinary graphical installer branch
                // and attach to the Firefox it launches, without changing the script.
                .env("DISPLAY", ":99")
                .env("MOZ_HEADLESS", "1")
                .env("MOZ_MARIONETTE", "1")
                .env("MOZ_REMOTE_ALLOW_SYSTEM_ACCESS", "1")
                .env("MOZ_NO_REMOTE", "1")
                .stdout(Stdio::piped())
                .stderr(fs::File::create(out.join("installer.stderr.log"))?)
                .spawn()?,
        );
        let stdout = setup.0.stdout.take().context("setup stdout")?;
        let (send, receive) = mpsc::channel();
        thread::spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
            let _ = send.send(result);
        });
        let line = receive.recv_timeout(Duration::from_secs(15))??;
        let manager = line
            .trim()
            .strip_prefix("Firefox Codex 관리: ")
            .context("Manager URL")?;
        let origin = manager.split('#').next().context("manager origin")?;
        let deadline = Instant::now() + Duration::from_secs(30);
        while std::net::TcpStream::connect("127.0.0.1:2828").is_err() {
            ensure!(
                Instant::now() < deadline,
                "Installer did not launch Firefox with test instrumentation"
            );
            thread::sleep(Duration::from_millis(100));
        }
        let (driver, _daemon) = connect_launched_firefox(&out)?;
        driver.call("/window/rect", json!({"width":1280,"height":1000}))?;
        let mut found = false;
        for handle in windows(&driver)? {
            driver.call("/window", json!({"handle":handle}))?;
            if driver
                .script("return location.href;", json!([]))?
                .as_str()
                .is_some_and(|s| s.starts_with(origin))
            {
                found = true;
                break;
            }
        }
        ensure!(
            found,
            "The installer did not open its setup page in Firefox"
        );
        let deadline = Instant::now() + Duration::from_secs(15);
        while driver.script(
            "return document.querySelector('#install')?.disabled;",
            json!([]),
        )? != false
        {
            ensure!(
                Instant::now() < deadline,
                "Setup could not discover preinstalled Codex"
            );
            thread::sleep(Duration::from_millis(100));
        }
        capture(&driver, &out, 0, "first-install")?;
        checkpoint(
            &out,
            &mut report,
            "scriptLaunchedManagerAndFirefox",
            json!(true),
        )?;
        driver.click("#install")?;
        driver.click("#confirm-yes")?;
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let text = driver.script(
                "return document.querySelector('#result').textContent;",
                json!([]),
            )?;
            if text.as_str().is_some_and(|s| s.contains("설치 완료")) {
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "First installation failed: {text}"
            );
            thread::sleep(Duration::from_millis(100));
        }
        checkpoint(
            &out,
            &mut report,
            "automaticRegistration",
            json!({
                "mcp":home.join(".codex/config.toml").is_file(),
                "skill":home.join(".codex/skills/firefox-browser/SKILL.md").is_file(),
                "guidance":home.join(".codex/AGENTS.md").is_file(),
                "nativeHost":home.join(".mozilla/native-messaging-hosts/firefox_codex_mcp.json").is_file()
            }),
        )?;
        driver.click("#extension-install")?;
        driver.context("chrome")?;
        for (index, notification) in [
            "addon-install-blocked-notification",
            "addon-webext-permissions-notification",
        ]
        .iter()
        .enumerate()
        {
            let deadline = Instant::now() + Duration::from_secs(25);
            loop {
                let ready = driver.script(r#"
                    const notice=document.getElementById(arguments[0]);
                    const button=notice?.querySelector('.popup-notification-primary-button');
                    return !!document.querySelector('#notification-popup')?.hasAttribute('panelopen') && !!button && !button.disabled && !button.hasAttribute('disabled');
                "#, json!([notification]))?;
                if ready == true {
                    break;
                }
                ensure!(
                    Instant::now() < deadline,
                    "Missing enabled Firefox permission prompt: {notification}"
                );
                thread::sleep(Duration::from_millis(100));
            }
            capture(&driver, &out, index as u64 + 1, "extension-prompt")?;
            driver.script("document.getElementById(arguments[0]).querySelector('.popup-notification-primary-button').click();",json!([notification]))?;
        }
        driver.call("/actions",json!({"actions":[{"type":"key","id":"keyboard","actions":[{"type":"keyDown","value":"\u{e00c}"},{"type":"keyUp","value":"\u{e00c}"}]}]}))?;
        driver.context("content")?;
        checkpoint(
            &out,
            &mut report,
            "signedExtensionInstalledThroughFirefoxPrompts",
            json!(true),
        )?;
        driver.click("#doctor")?;
        capture(&driver, &out, 3, "setup-complete")?;
        if driver.script("return !!document.querySelector('#first-task');", json!([]))? == true {
            driver.script(
                "document.querySelector('#first-task').scrollIntoView();",
                json!([]),
            )?;
            capture(&driver, &out, 3, "first-task-guide")?;
        }
        driver.click("#close")?;
        let fixture = Fixture::with_page(include_str!("../fixtures/first-run.html"))?;
        let url = format!("{}/first-web-task", fixture.url);
        let model = run_first_task(&home, &out, &url)?;
        checkpoint(&out, &mut report, "actualCodexModel", model)?;
        let mut readback = Value::Null;
        for handle in windows(&driver)? {
            driver.call("/window", json!({"handle":handle}))?;
            if driver.script("return location.href;", json!([]))? == url {
                readback = driver.script("return {count:document.querySelector('#count').textContent,input:document.querySelector('#message').value,code:document.querySelector('#observation-code').textContent.split(': ')[1],status:document.querySelector('#state').textContent};",json!([]))?;
                capture(&driver, &out, 4, "first-web-task")?;
                break;
            }
        }
        checkpoint(
            &out,
            &mut report,
            "independentDomReadback",
            readback.clone(),
        )?;
        ensure!(
            readback["count"] == "Count 1"
                && readback["status"] == "Count updated to 1"
                && readback["code"].as_str().is_some_and(|s| !s.is_empty())
                && readback["input"] == readback["code"],
            "Browser effect not independently verified: {readback}"
        );
        let installed = home.join(".local/lib/firefox-codex-mcp/firefox-codex-mcp");
        let doctor = Command::new(installed).arg("--doctor").output()?;
        ensure!(doctor.status.success(), "Doctor failed");
        let doctor: Value = serde_json::from_slice(&doctor.stdout)?;
        for field in [
            "globalMcpRegistered",
            "nativeHostRegistered",
            "skillInstalled",
            "automaticBrowserGuidance",
        ] {
            ensure!(doctor[field] == true, "Registration missing: {field}");
        }
        checkpoint(&out, &mut report, "registrationVerified", json!(true))?;
        Ok(())
    })();
    report["elapsedSeconds"] = json!(started.elapsed().as_secs_f64());
    report["status"] = json!(if result.is_ok() { "passed" } else { "failed" });
    if let Err(error) = &result {
        report["error"] = json!(format!("{error:#}"));
    }
    fs::write(
        out.join("first-run.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    result
}
