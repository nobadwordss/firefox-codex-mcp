//! Real native messaging transport, isolated HOME and Firefox profiles only.
use super::*;
const NATIVE_ID: &str = "{a171de16-96b6-4e50-94b7-e0b54d94436a}";
use std::path::Path;
struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Browser {
    driver: Driver,
    _daemon: Daemon,
}
fn command(binary: &Path, home: &Path) -> Command {
    let mut c = Command::new(binary);
    c.env("HOME", home)
        .env("CODEX_HOME", home.join(".codex"))
        .env_remove("FIREFOX_MCP_RUNTIME_DIR")
        .env_remove("XDG_RUNTIME_DIR")
        .env(
            "FIREFOX_MCP_CODEX_BINARY",
            env::var("FIREFOX_SETUP_CODEX_BINARY")
                .unwrap_or_else(|_| "/usr/lib/chatgpt/resources/codex".into()),
        );
    c
}
impl Browser {
    fn start(home: &Path, profile: &Path) -> Result<Self> {
        fs::create_dir_all(profile)?;
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        let mut c = command(
            Path::new(
                &env::var("FIREFOX_GECKODRIVER").unwrap_or_else(|_| "/tmp/geckodriver".into()),
            ),
            home,
        );
        let firefox_runtime = home.join("firefox-runtime");
        fs::create_dir_all(&firefox_runtime)?;
        // Match the real desktop mismatch: Firefox has XDG_RUNTIME_DIR,
        // Codex and the installer do not. Do not hide it with a shared override.
        c.env("MOZ_REMOTE_ALLOW_SYSTEM_ACCESS", "1")
            .env("XDG_RUNTIME_DIR", firefox_runtime);
        let log = fs::File::create(profile.join("webdriver.log"))?;
        let daemon = Daemon(
            c.args([
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
            .timeout(Duration::from_secs(45))
            .build()?;
        let base = format!("http://127.0.0.1:{port}");
        let deadline = Instant::now() + Duration::from_secs(8);
        while client.get(format!("{base}/status")).send().is_err() {
            ensure!(Instant::now() < deadline, "geckodriver unavailable");
            thread::sleep(Duration::from_millis(50));
        }
        let mut options = json!({"args":["-headless","-no-remote","-profile",profile],"prefs":{"extensions.webextensions.uuids":json!({NATIVE_ID:EXTENSION_UUID}).to_string()}});
        if let Ok(binary) = env::var("FIREFOX_BINARY") {
            options["binary"] = json!(binary);
        }
        let value:Value=client.post(format!("{base}/session")).json(&json!({"capabilities":{"alwaysMatch":{"browserName":"firefox","moz:firefoxOptions":options}}})).send()?.json()?;
        let session = value["value"]["sessionId"]
            .as_str()
            .with_context(|| format!("Firefox startup: {value}"))?
            .to_owned();
        eprintln!(
            "native E2E Firefox {}",
            value["value"]["capabilities"]["browserVersion"]
        );
        Ok(Self {
            driver: Driver {
                client,
                base,
                session,
                popup_open: Cell::new(false),
            },
            _daemon: daemon,
        })
    }
    fn addon(&self) -> Result<()> {
        let signed = env::var_os("FIREFOX_SIGNED_XPI");
        let path = signed.as_ref().map(PathBuf::from).unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("repository")
                .join("extension")
        });
        self.driver.call(
            "/moz/addon/install",
            json!({"path":path,"temporary":signed.is_none()}),
        )?;
        Ok(())
    }
    fn popup(&self) -> Result<()> {
        self.driver.popup_for(NATIVE_ID)?;
        self.driver.wait_status("Connected")
    }
}
fn installed_binary(home: &Path) -> PathBuf {
    if env::var_os("FIREFOX_SETUP_PACKAGE").is_some() {
        PathBuf::from("/usr/libexec/firefox-codex-mcp/firefox-codex-mcp")
    } else {
        home.join(".local/lib/firefox-codex-mcp/firefox-codex-mcp")
    }
}
fn server(home: &Path) -> Result<Server> {
    let binary = installed_binary(home);
    let mut c = command(&binary, home);
    c.arg("--extension");
    Server::from_command(c)
}
fn identified_server(home: &Path, identity: &str) -> Result<Server> {
    let mut client = server(home)?;
    client.thread_id = Some(identity.into());
    Ok(client)
}

#[test]
#[ignore = "actual Firefox: retained chats, process replacement, revocation and restart"]
fn chat_continuity_and_irreversible_revocation() -> Result<()> {
    const A: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    const B: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    let root = tempfile::tempdir()?;
    let home = root.path();
    let fixture = Fixture::start()?;
    let browser = Browser::start(home, &home.join("profile"))?;
    installed(home, &browser)?;
    if env::var_os("FIREFOX_SIGNED_UI").is_none() {
        browser.addon()?;
    }
    let mut a = identified_server(home, A)?;
    let mut b = identified_server(home, B)?;
    a.wait_connected(true)?;
    ensure!(a.ok("firefox_status", json!({}))?["continuity"] == "chat");
    let tab = a.ok("firefox_open_tab", json!({"url":fixture.url}))?["tabId"].clone();
    let before = wait_snapshot(&mut a)?;
    a.ok("firefox_click", action(&before, "Count 0")?)?;
    let next = wait_snapshot(&mut a)?;
    let mut typing = action(&next, "Message")?;
    typing["text"] = json!("same document across MCP processes");
    a.ok("firefox_type", typing)?;
    let stale = wait_snapshot(&mut a)?;
    let btab = b.ok(
        "firefox_open_tab",
        json!({"url":format!("{}/b",fixture.url)}),
    )?["tabId"]
        .clone();
    b.denied("firefox_select_tab", json!({"tabId":tab}), "not owned")?;
    // Replace A while its previous process is still alive, then deliver late EOF.
    let mut replacement = identified_server(home, A)?;
    ensure!(replacement.ok("firefox_status", json!({}))?["ownedTabs"] == 1);
    replacement.denied("firefox_click", action(&stale, "Count 1")?, "snapshot")?;
    a.denied("firefox_snapshot", json!({}), "")?;
    drop(a);
    ensure!(replacement.ok("firefox_tabs", json!({}))?["tabs"][0]["tabId"] == tab);
    replacement.denied("firefox_select_tab", json!({"tabId":btab}), "not owned")?;
    drop(replacement);
    let mut resumed = identified_server(home, A)?;
    let snapshot = wait_snapshot(&mut resumed)?;
    ensure!(snapshot["text"]
        .as_str()
        .is_some_and(|s| s.contains("Count 1")));
    resumed.ok("firefox_click", action(&snapshot, "Count 1")?)?;
    let handles: Value = browser
        .driver
        .client
        .get(format!(
            "{}/session/{}/window/handles",
            browser.driver.base, browser.driver.session
        ))
        .send()?
        .json()?;
    let mut verified = false;
    for handle in handles["value"].as_array().context("handles")? {
        browser.driver.call("/window", json!({"handle":handle}))?;
        if browser.driver.script("return location.href;", json!([]))? == format!("{}/", fixture.url)
        {
            let value=browser.driver.script("return {count:document.querySelector('#count').textContent,message:document.querySelector('#message').value};",json!([]))?;
            ensure!(
                value["count"] == "Count 2"
                    && value["message"] == "same document across MCP processes",
                "DOM state lost: {value}"
            );
            verified = true;
        }
    }
    ensure!(verified, "Missing original work tab");
    resumed.ok("firefox_revoke", json!({}))?;
    drop(resumed);
    let mut revoked = identified_server(home, A)?;
    revoked.denied("firefox_select_tab", json!({"tabId":tab}), "not owned")?;
    ensure!(wait_snapshot(&mut b)?["text"]
        .as_str()
        .is_some_and(|s| s.contains("Count 0")));
    revoked.ok("firefox_open_tab", json!({"url":fixture.url}))?;
    drop(revoked);
    browser.popup()?;
    browser.driver.click("#revoke")?;
    browser.driver.close_popup()?;
    let mut after_toolbar = identified_server(home, A)?;
    ensure!(after_toolbar.ok("firefox_status", json!({}))?["ownedTabs"] == 0);
    after_toolbar.denied("firefox_snapshot", json!({}), "No AI work tab")?;
    b.denied("firefox_snapshot", json!({}), "No AI work tab")?;
    after_toolbar.ok("firefox_open_tab", json!({"url":fixture.url}))?;
    drop(after_toolbar);
    drop(b);
    drop(browser);
    let restarted = Browser::start(home, &home.join("profile"))?;
    if env::var_os("FIREFOX_SIGNED_XPI").is_none() {
        restarted.addon()?;
    }
    let mut after_restart = identified_server(home, A)?;
    after_restart.wait_connected(true)?;
    ensure!(after_restart.ok("firefox_status", json!({}))?["ownedTabs"] == 0);
    if let Ok(out) = env::var("FIREFOX_NATIVE_ARTIFACTS") {
        fs::create_dir_all(&out)?;
        fs::write(
            Path::new(&out).join("continuity-result.json"),
            serde_json::to_vec_pretty(
                &json!({"sameTabAcrossProcesses":true,"independentFormStateReadback":true,"overlapFencing":true,"lateEofIsolation":true,"staleSnapshotRejected":true,"crossChatDenied":true,"revokeIrreversible":true,"detachedToolbarRevoke":true,"restartDoesNotRestoreAccess":true,"signed":env::var_os("FIREFOX_SIGNED_XPI").is_some()}),
            )?,
        )?;
    }
    Ok(())
}
fn installer_binary() -> PathBuf {
    env::var_os("FIREFOX_SETUP_SERVER_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_firefox-codex-mcp").into())
}
fn installed(home: &Path, browser: &Browser) -> Result<()> {
    let mut c = command(&installer_binary(), home);
    let mut process = Daemon(c.arg("--manage-no-open").stdout(Stdio::piped()).spawn()?);
    let mut line = String::new();
    BufReader::new(process.0.stdout.take().context("manager stdout")?).read_line(&mut line)?;
    let url = line
        .trim()
        .strip_prefix("Firefox Codex setup: ")
        .or_else(|| line.trim().strip_prefix("Firefox Codex 관리: "))
        .context("manager URL")?;
    browser.driver.goto(url)?;
    browser.driver.click("#install")?;
    browser.driver.click("#confirm-yes")?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let text = browser.driver.script(
            "return document.querySelector('#result').textContent;",
            json!([]),
        )?;
        if text.as_str().is_some_and(|t| {
            t.contains("Connection installed")
                || t.contains("Connected to Codex")
                || t.contains("설치 완료")
        }) {
            break;
        }
        ensure!(Instant::now() < deadline, "manager install: {text}");
        thread::sleep(Duration::from_millis(100));
    }
    if env::var_os("FIREFOX_SIGNED_UI").is_some() {
        ensure!(
            env::var_os("FIREFOX_SIGNED_XPI").is_some(),
            "UI install requires the signed XPI"
        );
        let signed = env::var("FIREFOX_SIGNED_XPI")?;
        let origin = url.split('#').next().context("manager origin")?;
        let embedded = browser
            .driver
            .client
            .get(format!("{origin}extension.xpi"))
            .send()?
            .error_for_status()?
            .bytes()?;
        ensure!(
            embedded.as_ref() == fs::read(signed)?,
            "Manager served a different signed XPI"
        );
        if let Ok(out) = env::var("FIREFOX_NATIVE_ARTIFACTS") {
            fs::create_dir_all(&out)?;
            save_desktop_screenshot(&browser.driver, Path::new(&out), 0, "installer")?;
        }
        browser.driver.click("#extension-install")?;
        browser.driver.context("chrome")?;
        for notification in [
            "addon-install-blocked-notification",
            "addon-webext-permissions-notification",
        ] {
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                // Firefox's headless chrome buttons have no scrollable rectangle.
                // Invoke the actual enabled notification button, never install via
                // AddonManager or the WebDriver add-on shortcut in this flow.
                let clicked = browser.driver.script(r#"
                    const panel = document.querySelector('#notification-popup');
                    const notice = document.getElementById(arguments[0]);
                    const button = notice?.querySelector('.popup-notification-primary-button');
                    if (!panel?.hasAttribute('panelopen') || !button || button.disabled || button.hasAttribute('disabled')) return false;
                    button.click(); return true;
                "#, json!([notification]))?;
                if clicked == true {
                    break;
                }
                ensure!(
                    Instant::now() < deadline,
                    "Missing enabled install confirmation: {notification}"
                );
                thread::sleep(Duration::from_millis(100));
            }
        }
        browser.driver.call("/actions", json!({"actions":[{"type":"key","id":"keyboard","actions":[{"type":"keyDown","value":"\u{e00c}"},{"type":"keyUp","value":"\u{e00c}"}]}]}))?;
        browser.driver.context("content")?;
    }
    browser.driver.click("#close")?;
    ensure!(fs::read_to_string(home.join(".codex/config.toml"))?.contains("--extension"));
    Ok(())
}
fn wait_snapshot(server: &mut Server) -> Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        let value = server.tool("firefox_snapshot", json!({}))?;
        if value["isError"] != true {
            return Ok(serde_json::from_str(
                value["content"][0]["text"].as_str().context("snapshot")?,
            )?);
        }
        ensure!(Instant::now() < deadline, "snapshot: {value}");
        thread::sleep(Duration::from_millis(100));
    }
}
fn capture_native(driver: &Driver, out: &Path, seq: u64, view: &str) -> Result<()> {
    use base64::Engine;
    fs::create_dir_all(out)?;
    let path = out.join(format!("step-{seq}-{view}.png"));
    if driver.popup_open.get() {
        // A remote extension popup is omitted from the ordinary browser
        // screenshot. Ask its WebDriver actor to capture the rendered popup.
        let png = driver.popup_command(
            "return actor.takeScreenshot(null, 0, true, false);",
            json!([]),
        )?;
        fs::write(
            path.with_extension("png.b64"),
            png.as_str().context("popup screenshot")?,
        )?;
    } else {
        save_desktop_screenshot(driver, out, seq, view)?;
    }
    let encoded = fs::read_to_string(path.with_extension("png.b64"))?;
    fs::write(
        path,
        base64::engine::general_purpose::STANDARD.decode(encoded.trim())?,
    )?;
    Ok(())
}
fn settings_from_popup(
    browser: &Browser,
    client: &mut Server,
    original: &str,
    single_profile: bool,
    action: Option<&str>,
) -> Result<()> {
    let handles = || -> Result<Vec<Value>> {
        let response: Value = browser
            .driver
            .client
            .get(format!(
                "{}/session/{}/window/handles",
                browser.driver.base, browser.driver.session
            ))
            .send()?
            .json()?;
        Ok(response["value"]
            .as_array()
            .context("window handles")?
            .clone())
    };
    let before_handles = handles()?;
    let before_tabs = client.ok("firefox_tabs", json!({}))?["tabs"].clone();
    let artifacts = env::var_os("FIREFOX_NATIVE_ARTIFACTS").map(PathBuf::from);
    let view = action.unwrap_or(if single_profile {
        "single-profile"
    } else {
        "nonselected-profile"
    });
    browser.popup()?;
    if single_profile {
        ensure!(
            browser.driver.script(
                "return document.querySelector('#choose').hidden;",
                json!([])
            )? == true,
            "A single Firefox profile must be selected without an extra choice"
        );
    }
    if let Some(out) = &artifacts {
        capture_native(&browser.driver, out, 10, &format!("{view}-popup"))?;
    }
    browser.driver.click("#settings")?;
    browser.driver.close_popup()?;
    let deadline = Instant::now() + Duration::from_secs(15);
    let manager_handle = loop {
        if let Some(handle) = handles()?
            .into_iter()
            .find(|handle| !before_handles.contains(handle))
        {
            break handle;
        }
        ensure!(
            Instant::now() < deadline,
            "Settings did not open in this Firefox profile"
        );
        thread::sleep(Duration::from_millis(100));
    };
    browser
        .driver
        .call("/window", json!({"handle":manager_handle}))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let ready = browser.driver.script("return location.origin.startsWith('http://127.0.0.1:') && !!document.querySelector('#doctor') && !document.querySelector('#doctor').disabled && /Installed|connected/i.test(document.querySelector('#version')?.textContent || '');", json!([]))?;
        if ready == true {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "Settings page did not become ready"
        );
        thread::sleep(Duration::from_millis(100));
    }
    if let Some(out) = &artifacts {
        capture_native(&browser.driver, out, 11, &format!("{view}-settings"))?;
    }
    browser.driver.click("#doctor")?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let result = browser.driver.script(
            "return document.querySelector('#result').textContent;",
            json!([]),
        )?;
        if result.as_str().is_some_and(|text| {
            text.contains("Codex connection: registered")
                && text.contains("Local helper: registered")
        }) {
            break;
        }
        ensure!(Instant::now() < deadline, "Settings check failed: {result}");
        thread::sleep(Duration::from_millis(100));
    }
    if let Some(out) = &artifacts {
        browser.driver.script(
            "document.querySelector('#result').scrollIntoView({block:'center'});",
            json!([]),
        )?;
        capture_native(&browser.driver, out, 12, &format!("{view}-check-result"))?;
    }
    ensure!(
        client.ok("firefox_tabs", json!({}))?["tabs"] == before_tabs,
        "Settings must not become a chat-owned work tab"
    );
    if let Some(action) = action {
        browser.driver.click(if action == "uninstall" {
            "#uninstall"
        } else {
            "#install"
        })?;
        browser.driver.click("#confirm-yes")?;
        let expected = if action == "uninstall" {
            "Connection removed."
        } else {
            "Connection installed."
        };
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let result = browser.driver.script(
                "return document.querySelector('#result').textContent;",
                json!([]),
            )?;
            if result.as_str().is_some_and(|text| {
                text.contains(expected)
                    || (env::var_os("FIREFOX_SETUP_PACKAGE").is_some()
                        && if action == "uninstall" {
                            text.contains("Disconnected this user from Codex")
                        } else {
                            text.contains("Connected to Codex")
                        })
            }) {
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "Settings {action} response failed: {result}"
            );
            thread::sleep(Duration::from_millis(100));
        }
        if let Some(out) = &artifacts {
            capture_native(&browser.driver, out, 13, &format!("{view}-response"))?;
        }
    }
    browser.driver.click("#close")?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let result = browser.driver.script(
            "return document.querySelector('#result').textContent;",
            json!([]),
        )?;
        if result
            .as_str()
            .is_some_and(|text| text.contains("Setup service closed"))
        {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "Settings service did not close: {result}"
        );
        thread::sleep(Duration::from_millis(100));
    }
    browser
        .driver
        .client
        .delete(format!(
            "{}/session/{}/window",
            browser.driver.base, browser.driver.session
        ))
        .send()?
        .error_for_status()?;
    browser.driver.call("/window", json!({"handle":original}))?;
    ensure!(
        handles()?.len() == before_handles.len(),
        "Settings left an extra tab"
    );
    Ok(())
}
#[test]
#[ignore = "launches actual Firefox with system access in disposable profiles"]
fn extension_first_install_multi_profile_and_restart() -> Result<()> {
    let root = tempfile::tempdir()?;
    let home = root.path();
    let fixture = Fixture::start()?;
    let browser = Browser::start(home, &home.join("profile-a"))?;
    installed(home, &browser)?;
    if env::var_os("FIREFOX_SIGNED_UI").is_none() {
        browser.addon()?;
    }
    let mut a = server(home)?;
    let mut b = server(home)?;
    a.wait_connected(true)?;
    if env::var_os("FIREFOX_NATIVE_APP_SERVER").is_some() {
        codex_app_server_roundtrip(home, &fixture, &browser)?;
    }
    // No profile/code/token/tab-grant input. Only a newly created work tab is visible.
    let baseline = browser.driver.handle()?;
    browser.driver.goto(&fixture.url)?;
    ensure!(a.ok("firefox_tabs", json!({}))?["tabs"] == json!([]));
    let opened = a.ok("firefox_open_tab", json!({"url":fixture.url}))?;
    let handles: Value = browser
        .driver
        .client
        .get(format!(
            "{}/session/{}/window/handles",
            browser.driver.base, browser.driver.session
        ))
        .send()?
        .json()?;
    let own_handle = handles["value"]
        .as_array()
        .context("handles")?
        .iter()
        .find(|h| h.as_str() != Some(&baseline))
        .context("own handle")?
        .clone();
    b.denied(
        "firefox_select_tab",
        json!({"tabId":opened["tabId"]}),
        "not owned",
    )?;
    b.ok("firefox_open_tab", json!({"url":fixture.url}))?;
    let before = wait_snapshot(&mut a)?;
    a.ok("firefox_click", action(&before, "Count 0")?)?;
    let after = wait_snapshot(&mut a)?;
    ensure!(after["text"]
        .as_str()
        .is_some_and(|s| s.contains("Count 1")));
    ensure!(wait_snapshot(&mut b)?["text"]
        .as_str()
        .is_some_and(|s| s.contains("Count 0")));
    let mut typing = action(&after, "Message")?;
    typing["text"] = json!("native connection verified");
    a.ok("firefox_type", typing)?;
    // Independent browser readback of the own tab; baseline remains untouched.
    browser
        .driver
        .call("/window", json!({"handle":own_handle}))?;
    ensure!(
        browser.driver.script(
            "return document.querySelector('#message').value;",
            json!([])
        )? == "native connection verified"
    );
    browser.driver.call("/window", json!({"handle":baseline}))?;
    ensure!(
        browser.driver.script(
            "return document.querySelector('#count').textContent;",
            json!([])
        )? == "Count 0"
    );
    settings_from_popup(&browser, &mut a, &baseline, true, None)?;
    ensure!(
        browser.driver.script(
            "return document.querySelector('#count').textContent;",
            json!([])
        )? == "Count 0"
    );
    a.ok(
        "firefox_navigate",
        json!({"url":format!("{}/next",fixture.url)}),
    )?;
    a.denied("firefox_click", action(&after, "Count 1")?, "snapshot")?;
    wait_snapshot(&mut a)?;
    // Actual extension control buttons revoke both chats, pause and resume.
    browser.popup()?;
    browser.driver.click("#toggle")?;
    browser.driver.wait_status("Paused")?;
    browser.driver.close_popup()?;
    ensure!(a.ok("firefox_status", json!({}))?["enabled"] == false);
    b.denied("firefox_snapshot", json!({}), "released")?;
    browser.driver.popup_for(NATIVE_ID)?;
    browser.driver.click("#toggle")?;
    browser.driver.wait_status("Connected")?;
    browser.driver.close_popup()?;
    a.denied("firefox_snapshot", json!({}), "No AI work tab")?;
    a.ok("firefox_open_tab", json!({"url":fixture.url}))?;
    wait_snapshot(&mut a)?;
    // Update while Firefox remains alive. The native helper must reconnect with
    // no old tab access; only the installed executable is atomically replaced.
    drop(a);
    drop(b);
    let update = command(&installer_binary(), home)
        .arg("--update")
        .output()?;
    ensure!(
        update.status.success(),
        "update: {}",
        String::from_utf8_lossy(&update.stderr)
    );
    let mut a = server(home)?;
    let b = server(home)?;
    a.wait_connected(true)?;
    browser.driver.call("/window", json!({"handle":baseline}))?;
    settings_from_popup(&browser, &mut a, &baseline, true, Some("update"))?;
    ensure!(
        browser.driver.script(
            "return document.querySelector('#count').textContent;",
            json!([])
        )? == "Count 0"
    );
    // Updates revoke running tool connections. A new chat/process reconnects;
    // the Settings HTTP response and unrelated browser page already survived.
    drop(a);
    drop(b);
    let mut a = server(home)?;
    let b = server(home)?;
    a.wait_connected(true)?;
    ensure!(a.ok("firefox_status", json!({}))?["ownedTabs"] == 0);
    a.ok("firefox_open_tab", json!({"url":fixture.url}))?;
    wait_snapshot(&mut a)?;
    if env::var_os("FIREFOX_NATIVE_IDLE_CHECK").is_some() {
        thread::sleep(Duration::from_secs(65));
        wait_snapshot(&mut a)?;
    }
    codex_workflow(home, &browser, &fixture)?;
    // Second profile does not steal the selected connection.
    let second = Browser::start(home, &home.join("profile-b"))?;
    second.addon()?;
    second.driver.goto(&fixture.url)?;
    let second_baseline = second.driver.handle()?;
    settings_from_popup(&second, &mut a, &second_baseline, false, None)?;
    second.popup()?;
    second.driver.click("#choose")?;
    second.driver.close_popup()?;
    thread::sleep(Duration::from_millis(250));
    a.denied("firefox_snapshot", json!({}), "No AI work tab")?;
    a.ok("firefox_open_tab", json!({"url":fixture.url}))?;
    wait_snapshot(&mut a)?;
    let selected = a.ok("firefox_status", json!({}))?["profileId"].clone();
    drop(second);
    a.wait_connected(false)?;
    a.denied(
        "firefox_open_tab",
        json!({"url":fixture.url}),
        "selected Firefox is offline",
    )?;
    // Restart exactly the selected disposable profile. Temporary addon explicitly
    // reloaded; this verifies transport reconnection, not unsigned persistence.
    let restarted = Browser::start(home, &home.join("profile-b"))?;
    if env::var_os("FIREFOX_SIGNED_XPI").is_none() {
        restarted.addon()?;
    }
    a.wait_connected(true)?;
    ensure!(a.ok("firefox_status", json!({}))?["profileId"] == selected);
    a.denied("firefox_snapshot", json!({}), "No AI work tab")?;
    a.ok("firefox_open_tab", json!({"url":fixture.url}))?;
    wait_snapshot(&mut a)?;
    a.ok("firefox_revoke", json!({}))?;
    a.denied("firefox_snapshot", json!({}), "No AI work tab")?;
    restarted.popup()?;
    if let Ok(out) = env::var("FIREFOX_NATIVE_ARTIFACTS") {
        let out = PathBuf::from(out);
        fs::create_dir_all(&out)?;
        capture_native(&restarted.driver, &out, 1, "native-popup")?;
        fs::write(
            out.join("result.json"),
            serde_json::to_vec_pretty(
                &json!({"firstInstall":true,"signedInstallUi":env::var_os("FIREFOX_SIGNED_UI").is_some(),"singleProfileAutomaticSelection":true,"extensionSettingsInSameFirefoxProfile":true,"settingsExcludedFromWorkTabs":true,"settingsUpdateResponseSurvivesNativeReset":true,"nonselectedProfileSettings":true,"twoChats":true,"multiProfile":true,"offlineNoFallback":true,"restartReconnect":true,"temporaryAddonReloaded":env::var_os("FIREFOX_SIGNED_XPI").is_none(),"permanentSignedRestartVerified":env::var_os("FIREFOX_SIGNED_XPI").is_some()}),
            )?,
        )?;
    }
    restarted.driver.close_popup()?;
    restarted.driver.goto(&fixture.url)?;
    let restarted_baseline = restarted.driver.handle()?;
    settings_from_popup(
        &restarted,
        &mut a,
        &restarted_baseline,
        false,
        Some("uninstall"),
    )?;
    ensure!(
        restarted.driver.script(
            "return document.querySelector('#count').textContent;",
            json!([])
        )? == "Count 0"
    );
    ensure!(!home
        .join(".local/lib/firefox-codex-mcp/firefox-codex-mcp")
        .exists());
    ensure!(!fs::read_to_string(home.join(".codex/config.toml"))?.contains("mcp_servers.firefox"));
    if let Ok(out) = env::var("FIREFOX_NATIVE_ARTIFACTS") {
        fs::write(
            Path::new(&out).join("settings-removal-result.json"),
            serde_json::to_vec_pretty(
                &json!({"responseDeliveredAfterNativeReset":true,"serviceClosed":true,"firefoxFixturePreserved":true,"registrationRemoved":true}),
            )?,
        )?;
    }
    drop(restarted);
    drop(browser);
    drop(a);
    drop(b);
    Ok(())
}

// The real Codex app-server dispatches MCP without an API account. This proves
// process/config/metadata integration, independently of model tool selection.
struct AppServer {
    _process: Daemon,
    input: ChildStdin,
    output: Receiver<std::io::Result<String>>,
    seq: u64,
}
impl AppServer {
    fn start(home: &Path) -> Result<Self> {
        let binary = env::var("FIREFOX_SETUP_CODEX_BINARY")?;
        // HOME is an ordinary documented MCP environment setting here: Codex
        // otherwise replaces it with the disposable runner account's real HOME.
        // The helper and Firefox share this test's temp HOME, not a private IPC override.
        let isolated_home = format!(
            "mcp_servers.firefox.env={{HOME={}}}",
            serde_json::to_string(home)?
        );
        let mut process = Daemon(
            command(Path::new(&binary), home)
                .args(["app-server", "-c", &isolated_home])
                .current_dir(home)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()?,
        );
        let input = process.0.stdin.take().context("Codex app-server stdin")?;
        let stdout = process.0.stdout.take().context("Codex app-server stdout")?;
        let (tx, output) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut app = Self {
            _process: process,
            input,
            output,
            seq: 0,
        };
        app.rpc("initialize", json!({"clientInfo":{"name":"firefox_linux_package_e2e","version":"1"},"capabilities":{"experimentalApi":true}}))?;
        writeln!(app.input, "{}", json!({"method":"initialized","params":{}}))?;
        app.input.flush()?;
        Ok(app)
    }
    fn rpc(&mut self, method: &str, params: Value) -> Result<Value> {
        self.seq += 1;
        writeln!(
            self.input,
            "{}",
            json!({"id":self.seq,"method":method,"params":params})
        )?;
        self.input.flush()?;
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            let line = self
                .output
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))??;
            let message: Value = serde_json::from_str(&line)?;
            if message["id"] == self.seq && message.get("method").is_none() {
                ensure!(message.get("error").is_none(), "Codex {method}: {message}");
                return Ok(message["result"].clone());
            }
            ensure!(
                message.get("id").is_none() || message.get("method").is_none(),
                "Unexpected Codex app-server request: {message}"
            );
        }
    }
    fn chat(&mut self, home: &Path) -> Result<String> {
        let result = self.rpc("thread/start", json!({"cwd":home,"ephemeral":true,"model":"gpt-5","approvalPolicy":"never","sandbox":"read-only"}))?;
        Ok(result["thread"]["id"]
            .as_str()
            .context("Codex chat ID")?
            .into())
    }
    fn raw(&mut self, chat: &str, name: &str, arguments: Value) -> Result<Value> {
        self.rpc(
            "mcpServer/tool/call",
            json!({"threadId":chat,"server":"firefox","tool":name,"arguments":arguments}),
        )
    }
    fn ok(&mut self, chat: &str, name: &str, arguments: Value) -> Result<Value> {
        let result = self.raw(chat, name, arguments)?;
        ensure!(result["isError"] != true, "Codex {name}: {result}");
        Ok(serde_json::from_str(
            result["content"][0]["text"]
                .as_str()
                .context("Codex tool content")?,
        )?)
    }
}
fn codex_app_server_roundtrip(home: &Path, fixture: &Fixture, browser: &Browser) -> Result<()> {
    let artifacts = PathBuf::from(env::var("FIREFOX_NATIVE_ARTIFACTS")?);
    fs::create_dir_all(&artifacts)?;
    let mut app = AppServer::start(home)?;
    let a = app.chat(home)?;
    let b = app.chat(home)?;
    let tools = app.rpc(
        "mcpServerStatus/list",
        json!({"threadId":a,"serverName":"firefox"}),
    )?;
    fs::write(
        artifacts.join("codex-app-server-tools.json"),
        serde_json::to_vec_pretty(&tools)?,
    )?;
    let status = app.ok(&a, "firefox_status", json!({}))?;
    ensure!(
        status["connected"] == true,
        "Codex package connection: {status}"
    );
    let url = format!("{}/codex-package", fixture.url);
    let tab = app.ok(&a, "firefox_open_tab", json!({"url":url}))?["tabId"].clone();
    let deadline = Instant::now() + Duration::from_secs(15);
    let snapshot = loop {
        let result = app.raw(&a, "firefox_snapshot", json!({}))?;
        if result["isError"] != true {
            let value: Value = serde_json::from_str(
                result["content"][0]["text"]
                    .as_str()
                    .context("Codex snapshot")?,
            )?;
            if value["text"]
                .as_str()
                .is_some_and(|text| text.contains("Observation code"))
            {
                break value;
            }
        }
        ensure!(
            Instant::now() < deadline,
            "Codex package snapshot unavailable: {result}"
        );
        thread::sleep(Duration::from_millis(100));
    };
    app.ok(&a, "firefox_click", action(&snapshot, "Count 0")?)?;
    let snapshot = app.ok(&a, "firefox_snapshot", json!({}))?;
    let mut typing = action(&snapshot, "Message")?;
    typing["text"] = json!("Codex app-server Linux package");
    app.ok(&a, "firefox_type", typing)?;
    let handles: Value = browser
        .driver
        .client
        .get(format!(
            "{}/session/{}/window/handles",
            browser.driver.base, browser.driver.session
        ))
        .send()?
        .json()?;
    let mut verified = false;
    for handle in handles["value"]
        .as_array()
        .context("Codex Firefox handles")?
    {
        browser.driver.call("/window", json!({"handle":handle}))?;
        if browser.driver.script("return location.href;", json!([]))? == url {
            let dom = browser.driver.script("return {count:document.querySelector('#count').textContent,message:document.querySelector('#message').value};", json!([]))?;
            ensure!(
                dom["count"] == "Count 1" && dom["message"] == "Codex app-server Linux package",
                "Codex DOM readback: {dom}"
            );
            capture_native(&browser.driver, &artifacts, 40, "codex-app-server-actions")?;
            verified = true;
        }
    }
    ensure!(verified, "Codex package work tab missing");
    ensure!(
        app.raw(&b, "firefox_select_tab", json!({"tabId":tab}))?["isError"] == true,
        "Codex package chat isolation failed"
    );
    app.ok(&a, "firefox_revoke", json!({}))?;
    ensure!(
        app.raw(&a, "firefox_snapshot", json!({}))?["isError"] == true,
        "Codex package revoke failed"
    );
    fs::write(
        artifacts.join("codex-app-server-result.json"),
        serde_json::to_vec_pretty(
            &json!({"actualCodexProcess":true,"actualFirefox":true,"packageManagedBinary":env::var_os("FIREFOX_SETUP_PACKAGE").is_some(),"systemNativeManifest":env::var_os("FIREFOX_SETUP_PACKAGE").is_some(),"read":true,"click":true,"type":true,"independentDomReadback":true,"chatIsolation":true,"revoke":true,"desktopUi":false,"llmDispatch":false}),
        )?,
    )?;
    Ok(())
}

fn codex_workflow(home: &Path, browser: &Browser, fixture: &Fixture) -> Result<()> {
    let Some(binary) = env::var_os("FIREFOX_NATIVE_CODEX_E2E") else {
        return Ok(());
    };
    let url = format!("{}/codex-native", fixture.url);
    let config = format!(
        "mcp_servers.firefox={{command={},args=[\"--extension\"],env={{HOME={}}},required=true}}",
        serde_json::to_string(&installed_binary(home))?,
        serde_json::to_string(home)?
    );
    let prompt=format!("Authorized integration test on a disposable local Firefox fixture. Use only firefox MCP tools; no shell, files, other tools or external sites. Open {url} as your own work tab. Read the random Observation code from rendered body text. Click Count 0 exactly once; then take a fresh snapshot and verify status Count updated to 1. Type the exact observed code into Message. Read a new snapshot, scroll down 600 pixels, read again. Use fresh snapshot and element IDs. Retry stale BEFORE-dispatch errors at most twice, never unknown outcomes. Finally call firefox_revoke and confirm firefox_snapshot is rejected. Report the code and result.");
    let mut process = Daemon(
        Command::new(binary)
            .args([
                "exec",
                "--ignore-user-config",
                "--ephemeral",
                "--skip-git-repo-check",
                "--sandbox",
                "read-only",
                "--json",
                "--color",
                "never",
                "-c",
                "approval_policy=\"on-request\"",
                "-c",
                "approvals_reviewer=\"auto_review\"",
                "-c",
                &config,
            ])
            .arg(prompt)
            .current_dir(home)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?,
    );
    let artifacts = PathBuf::from(
        env::var("FIREFOX_NATIVE_ARTIFACTS")
            .context("Set FIREFOX_NATIVE_ARTIFACTS for model E2E")?,
    );
    fs::create_dir_all(&artifacts)?;
    let mut stderr = process.0.stderr.take().context("stderr")?;
    let mut log = fs::File::create(artifacts.join("codex.stderr.log"))?;
    thread::spawn(move || {
        let _ = std::io::copy(&mut stderr, &mut log);
    });
    let stdout = process.0.stdout.take().context("stdout")?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut transcript = fs::File::create(artifacts.join("codex.jsonl"))?;
    let mut calls = std::collections::HashSet::new();
    let mut done = false;
    let deadline = Instant::now() + Duration::from_secs(240);
    loop {
        ensure!(Instant::now() < deadline, "Codex workflow timed out");
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(line) => {
                let line = line?;
                writeln!(transcript, "{line}")?;
                let e: Value = serde_json::from_str(&line)?;
                if e["type"] == "item.completed" && e["item"]["type"] == "mcp_tool_call" {
                    let item = &e["item"];
                    eprintln!("Native Codex tool {} {}", item["tool"], item["status"]);
                    if item["status"] == "completed"
                        && item["error"].is_null()
                        && item["result"]["isError"] != true
                    {
                        calls.insert(item["tool"].as_str().unwrap_or_default().to_owned());
                    }
                }
                if e["type"] == "turn.completed" {
                    done = true;
                }
                ensure!(
                    e["type"] != "turn.failed" && e["type"] != "error",
                    "Codex failure: {e}"
                );
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    ensure!(process.0.wait()?.success() && done, "Codex incomplete");
    for tool in [
        "firefox_open_tab",
        "firefox_snapshot",
        "firefox_click",
        "firefox_type",
        "firefox_scroll",
        "firefox_revoke",
    ] {
        ensure!(calls.contains(tool), "Missing actual Codex tool {tool}");
    }
    let handles: Value = browser
        .driver
        .client
        .get(format!(
            "{}/session/{}/window/handles",
            browser.driver.base, browser.driver.session
        ))
        .send()?
        .json()?;
    let mut found = false;
    for handle in handles["value"].as_array().context("handles")? {
        browser.driver.call("/window", json!({"handle":handle}))?;
        let current = browser.driver.script("return location.href;", json!([]))?;
        if current == url {
            let result=browser.driver.script("return {count:document.querySelector('#count').textContent,input:document.querySelector('#message').value,code:document.querySelector('#observation-code').textContent.split(': ')[1],scrollY};",json!([]))?;
            ensure!(
                result["count"] == "Count 1"
                    && result["input"] == result["code"]
                    && result["scrollY"].as_f64().is_some_and(|v| v > 0.),
                "Actual Codex DOM readback: {result}"
            );
            fs::write(
                artifacts.join("codex-result.json"),
                serde_json::to_vec_pretty(&result)?,
            )?;
            found = true;
            break;
        }
    }
    ensure!(found, "Actual Codex did not leave its result tab");
    Ok(())
}
