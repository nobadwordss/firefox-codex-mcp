#![cfg(windows)]
//! Runs only on a disposable Windows account/runner. No personal browser profile.
use anyhow::{ensure, Context, Result};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::{
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
const A: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const B: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
const EXT: &str = "{a171de16-96b6-4e50-94b7-e0b54d94436a}";
const UUID: &str = "5ee70e6e-5f96-4594-8ef0-ad9ef7a20bed";
fn cmd(binary: &Path, root: &Path) -> Command {
    let mut c = Command::new(binary);
    c.env("FIREFOX_MCP_INSTALL_ROOT", root.join("helper"))
        .env("CODEX_HOME", root.join("codex"))
        .env("FIREFOX_MCP_RUNTIME_DIR", root.join("runtime"));
    c
}
fn run(binary: &Path, root: &Path, arg: &str) -> Result<String> {
    let output = cmd(binary, root).arg(arg).output()?;
    ensure!(
        output.status.success(),
        "{arg} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?)
}
fn installed(root: &Path) -> Result<PathBuf> {
    let r: Value = serde_json::from_slice(&fs::read(root.join("helper/install.json"))?)?;
    Ok(PathBuf::from(
        r["executable"].as_str().context("installed exe")?,
    ))
}
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Mcp {
    process: Process,
    input: ChildStdin,
    output: mpsc::Receiver<std::io::Result<String>>,
    id: u64,
    identity: String,
}
impl Mcp {
    fn start(root: &Path, identity: &str) -> Result<Self> {
        let mut process = Process(
            cmd(&installed(root)?, root)
                .arg("--extension")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()?,
        );
        let input = process.0.stdin.take().context("stdin")?;
        let stdout = process.0.stdout.take().context("stdout")?;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut s = Self {
            process,
            input,
            output: rx,
            id: 0,
            identity: identity.into(),
        };
        s.rpc("initialize",json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"windows-e2e","version":"1"}}))?;
        Ok(s)
    }
    fn rpc(&mut self, method: &str, params: Value) -> Result<Value> {
        self.id += 1;
        writeln!(
            self.input,
            "{}",
            json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params})
        )?;
        self.input.flush()?;
        let line = self.output.recv_timeout(Duration::from_secs(70))??;
        let value: Value = serde_json::from_str(&line)?;
        ensure!(
            value["id"] == self.id && value.get("error").is_none(),
            "MCP error {value}"
        );
        Ok(value["result"].clone())
    }
    fn tool(&mut self, name: &str, args: Value) -> Result<Value> {
        self.rpc(
            "tools/call",
            json!({"name":name,"arguments":args,"_meta":{"threadId":self.identity}}),
        )
    }
    fn ok(&mut self, name: &str, args: Value) -> Result<Value> {
        let v = self.tool(name, args)?;
        ensure!(v["isError"] != true, "{name}: {v}");
        Ok(serde_json::from_str(
            v["content"][0]["text"].as_str().context("content")?,
        )?)
    }
    fn denied(&mut self, name: &str, args: Value) -> Result<()> {
        let v = self.tool(name, args)?;
        ensure!(v["isError"] == true, "unexpected success {name}: {v}");
        Ok(())
    }
    fn connected(&mut self) -> Result<()> {
        let until = Instant::now() + Duration::from_secs(25);
        loop {
            let s = self.ok("firefox_status", json!({}))?;
            if s["connected"] == true {
                return Ok(());
            }
            ensure!(Instant::now() < until, "Firefox did not connect: {s}");
            thread::sleep(Duration::from_millis(200));
        }
    }
}
impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.process.0.kill();
    }
}
// The real Codex app-server dispatches MCP calls without an LLM/API account.
// This exercises Codex's Windows process/config/metadata path, not the desktop UI.
struct Codex {
    _process: Process,
    input: ChildStdin,
    output: mpsc::Receiver<std::io::Result<String>>,
    seq: u64,
}
impl Codex {
    fn start(root: &Path) -> Result<Self> {
        let mut process = Process(
            cmd(Path::new(&env::var("FIREFOX_SETUP_CODEX_BINARY")?), root)
                .arg("app-server")
                .current_dir(root)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()?,
        );
        let input = process.0.stdin.take().context("Codex stdin")?;
        let stdout = process.0.stdout.take().context("Codex stdout")?;
        let (tx, output) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut server = Self {
            _process: process,
            input,
            output,
            seq: 0,
        };
        server.rpc("initialize",json!({"clientInfo":{"name":"firefox_windows_e2e","version":"1"},"capabilities":{"experimentalApi":true}}))?;
        writeln!(
            server.input,
            "{}",
            json!({"method":"initialized","params":{}})
        )?;
        server.input.flush()?;
        Ok(server)
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
                "Unexpected Codex request: {message}"
            );
        }
    }
    fn thread(&mut self, root: &Path) -> Result<String> {
        let r=self.rpc("thread/start",json!({"cwd":root,"ephemeral":true,"model":"gpt-5","approvalPolicy":"never","sandbox":"read-only"}))?;
        Ok(r["thread"]["id"]
            .as_str()
            .context("Codex thread ID")?
            .into())
    }
    fn raw(&mut self, chat: &str, name: &str, args: Value) -> Result<Value> {
        self.rpc(
            "mcpServer/tool/call",
            json!({"threadId":chat,"server":"firefox","tool":name,"arguments":args}),
        )
    }
    fn ok(&mut self, chat: &str, name: &str, args: Value) -> Result<Value> {
        let r = self.raw(chat, name, args)?;
        ensure!(r["isError"] != true, "Codex {name}: {r}");
        Ok(serde_json::from_str(
            r["content"][0]["text"]
                .as_str()
                .context("Codex tool content")?,
        )?)
    }
}
fn codex_roundtrip(root: &Path, url: &str, artifacts: &Path) -> Result<()> {
    let mut app = Codex::start(root)?;
    let a = app.thread(root)?;
    let b = app.thread(root)?;
    let inventory = app.rpc(
        "mcpServerStatus/list",
        json!({"threadId":a,"serverName":"firefox"}),
    )?;
    fs::write(
        artifacts.join("codex-app-server-tools.json"),
        serde_json::to_vec_pretty(&inventory)?,
    )?;
    let status = app.ok(&a, "firefox_status", json!({}))?;
    ensure!(
        status["connected"] == true,
        "Codex Firefox disconnected: {status}"
    );
    let tab = app.ok(
        &a,
        "firefox_open_tab",
        json!({"url":format!("{url}/codex-app-server")}),
    )?["tabId"]
        .clone();
    let until = Instant::now() + Duration::from_secs(15);
    let s = loop {
        let result = app.raw(&a, "firefox_snapshot", json!({}))?;
        if result["isError"] != true {
            let s: Value =
                serde_json::from_str(result["content"][0]["text"].as_str().context("snapshot")?)?;
            if s["text"]
                .as_str()
                .is_some_and(|v| v.contains("Observation code"))
            {
                break s;
            }
        }
        ensure!(Instant::now() < until, "Codex rendered snapshot missing");
        thread::sleep(Duration::from_millis(100));
    };
    app.ok(&a, "firefox_click", action(&s, "Count 0")?)?;
    let s = app.ok(&a, "firefox_snapshot", json!({}))?;
    let mut typing = action(&s, "Message")?;
    typing["text"] = json!("Codex app-server Windows");
    app.ok(&a, "firefox_type", typing)?;
    ensure!(
        app.raw(&b, "firefox_select_tab", json!({"tabId":tab}))?["isError"] == true,
        "Codex chat isolation failed"
    );
    app.ok(&a, "firefox_revoke", json!({}))?;
    ensure!(
        app.raw(&a, "firefox_snapshot", json!({}))?["isError"] == true,
        "Codex revoke failed"
    );
    fs::write(
        artifacts.join("codex-app-server-result.json"),
        serde_json::to_vec_pretty(
            &json!({"actualCodexProcess":true,"actualFirefox":true,"read":true,"click":true,"type":true,"chatIsolation":true,"revoke":true,"desktopUi":false,"llmDispatch":false}),
        )?,
    )?;
    Ok(())
}
struct Browser {
    driver: Process,
    client: Client,
    base: String,
    session: String,
    version: Value,
}
impl Browser {
    fn start(root: &Path, profile: &Path) -> Result<Self> {
        fs::create_dir_all(profile)?;
        let sock = TcpListener::bind("127.0.0.1:0")?;
        let port = sock.local_addr()?.port();
        drop(sock);
        let base = format!("http://127.0.0.1:{port}");
        let driver = Process(
            cmd(Path::new(&env::var("FIREFOX_GECKODRIVER")?), root)
                .env("MOZ_REMOTE_ALLOW_SYSTEM_ACCESS", "1")
                .args([
                    "--host",
                    "127.0.0.1",
                    "--port",
                    &port.to_string(),
                    "--allow-system-access",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()?,
        );
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .no_proxy()
            .build()?;
        let until = Instant::now() + Duration::from_secs(15);
        while client.get(format!("{base}/status")).send().is_err() {
            ensure!(Instant::now() < until, "geckodriver startup");
            thread::sleep(Duration::from_millis(100));
        }
        let value:Value=client.post(format!("{base}/session")).json(&json!({"capabilities":{"alwaysMatch":{"browserName":"firefox","moz:firefoxOptions":{"binary":env::var("FIREFOX_BINARY")?,"args":["-headless","-no-remote","-profile",profile],"prefs":{"extensions.webextensions.uuids":json!({EXT:UUID}).to_string()}}}}})).send()?.json()?;
        let session = value["value"]["sessionId"]
            .as_str()
            .with_context(|| format!("Firefox startup: {value}"))?
            .into();
        let version = value["value"]["capabilities"]["browserVersion"].clone();
        Ok(Self {
            driver,
            client,
            base,
            session,
            version,
        })
    }
    fn call(&self, path: &str, body: Value) -> Result<Value> {
        let v: Value = self
            .client
            .post(format!("{}/session/{}{}", self.base, self.session, path))
            .json(&body)
            .send()?
            .json()?;
        ensure!(v["value"].get("error").is_none(), "WebDriver {path}: {v}");
        Ok(v["value"].clone())
    }
    fn script(&self, s: &str) -> Result<Value> {
        self.call("/execute/sync", json!({"script":s,"args":[]}))
    }
    fn goto(&self, url: &str) -> Result<()> {
        self.call("/url", json!({"url":url}))?;
        Ok(())
    }
    fn click(&self, selector: &str) -> Result<()> {
        let e = self.call("/element", json!({"using":"css selector","value":selector}))?;
        let id = e["element-6066-11e4-a52e-4f735466cecf"]
            .as_str()
            .context("element")?;
        self.call(&format!("/element/{id}/click"), json!({}))?;
        Ok(())
    }
    fn screenshot(&self, path: &Path) -> Result<()> {
        use base64::Engine;
        let v: Value = self
            .client
            .get(format!("{}/session/{}/screenshot", self.base, self.session))
            .send()?
            .json()?;
        fs::write(
            path,
            base64::engine::general_purpose::STANDARD
                .decode(v["value"].as_str().context("screenshot")?)?,
        )?;
        Ok(())
    }
}
impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self
            .client
            .delete(format!("{}/session/{}", self.base, self.session))
            .send();
        let _ = self.driver.0.kill();
    }
}
fn fixture() -> Result<String> {
    let socket = TcpListener::bind("127.0.0.1:0")?;
    let url = format!("http://{}", socket.local_addr()?);
    thread::spawn(move || {
        for mut stream in socket.incoming().flatten() {
            let mut scratch = [0u8; 4096];
            let _ = stream.read(&mut scratch);
            let body = r##"<!doctype html><html><head><title>Windows Firefox E2E</title></head><body><h1>Firefox Windows work tab</h1><p>Observation code: WINDOWS-NATIVE-42</p><button id='count' onclick='this.textContent="Count "+(++window.counter);document.querySelector("#status").textContent="Count updated to "+window.counter'>Count 0</button><label>Message<input id='message'></label><p id='status' role='status'>Ready</p><script>window.counter=0</script><div style='height:1800px'>Scroll target</div></body></html>"##;
            let _=write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
        }
    });
    Ok(url)
}
fn snapshot(m: &mut Mcp) -> Result<Value> {
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        let result = m.tool("firefox_snapshot", json!({}))?;
        if result["isError"] != true {
            let s: Value = serde_json::from_str(
                result["content"][0]["text"]
                    .as_str()
                    .context("snapshot text")?,
            )?;
            if s["text"]
                .as_str()
                .is_some_and(|v| v.contains("Observation code"))
            {
                return Ok(s);
            }
        }
        ensure!(Instant::now() < until, "fixture did not render: {result}");
        thread::sleep(Duration::from_millis(100));
    }
}
fn action(s: &Value, name: &str) -> Result<Value> {
    let e = s["elements"]
        .as_array()
        .context("elements")?
        .iter()
        .find(|e| e["label"] == name)
        .with_context(|| format!("missing {name}: {s}"))?;
    Ok(json!({"snapshotId":s["snapshotId"],"elementId":e["id"]}))
}

#[test]
#[ignore = "disposable Windows runner: actual HKCU install and signed Firefox extension"]
fn windows_install_update_remove_and_firefox_e2e() -> Result<()> {
    ensure!(
        env::var("FIREFOX_MCP_WINDOWS_E2E").as_deref() == Ok("1"),
        "Requires disposable Windows test account"
    );
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().join("첫 설치 user's & space");
    fs::create_dir_all(root.join("codex"))?;
    let artifacts = PathBuf::from(env::var("FIREFOX_WINDOWS_ARTIFACTS")?);
    fs::create_dir_all(&artifacts)?;
    let binary = env::var_os("FIREFOX_SETUP_SERVER_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_firefox-codex-mcp")));
    let initial="# Preserve this comment\nmodel = \"example-model\"\n[mcp_servers.other]\ncommand = \"other.exe\"\n";
    fs::write(root.join("codex/config.toml"), initial)?;
    fs::write(root.join("codex/AGENTS.md"), "Prefer Rust.\n")?;
    let profile = root.join("profile-a");
    let browser = Browser::start(&root, &profile)?;
    let mut manager = Process(
        cmd(&binary, &root)
            .arg("--manage-no-open")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?,
    );
    let stdout = manager.0.stdout.take().context("manager stdout")?;
    let (send, receive) = mpsc::channel();
    thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
        let _ = send.send(result);
    });
    let line = receive.recv_timeout(Duration::from_secs(10))??;
    let manager_url = line.split_whitespace().last().context("manager URL")?;
    ensure!(
        manager_url.starts_with("http://127.0.0.1:"),
        "Manager must be loopback"
    );
    browser.goto(manager_url)?;
    let until = Instant::now() + Duration::from_secs(10);
    while browser
        .script("return document.querySelector('#version').textContent;")?
        .as_str()
        .is_none_or(|v| v.contains("Checking"))
    {
        ensure!(Instant::now() < until, "manager state unavailable");
        thread::sleep(Duration::from_millis(100));
    }
    browser.click("#install")?;
    browser.click("#confirm-yes")?;
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let result = browser.script("return document.querySelector('#result').textContent;")?;
        if result
            .as_str()
            .is_some_and(|v| v.contains("Connection installed"))
        {
            break;
        }
        ensure!(Instant::now() < until, "GUI install failed: {result}");
        thread::sleep(Duration::from_millis(100));
    }
    browser.screenshot(&artifacts.join("first-install-manager.png"))?;
    browser.click("#extension-install")?;
    browser.call("/moz/context", json!({"context":"chrome"}))?;
    for notice in [
        "addon-install-blocked-notification",
        "addon-webext-permissions-notification",
    ] {
        let until = Instant::now() + Duration::from_secs(25);
        loop {
            let script=format!("const panel=document.querySelector('#notification-popup');const button=document.getElementById('{}')?.querySelector('.popup-notification-primary-button');if(!panel?.hasAttribute('panelopen')||!button||button.disabled||button.hasAttribute('disabled'))return false;button.click();return true;",notice);
            if browser.script(&script)? == true {
                break;
            }
            ensure!(
                Instant::now() < until,
                "Missing Firefox install confirmation: {notice}"
            );
            thread::sleep(Duration::from_millis(100));
        }
    }
    browser.call("/actions",json!({"actions":[{"type":"key","id":"keyboard","actions":[{"type":"keyDown","value":"\u{e00c}"},{"type":"keyUp","value":"\u{e00c}"}]}]}))?;
    browser.call("/moz/context", json!({"context":"content"}))?;
    browser.click("#close")?;
    drop(manager);
    let first = installed(&root)?;
    let doctor: Value = serde_json::from_str(&run(&binary, &root, "--doctor")?)?;
    ensure!(
        doctor["nativeHostRegistered"] == true && doctor["globalMcpRegistered"] == true,
        "{doctor}"
    );
    // Validate registration with the actual official Windows Codex CLI.
    let codex = PathBuf::from(env::var("FIREFOX_SETUP_CODEX_BINARY")?);
    let listing = cmd(&codex, &root)
        .args(["mcp", "list", "--json"])
        .output()?;
    ensure!(
        listing.status.success(),
        "Codex MCP listing failed: {}",
        String::from_utf8_lossy(&listing.stderr)
    );
    let list: Value = serde_json::from_slice(&listing.stdout)?;
    ensure!(
        list.as_array()
            .is_some_and(|v| v.iter().any(|v| v["name"] == "firefox")),
        "Codex did not discover Firefox"
    );
    fs::write(
        artifacts.join("codex-mcp-registration.json"),
        &listing.stdout,
    )?;
    let url = fixture()?;
    let mut a = Mcp::start(&root, A)?;
    let mut b = Mcp::start(&root, B)?;
    a.connected()?;
    let tab = a.ok("firefox_open_tab", json!({"url":url}))?["tabId"].clone();
    let s = snapshot(&mut a)?;
    a.ok("firefox_click", action(&s, "Count 0")?)?;
    let s = snapshot(&mut a)?;
    ensure!(
        s["text"]
            .as_str()
            .is_some_and(|v| v.contains("Count updated to 1")),
        "status text missing"
    );
    let mut typing = action(&s, "Message")?;
    typing["text"] = json!("WINDOWS-NATIVE-42");
    a.ok("firefox_type", typing)?;
    let btab = b.ok("firefox_open_tab", json!({"url":format!("{url}/b")}))?["tabId"].clone();
    b.denied("firefox_select_tab", json!({"tabId":tab}))?;
    drop(a);
    let mut a = Mcp::start(&root, A)?;
    ensure!(
        a.ok("firefox_tabs", json!({}))?["tabs"][0]["tabId"] == tab,
        "turn continuity failed"
    );
    a.denied("firefox_select_tab", json!({"tabId":btab}))?;
    let s = snapshot(&mut a)?;
    a.ok("firefox_click", action(&s, "Count 1")?)?;
    let handles: Value = browser
        .client
        .get(format!(
            "{}/session/{}/window/handles",
            browser.base, browser.session
        ))
        .send()?
        .json()?;
    let mut found = false;
    for handle in handles["value"].as_array().context("window handles")? {
        browser.call("/window", json!({"handle":handle}))?;
        if browser.script("return location.href;")? == format!("{url}/") {
            let dom=browser.script("return {count:document.querySelector('#count').textContent,message:document.querySelector('#message').value};")?;
            ensure!(
                dom["count"] == "Count 2" && dom["message"] == "WINDOWS-NATIVE-42",
                "Document state changed: {dom}"
            );
            browser.screenshot(&artifacts.join("work-tab-after-reconnect.png"))?;
            found = true;
        }
    }
    ensure!(found, "Original work tab missing");
    a.ok("firefox_revoke", json!({}))?;
    a.denied("firefox_snapshot", json!({}))?;
    drop(a);
    let mut a = Mcp::start(&root, A)?;
    a.denied("firefox_select_tab", json!({"tabId":tab}))?;
    ensure!(
        b.ok("firefox_tabs", json!({}))?["tabs"][0]["tabId"] == btab,
        "other chat lost access"
    );
    // Exercise real extension UI: global revoke and reconnect pause, no internal API.
    browser.goto(&format!("moz-extension://{UUID}/native/popup.html"))?;
    browser.click("#revoke")?;
    b.denied("firefox_snapshot", json!({}))?;
    browser.screenshot(&artifacts.join("extension-revoked.png"))?;
    codex_roundtrip(&root, &url, &artifacts)?;
    // A different build payload rotates immutable paths while old MCP EXEs run.
    let candidate = root.join("update candidate.exe");
    let mut bytes = fs::read(&binary)?;
    bytes.extend_from_slice(b"\nwindows-update-e2e-overlay\n");
    fs::write(&candidate, bytes)?;
    run(&candidate, &root, "--update")?;
    let second = installed(&root)?;
    ensure!(
        first != second && first.is_file(),
        "running executable was overwritten"
    );
    drop(a);
    drop(b);
    let mut fresh = Mcp::start(&root, A)?;
    fresh.connected()?;
    fresh.denied("firefox_select_tab", json!({"tabId":tab}))?;
    fresh.ok("firefox_open_tab", json!({"url":url}))?;
    snapshot(&mut fresh)?;
    drop(fresh);
    // Permanent signed extension and selected profile survive normal Firefox restart.
    let version = browser.version.clone();
    drop(browser);
    let browser = Browser::start(&root, &profile)?;
    let mut after_restart = Mcp::start(&root, A)?;
    after_restart.connected()?;
    ensure!(
        after_restart.ok("firefox_tabs", json!({}))?["tabs"]
            .as_array()
            .is_some_and(|v| v.is_empty()),
        "restart restored revoked tabs"
    );
    after_restart.ok("firefox_open_tab", json!({"url":url}))?;
    snapshot(&mut after_restart)?;
    run(&candidate, &root, "--uninstall")?;
    let mut after_remove = Mcp::start(&root, A).err();
    ensure!(
        after_remove.take().is_some(),
        "receipt remained after removal"
    );
    ensure!(
        !root.join("helper/firefox_codex_mcp.json").exists(),
        "native manifest retained"
    );
    let text = fs::read_to_string(root.join("codex/config.toml"))?;
    ensure!(
        text.contains("# Preserve this comment")
            && text.contains("mcp_servers.other")
            && !text.contains("mcp_servers.firefox"),
        "unrelated config changed: {text}"
    );
    ensure!(
        fs::read_to_string(root.join("codex/AGENTS.md"))? == "Prefer Rust.\n",
        "guidance not restored"
    );
    drop(after_restart);
    drop(browser);
    run(&binary, &root, "--repair")?;
    fs::write(
        artifacts.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"os":env::var("OS").ok(),"runner":env::var("ImageOS").ok(),"firefoxVersion":version,"signedExtension":"0.4.0","verified":["Unicode and spaces install","official Codex CLI MCP discovery","actual Codex app-server tool dispatch","signed permanent extension","native messaging + named pipes","read text and status","click and type","two-chat isolation","MCP replacement continuity","irreversible revoke","extension UI revoke","update with running old EXE","Firefox restart","uninstall preserves unrelated settings"],"notVerified":["Windows 10 desktop","Windows 11 desktop","Codex desktop UI or live model dispatch","Authenticode and SmartScreen"]}),
        )?,
    )?;
    Ok(())
}
