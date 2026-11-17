#![cfg(target_os = "linux")]
//! Real package-manager checks. Explicitly opt in on a disposable CI machine only.
//! Never run this ignored test on a developer's ordinary workstation.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Output, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const EXECUTABLE: &str = "/usr/libexec/firefox-codex-mcp/firefox-codex-mcp";
const MARKER: &str = "/usr/libexec/firefox-codex-mcp/installation.json";
const MANIFEST: &str = "/usr/lib/mozilla/native-messaging-hosts/firefox_codex_mcp.json";
const EXTENSION: &str = "{a171de16-96b6-4e50-94b7-e0b54d94436a}";
const CONFIG: &str = "# keep this comment\nmodel = \"test-model\"\n[mcp_servers.other]\ncommand = \"/bin/true\"\nargs = []\n";
const RULES: &str = "Keep my existing Rust preference.\n";
const PROTECTED: &[&str] = &[
    ".config/mozilla/firefox/profiles.ini",
    ".config/mozilla/firefox/main/prefs.js",
    ".config/mozilla/firefox/main/user.js",
    ".config/mozilla/firefox/main/logins.json",
    ".config/mozilla/firefox/main/key4.db",
    ".config/mozilla/firefox/main/cookies.sqlite",
];

fn checked(command: &mut Command) -> Result<Output> {
    let out = command.output()?;
    ensure!(
        out.status.success(),
        "Command {command:?} failed: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(out)
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn write(path: &Path, bytes: impl AsRef<[u8]>) -> Result<()> {
    fs::create_dir_all(path.parent().context("fixture parent")?)?;
    fs::write(path, bytes)?;
    Ok(())
}

struct Account {
    name: String,
    home: PathBuf,
    codex: PathBuf,
    protected: Vec<(String, String)>,
}
impl Account {
    fn create(root: &Path, suffix: &str) -> Result<Self> {
        let name = format!("fcmcp{}{}", std::process::id(), suffix);
        let home = root.join(&name);
        fs::create_dir(&home)?;
        checked(Command::new("useradd").args([
            "--user-group",
            "--no-create-home",
            "--home-dir",
            home.to_str().context("home path")?,
            "--shell",
            "/bin/sh",
            &name,
        ]))?;
        let mut account = Self {
            name,
            codex: home.join(if suffix == "b" {
                ".codex-custom"
            } else {
                ".codex"
            }),
            home,
            protected: Vec::new(),
        };
        write(&account.codex.join("config.toml"), CONFIG)?;
        write(&account.codex.join("AGENTS.md"), RULES)?;
        for path in PROTECTED {
            let bytes = if path.ends_with("profiles.ini") {
                "[Profile0]\nIsRelative=1\nPath=main\nDefault=1\n".to_owned()
            } else {
                format!("Synthetic protected fixture; no real personal data: {path}\n")
            };
            write(&account.home.join(path), &bytes)?;
            account
                .protected
                .push(((*path).into(), sha(bytes.as_bytes())));
        }
        account.own_files()?;
        Ok(account)
    }
    fn own_files(&self) -> Result<()> {
        checked(Command::new("chown").args([
            "--recursive",
            &format!("{}:{}", self.name, self.name),
            self.home.to_str().context("home")?,
        ]))?;
        Ok(())
    }
    fn command(&self, binary: impl AsRef<Path>) -> Result<Command> {
        let mut c = Command::new("runuser");
        c.args(["--user", &self.name, "--", "env", "-i"])
            .arg(format!("HOME={}", self.home.display()))
            .arg(format!("CODEX_HOME={}", self.codex.display()))
            .arg(format!("PATH={}", env::var("PATH")?))
            .arg("LANG=C.UTF-8")
            .arg(format!(
                "FIREFOX_MCP_CODEX_BINARY={}",
                env::var("FIREFOX_PACKAGE_CODEX_BINARY")?
            ))
            .arg(binary.as_ref())
            .current_dir(&self.home);
        Ok(c)
    }
    fn run(&self, binary: impl AsRef<Path>, argument: &str) -> Result<Output> {
        checked(self.command(binary)?.arg(argument))
    }
    fn protected_unchanged(&self) -> Result<()> {
        for (path, expected) in &self.protected {
            ensure!(
                sha(&fs::read(self.home.join(path))?) == *expected,
                "Protected Firefox/login fixture changed for {}: {path}",
                self.name
            );
        }
        Ok(())
    }
    fn config(&self) -> Result<String> {
        Ok(fs::read_to_string(self.codex.join("config.toml"))?)
    }
    fn connection(&self) -> Result<()> {
        let config = self.config()?;
        ensure!(config.contains(EXECUTABLE) && config.contains("--extension"));
        ensure!(
            config.contains("# keep this comment")
                && config.contains("model = \"test-model\"")
                && config.contains("[mcp_servers.other]")
                && config.contains("args = []"),
            "Unrelated TOML changed: {config}"
        );
        ensure!(!self
            .home
            .join(".local/lib/firefox-codex-mcp/firefox-codex-mcp")
            .exists());
        ensure!(!self
            .home
            .join(".mozilla/native-messaging-hosts/firefox_codex_mcp.json")
            .exists());
        for launcher in ["firefox-codex.desktop", "firefox-codex-manager.desktop"] {
            ensure!(!self
                .home
                .join(".local/share/applications")
                .join(launcher)
                .exists());
        }
        let out = self.run(EXECUTABLE, "--doctor")?;
        let doctor: Value = serde_json::from_slice(&out.stdout)?;
        for key in [
            "globalMcpRegistered",
            "binaryInstalled",
            "nativeHostRegistered",
            "skillInstalled",
            "automaticBrowserGuidance",
        ] {
            ensure!(doctor[key] == true, "Package doctor {key}: {doctor}");
        }
        self.protected_unchanged()
    }
    fn disconnected(&self) -> Result<()> {
        let config = self.config()?;
        ensure!(!config.contains("mcp_servers.firefox") && config.contains("args = []"));
        ensure!(config.contains("mcp_servers.other") && config.contains("keep this comment"));
        let rules = fs::read_to_string(self.codex.join("AGENTS.md"))?;
        ensure!(rules.starts_with(RULES) && !rules.contains("firefox-codex-mcp:start"));
        self.protected_unchanged()
    }
}
impl Drop for Account {
    fn drop(&mut self) {
        let _ = Command::new("userdel").arg(&self.name).status();
    }
}

struct Process {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<Result<Value>>,
}
impl Process {
    fn start(account: &Account, argument: &str, native: bool) -> Result<Self> {
        let mut command = if native {
            let manifest: Value = serde_json::from_slice(&fs::read(MANIFEST)?)?;
            ensure!(manifest["allowed_extensions"] == json!([EXTENSION]));
            let wrapper = Path::new(manifest["path"].as_str().context("system native wrapper")?);
            ensure!(fs::metadata(wrapper)?.permissions().mode() & 0o022 == 0);
            account.command(wrapper)?
        } else {
            account.command(EXECUTABLE)?
        };
        if native {
            command.arg(MANIFEST).arg(EXTENSION);
        } else {
            command.arg(argument);
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let mut input = child.stdin.take().context("process stdin")?;
        let stdout = child.stdout.take().context("process stdout")?;
        let (tx, rx) = mpsc::channel();
        if native {
            let hello = serde_json::to_vec(
                &json!({"version":1,"role":"extension","profileId":"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb","capabilities":["chat-continuity-v1"]}),
            )?;
            input.write_all(&(hello.len() as u32).to_le_bytes())?;
            input.write_all(&hello)?;
            input.flush()?;
            thread::spawn(move || {
                let mut stdout = stdout;
                let mut size = [0_u8; 4];
                let ready = (|| -> Result<Value> {
                    stdout.read_exact(&mut size)?;
                    let size = u32::from_le_bytes(size) as usize;
                    ensure!(size < 1024 * 1024, "Native frame too large");
                    let mut bytes = vec![0; size];
                    stdout.read_exact(&mut bytes)?;
                    Ok(serde_json::from_slice(&bytes)?)
                })();
                let _ = tx.send(ready);
                let _ = std::io::copy(&mut stdout, &mut std::io::sink());
            });
        } else {
            if argument == "--extension" {
                writeln!(
                    input,
                    "{}",
                    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"package-ci","version":"1"}}})
                )?;
                input.flush()?;
            }
            thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let message = line.map_err(Into::into).and_then(|line| {
                        if argument_is_manager_line(&line) {
                            Ok(json!({"managerReady":true}))
                        } else {
                            serde_json::from_str(&line).map_err(Into::into)
                        }
                    });
                    if tx.send(message).is_err() {
                        break;
                    }
                }
            });
        }
        let ready = rx.recv_timeout(Duration::from_secs(15))??;
        ensure!(
            ready["kind"] == "ready"
                || ready["result"]["serverInfo"]["name"] == "firefox-codex-mcp"
                || ready["managerReady"] == true,
            "Unexpected process readiness: {ready}"
        );
        Ok(Self {
            child,
            input,
            output: rx,
        })
    }
    fn status(&mut self, expect_error: bool) -> Result<()> {
        let write = writeln!(self.input, "{}", json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"firefox_status","arguments":{}}})).and_then(|_| self.input.flush());
        if write.is_err() && expect_error {
            return self.exited("user Disconnect");
        }
        write?;
        match self.output.recv_timeout(Duration::from_secs(8)) {
            Ok(Ok(reply)) => ensure!(
                (reply.get("error").is_some() || reply["result"]["isError"] == true)
                    == expect_error,
                "Unexpected existing MCP response after Disconnect: {reply}"
            ),
            Err(mpsc::RecvTimeoutError::Disconnected) if expect_error => {
                self.exited("user Disconnect")?
            }
            other => anyhow::bail!("Existing MCP did not complete: {other:?}"),
        }
        Ok(())
    }
    fn exited(&mut self, reason: &str) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if self.child.try_wait()?.is_some() {
                return Ok(());
            }
            ensure!(Instant::now() < deadline, "Old helper survived {reason}");
            thread::sleep(Duration::from_millis(25));
        }
    }
}
fn argument_is_manager_line(line: &str) -> bool {
    line.starts_with("Firefox Codex setup: http://127.0.0.1:")
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Manager {
    format: String,
}
impl Manager {
    fn install(&self, package: &Path) -> Result<()> {
        if self.format == "deb" {
            checked(Command::new("dpkg").arg("--install").arg(package))?;
        } else {
            checked(
                Command::new("dnf")
                    .args(["--assumeyes", "install"])
                    .arg(package),
            )?;
        }
        Ok(())
    }
    fn reinstall(&self, package: &Path) -> Result<()> {
        if self.format == "deb" {
            self.install(package)
        } else {
            checked(
                Command::new("dnf")
                    .args(["--assumeyes", "reinstall"])
                    .arg(package),
            )?;
            Ok(())
        }
    }
    fn remove(&self) -> Result<()> {
        if self.format == "deb" {
            checked(Command::new("dpkg").args(["--remove", "firefox-codex-mcp"]))?;
        } else {
            checked(Command::new("dnf").args(["--assumeyes", "remove", "firefox-codex-mcp"]))?;
        }
        Ok(())
    }
}
fn generation() -> Result<String> {
    let marker: Value = serde_json::from_slice(&fs::read(MARKER)?)?;
    ensure!(marker["schema"] == 1);
    ensure!(marker["executable_sha256"] == sha(&fs::read(EXECUTABLE)?));
    let generation = marker["generation"]
        .as_str()
        .context("package generation")?;
    ensure!(generation.len() >= 32, "Predictable generation marker");
    Ok(generation.into())
}
fn registrations(account: &Account) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    [
        account.codex.join("config.toml"),
        account.codex.join("AGENTS.md"),
        account.codex.join("skills/firefox-browser/SKILL.md"),
        account
            .home
            .join(".local/state/firefox-codex-mcp/install.json"),
    ]
    .into_iter()
    .map(|path| Ok((path.clone(), fs::read(path)?)))
    .collect()
}
fn brokers(account: &Account) -> Result<Vec<PathBuf>> {
    let uid = fs::metadata(&account.home)?.uid();
    let mut processes = Vec::new();
    for entry in fs::read_dir("/proc")? {
        let path = entry?.path();
        if fs::metadata(&path).is_ok_and(|m| m.uid() == uid)
            && fs::read(path.join("cmdline")).is_ok_and(|args| {
                args.starts_with(format!("{EXECUTABLE}\0--extension-broker\0").as_bytes())
            })
        {
            processes.push(path);
        }
    }
    ensure!(
        !processes.is_empty(),
        "Native host did not start a package broker"
    );
    Ok(processes)
}
fn brokers_exited(processes: &[PathBuf]) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while processes
        .iter()
        .any(|p| fs::read_link(p.join("exe")).is_ok())
    {
        ensure!(
            Instant::now() < deadline,
            "Old package broker survived generation invalidation"
        );
        thread::sleep(Duration::from_millis(25));
    }
    Ok(())
}

#[test]
#[ignore = "installs/removes real OS packages; disposable CI root only"]
fn real_package_lifecycle_preserves_users_and_revokes_running_processes() -> Result<()> {
    ensure!(env::var("FIREFOX_PACKAGE_TEST_DISPOSABLE").as_deref() == Ok("1"));
    ensure!(
        env::var("GITHUB_ACTIONS").as_deref() == Ok("true")
            || Path::new("/run/.containerenv").exists()
            || Path::new("/.dockerenv").exists(),
        "This test is restricted to disposable CI/container environments"
    );
    ensure!(
        unsafe { libc::geteuid() } == 0,
        "Package test requires disposable root"
    );
    ensure!(
        !Path::new(EXECUTABLE).exists(),
        "Refusing to replace an existing host installation"
    );
    let manager = Manager {
        format: env::var("FIREFOX_PACKAGE_FORMAT")?,
    };
    ensure!(matches!(manager.format.as_str(), "deb" | "rpm"));
    let baseline = PathBuf::from(env::var("FIREFOX_PACKAGE_BASELINE")?);
    let candidate = PathBuf::from(env::var("FIREFOX_PACKAGE_CANDIDATE")?);
    let portable = PathBuf::from(env::var("FIREFOX_PACKAGE_PORTABLE_BINARY")?);
    let legacy = PathBuf::from(env::var("FIREFOX_PACKAGE_LEGACY_BINARY")?);
    let evidence = PathBuf::from(env::var("FIREFOX_PACKAGE_ARTIFACTS")?);
    fs::create_dir_all(&evidence)?;
    let root = tempfile::tempdir()?;
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755))?;
    let a = Account::create(root.path(), "a")?;
    let b = Account::create(root.path(), "b")?;
    // A real published portable release must migrate without a second helper.
    for account in [&a, &b] {
        account.run(&legacy, "--install")?;
        // The published old CLI normalized empty arrays. A subsequent user
        // edit must be preserved by the new targeted migration/removal path.
        let mut config = account.config()?.parse::<toml_edit::DocumentMut>()?;
        config["mcp_servers"]["other"]["args"] = toml_edit::value(toml_edit::Array::new());
        fs::write(account.codex.join("config.toml"), config.to_string())?;
        ensure!(account
            .home
            .join(".local/lib/firefox-codex-mcp/firefox-codex-mcp")
            .is_file());
        account.protected_unchanged()?;
    }
    let old_b = b
        .home
        .join(".local/lib/firefox-codex-mcp/firefox-codex-mcp");
    let old_b_bytes = fs::read(&old_b)?;
    let mut changed_b = old_b_bytes.clone();
    changed_b.extend_from_slice(b"user modification");
    fs::write(&old_b, &changed_b)?;
    manager.install(&baseline)?;
    let initial = generation()?;
    ensure!(Path::new(MANIFEST).is_file());
    if manager.format == "rpm" {
        ensure!(
            fs::read(MANIFEST)?
                == fs::read("/usr/lib64/mozilla/native-messaging-hosts/firefox_codex_mcp.json")?
        );
    }
    a.run(EXECUTABLE, "--install")?;
    a.connection()?;
    b.run(EXECUTABLE, "--install")?;
    ensure!(
        fs::read(&old_b)? == changed_b,
        "Edited legacy binary was overwritten"
    );
    ensure!(b.config()?.contains(EXECUTABLE));
    // Preserved, edited old payload is inert: no manifest/config references it.
    ensure!(!b
        .home
        .join(".mozilla/native-messaging-hosts/firefox_codex_mcp.json")
        .exists());
    fs::remove_file(&old_b)?;
    b.connection()?;
    let mut native = Process::start(&a, "--native-host", true)?;
    let old_brokers = brokers(&a)?;
    let mut mcp = Process::start(&a, "--extension", false)?;
    let mut settings = Process::start(&a, "--manage-no-open", false)?;
    manager.install(&candidate)?;
    ensure!(
        generation()? != initial,
        "Upgrade did not rotate generation"
    );
    native.exited("package upgrade")?;
    mcp.exited("package upgrade")?;
    settings.exited("package upgrade")?;
    brokers_exited(&old_brokers)?;
    a.connection()?;
    b.connection()?;
    let upgraded = generation()?;
    let mut active = Process::start(&b, "--extension", false)?;
    manager.reinstall(&candidate)?;
    ensure!(
        generation()? != upgraded,
        "Reinstall did not rotate generation"
    );
    active.exited("package reinstall")?;
    a.connection()?;
    b.connection()?;
    // Disconnect only this user. Package and the second user's connection stay.
    let mut disconnected_mcp = Process::start(&b, "--extension", false)?;
    disconnected_mcp.status(false)?;
    // Firefox-launched Settings has no CODEX_HOME override. Follow this user's
    // trusted receipt to the custom root instead of editing ~/.codex.
    checked(
        b.command("/usr/bin/env")?
            .args(["--unset=CODEX_HOME", EXECUTABLE, "--doctor"]),
    )?;
    checked(
        b.command("/usr/bin/env")?
            .args(["--unset=CODEX_HOME", EXECUTABLE, "--uninstall"]),
    )?;
    disconnected_mcp.status(true)?;
    b.disconnected()?;
    // Reconnect immediately, without waiting for the background marker poll.
    // A connection nonce must keep the old MCP fenced after settings reappear.
    b.run(EXECUTABLE, "--install")?;
    disconnected_mcp.status(true)?;
    let mut fresh_mcp = Process::start(&b, "--extension", false)?;
    fresh_mcp.status(false)?;
    drop(fresh_mcp);
    b.connection()?;
    b.run(EXECUTABLE, "--uninstall")?;
    b.disconnected()?;
    ensure!(Path::new(EXECUTABLE).is_file() && Path::new(MANIFEST).is_file());
    a.connection()?;
    let foreign = b
        .home
        .join(".mozilla/native-messaging-hosts/firefox_codex_mcp.json");
    let foreign_bytes = br#"{"name":"firefox_codex_mcp","description":"User-owned host","type":"stdio","path":"/bin/true","allowed_extensions":["foreign@example.invalid"]}"#;
    write(&foreign, foreign_bytes)?;
    b.own_files()?;
    let config_before = b.config()?;
    ensure!(!b
        .command(EXECUTABLE)?
        .arg("--install")
        .output()?
        .status
        .success());
    ensure!(fs::read(&foreign)? == foreign_bytes && b.config()? == config_before);
    fs::remove_file(&foreign)?;
    let foreign_config = format!(
        "{config_before}\n[mcp_servers.firefox]\ncommand = \"/bin/foreign-browser\"\nargs = []\n"
    );
    fs::write(b.codex.join("config.toml"), &foreign_config)?;
    ensure!(!b
        .command(EXECUTABLE)?
        .arg("--install")
        .output()?
        .status
        .success());
    ensure!(
        b.config()? == foreign_config,
        "Foreign Codex server registration was changed"
    );
    fs::write(b.codex.join("config.toml"), config_before)?;
    b.run(EXECUTABLE, "--install")?;
    disconnected_mcp.status(true)?;
    let mut reconnected_mcp = Process::start(&b, "--extension", false)?;
    reconnected_mcp.status(false)?;
    drop(reconnected_mcp);
    b.connection()?;
    // Edits made after Connect survive both package erasure and Disconnect.
    let skill = b.codex.join("skills/firefox-browser/SKILL.md");
    fs::write(&skill, "User-customized browser skill.\n")?;
    let rules = b.codex.join("AGENTS.md");
    fs::write(
        &rules,
        format!("{}\nUser-added rule.\n", fs::read_to_string(&rules)?),
    )?;
    let before_remove_a = registrations(&a)?;
    let before_remove_b = registrations(&b)?;
    let mut removing_native = Process::start(&a, "--native-host", true)?;
    let removing_brokers = brokers(&a)?;
    let mut removing_mcp = Process::start(&a, "--extension", false)?;
    let mut removing_settings = Process::start(&a, "--manage-no-open", false)?;
    manager.remove()?;
    removing_native.exited("package removal")?;
    removing_mcp.exited("package removal")?;
    removing_settings.exited("package removal")?;
    brokers_exited(&removing_brokers)?;
    ensure!(
        !Path::new(EXECUTABLE).exists()
            && !Path::new(MARKER).exists()
            && !Path::new(MANIFEST).exists()
    );
    for (path, bytes) in before_remove_a.iter().chain(&before_remove_b) {
        ensure!(
            fs::read(path)? == *bytes,
            "Package scripts changed user settings: {}",
            path.display()
        );
    }
    for account in [&a, &b] {
        account.run(&portable, "--uninstall")?;
        account.run(&portable, "--uninstall")?;
        account.disconnected()?;
    }
    ensure!(fs::read_to_string(&skill)? == "User-customized browser skill.\n");
    ensure!(fs::read_to_string(&rules)?.contains("User-added rule."));
    if manager.format == "deb" {
        checked(Command::new("dpkg").args(["--purge", "firefox-codex-mcp"]))?;
        checked(Command::new("dpkg").args(["--purge", "firefox-codex-mcp"]))?;
    }
    // A complete remove followed by install must create a valid new generation.
    manager.install(&candidate)?;
    a.run(EXECUTABLE, "--install")?;
    a.connection()?;
    a.run(EXECUTABLE, "--uninstall")?;
    manager.remove()?;
    a.disconnected()?;
    b.disconnected()?;
    let package_tool = if manager.format == "deb" {
        "dpkg"
    } else {
        "rpm"
    };
    let tool = checked(Command::new(package_tool).arg("--version"))?;
    fs::write(
        evidence.join(format!("{}-lifecycle.json", manager.format)),
        serde_json::to_vec_pretty(&json!({
            "format":manager.format,"sourceVersion":env!("CARGO_PKG_VERSION"),"osRelease":fs::read_to_string("/etc/os-release")?,
            "packageTool":String::from_utf8_lossy(&tool.stdout),"baselinePackage":baseline.file_name(),"candidatePackage":candidate.file_name(),
            "baselineDescription":"Same candidate source, package release 0; candidate release 1. No earlier public RPM/DEB is claimed.",
            "legacyPortableVersion":"0.4.2","legacyBinarySha256":sha(&fs::read(legacy)?),
            "verified":["actual package install","actual package release upgrade","actual package reinstall","two nonroot users","portable 0.4.2 migration","edited legacy payload preserved and disconnected","foreign user manifest refusal","foreign Codex registration refusal","no user binary copy","system native manifest and wrapper","unchanged unrelated TOML including args=[]","custom CODEX_HOME recovered from receipt without environment override","user Disconnect leaves package and other user","existing MCP denied after Disconnect","generation revokes active broker, native host, MCP and Settings","package removal leaves user settings untouched","offline portable Disconnect after package removal","modified skill and added rules preserved","idempotent Disconnect","remove then install","synthetic Firefox profile and login fixtures byte-identical"],
            "protectedFixturesA":a.protected,"protectedFixturesB":b.protected,
            "personalProfilesUsed":false,"desktopGuiOrModelSelection":false
        }))?,
    )?;
    Ok(())
}
