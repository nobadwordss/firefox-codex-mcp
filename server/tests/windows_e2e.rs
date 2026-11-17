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
fn known_folders(root: &Path) -> bool {
    root.join("use-known-folders").is_file()
}
fn install_root(root: &Path) -> Result<PathBuf> {
    if known_folders(root) {
        Ok(
            PathBuf::from(env::var_os("LOCALAPPDATA").context("LOCALAPPDATA")?)
                .join("FirefoxCodexMCP"),
        )
    } else {
        Ok(root.join("helper"))
    }
}
fn cmd(binary: &Path, root: &Path) -> Command {
    let mut c = Command::new(binary);
    c.env("CODEX_HOME", root.join("codex"));
    if known_folders(root) {
        c.env_remove("FIREFOX_MCP_INSTALL_ROOT")
            .env_remove("FIREFOX_MCP_RUNTIME_DIR");
    } else {
        c.env("FIREFOX_MCP_INSTALL_ROOT", root.join("helper"))
            .env("FIREFOX_MCP_RUNTIME_DIR", root.join("runtime"));
    }
    c
}
fn run(binary: &Path, root: &Path, arg: &str) -> Result<String> {
    eprintln!("Windows lifecycle {arg}: {}", root.display());
    let mut process = Process(
        cmd(binary, root)
            .arg(arg)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?,
    );
    let mut stdout = process.0.stdout.take().context("lifecycle stdout")?;
    let (send, receive) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.read_to_end(&mut bytes).map(|_| bytes);
        let _ = send.send(result);
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = process.0.try_wait()? {
            break status;
        }
        ensure!(
            Instant::now() < deadline,
            "Lifecycle process did not exit: {arg}"
        );
        thread::sleep(Duration::from_millis(50));
    };
    ensure!(status.success(), "{arg} failed: {status}");
    let output = receive
        .recv_timeout(Duration::from_secs(2))
        .context("Lifecycle exited but a descendant retained its output pipe")??;
    if matches!(arg, "--update" | "--uninstall") {
        let result: Value = serde_json::from_slice(&output)?;
        ensure!(
            result["restartRequired"] != true,
            "Router reset failed: {result}"
        );
    }
    eprintln!("Windows lifecycle {arg}: complete");
    Ok(String::from_utf8(output)?)
}
fn installed(root: &Path) -> Result<PathBuf> {
    let r: Value = serde_json::from_slice(&fs::read(install_root(root)?.join("install.json"))?)?;
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
        eprintln!("Raw MCP client: {method}");
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
        // Codex filters inherited environment variables for MCP children. These
        // overrides isolate this test's alternate root; normal installation uses
        // Windows Known Folders on both sides and needs no such configuration.
        let isolation = format!(
            "mcp_servers.firefox.env={{FIREFOX_MCP_INSTALL_ROOT={},FIREFOX_MCP_RUNTIME_DIR={}}}",
            serde_json::to_string(&root.join("helper"))?,
            serde_json::to_string(&root.join("runtime"))?
        );
        let mut command = cmd(Path::new(&env::var("FIREFOX_SETUP_CODEX_BINARY")?), root);
        command.arg("app-server");
        if !known_folders(root) {
            command.args(["-c", &isolation]);
        }
        let mut process = Process(
            command
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
        eprintln!("Codex app-server: {method}");
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
fn codex_roundtrip(root: &Path, url: &str, artifacts: &Path, browser: &Browser) -> Result<()> {
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
    // Verify actual effects independently of the MCP success response.
    let handles: Value = browser
        .client
        .get(format!(
            "{}/session/{}/window/handles",
            browser.base, browser.session
        ))
        .send()?
        .json()?;
    let mut verified = false;
    for handle in handles["value"]
        .as_array()
        .context("Codex window handles")?
    {
        browser.call("/window", json!({"handle":handle}))?;
        if browser.script("return location.href;")? == format!("{url}/codex-app-server") {
            let dom = browser.script("return {count:document.querySelector('#count').textContent,message:document.querySelector('#message').value};")?;
            ensure!(
                dom["count"] == "Count 1" && dom["message"] == "Codex app-server Windows",
                "Codex action readback failed: {dom}"
            );
            browser.screenshot(&artifacts.join("codex-app-server-actions.png"))?;
            verified = true;
        }
    }
    ensure!(verified, "Codex action tab missing");
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
            &json!({"actualCodexProcess":true,"actualFirefox":true,"read":true,"click":true,"type":true,"independentDomReadback":true,"chatIsolation":true,"revoke":true,"desktopUi":false,"llmDispatch":false}),
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
        eprintln!("Starting disposable Firefox: {}", profile.display());
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
    fn popup_script(&self, script: &str) -> Result<Value> {
        self.call("/execute/sync", json!({"script":r#"
            const {getMarionetteCommandsActorProxy} = ChromeUtils.importESModule('chrome://remote/content/marionette/actors/MarionetteCommandsParent.sys.mjs');
            const actor = getMarionetteCommandsActorProxy(() => document.querySelector('.webextension-popup-browser').browsingContext);
            return actor.executeScript(arguments[0], [], {timeout:10000, newSandbox:true});
        "#,"args":[script]}))
    }
    fn popup(&self) -> Result<()> {
        self.call("/moz/context", json!({"context":"chrome"}))?;
        let widget = self.script(&format!(r#"
            const {{CustomizableUI}} = ChromeUtils.importESModule('moz-src:///browser/components/customizableui/CustomizableUI.sys.mjs');
            const id = '{}'.toLowerCase().replace(/[^a-z0-9_-]/g,'_') + '-browser-action';
            CustomizableUI.addWidgetToArea(id, CustomizableUI.AREA_NAVBAR);
            return id;
        "#, EXT))?;
        self.click(&format!(
            "#{} .unified-extensions-item-action-button",
            widget.as_str().context("toolbar widget")?
        ))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if self.script("return !!document.querySelector('.webextension-popup-browser');")?
                == true
            {
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "Extension toolbar popup did not open"
            );
            thread::sleep(Duration::from_millis(100));
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if self
                .popup_script("return document.querySelector('#status')?.textContent;")?
                .as_str()
                .is_some_and(|text| text.contains("Connected"))
            {
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "Extension toolbar popup did not connect"
            );
            thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }
    fn popup_click(&self, selector: &str) -> Result<()> {
        self.call("/execute/sync", json!({"script":r#"
            const {getMarionetteCommandsActorProxy} = ChromeUtils.importESModule('chrome://remote/content/marionette/actors/MarionetteCommandsParent.sys.mjs');
            const actor = getMarionetteCommandsActorProxy(() => document.querySelector('.webextension-popup-browser').browsingContext);
            return actor.findElement('css selector', arguments[0], {timeout:5000,all:false})
                .then(element => actor.clickElement(element, new Map([['moz:accessibilityChecks',false],['moz:webdriverClick',true]])));
        "#,"args":[selector]}))?;
        Ok(())
    }
    fn close_popup(&self) -> Result<()> {
        self.call("/moz/context", json!({"context":"chrome"}))?;
        self.call("/actions",json!({"actions":[{"type":"key","id":"keyboard","actions":[{"type":"keyDown","value":"\u{e00c}"},{"type":"keyUp","value":"\u{e00c}"}]}]}))?;
        self.call("/moz/context", json!({"context":"content"}))?;
        self.call("/frame", json!({"id":null}))?;
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
fn settings_from_popup(
    browser: &Browser,
    client: &mut Mcp,
    artifacts: &Path,
    single_profile: bool,
    action: Option<&str>,
) -> Result<()> {
    let handles = || -> Result<Vec<Value>> {
        let response: Value = browser
            .client
            .get(format!(
                "{}/session/{}/window/handles",
                browser.base, browser.session
            ))
            .send()?
            .json()?;
        Ok(response["value"]
            .as_array()
            .context("window handles")?
            .clone())
    };
    let original: Value = browser
        .client
        .get(format!(
            "{}/session/{}/window",
            browser.base, browser.session
        ))
        .send()?
        .json()?;
    let original = original["value"].clone();
    let before_handles = handles()?;
    let before_tabs = client.ok("firefox_tabs", json!({}))?["tabs"].clone();
    browser.popup()?;
    if single_profile {
        ensure!(
            browser.popup_script("return document.querySelector('#choose').hidden;")? == true,
            "A single Firefox profile must be selected without an extra choice"
        );
    }
    browser.popup_click("#settings")?;
    browser.close_popup()?;
    let deadline = Instant::now() + Duration::from_secs(20);
    let manager = loop {
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
    browser.call("/window", json!({"handle":manager}))?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if browser.script("return location.origin.startsWith('http://127.0.0.1:') && !!document.querySelector('#doctor') && !document.querySelector('#doctor').disabled && document.querySelector('#version')?.textContent.includes('Installed');")? == true { break; }
        ensure!(
            Instant::now() < deadline,
            "Settings page did not become ready"
        );
        thread::sleep(Duration::from_millis(100));
    }
    browser.click("#doctor")?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let result = browser.script("return document.querySelector('#result').textContent;")?;
        if result.as_str().is_some_and(|text| {
            text.contains("Codex registration: ready") && text.contains("Native connection: ready")
        }) {
            break;
        }
        ensure!(Instant::now() < deadline, "Settings check failed: {result}");
        thread::sleep(Duration::from_millis(100));
    }
    ensure!(
        client.ok("firefox_tabs", json!({}))?["tabs"] == before_tabs,
        "Settings must not become a chat-owned work tab"
    );
    browser.screenshot(&artifacts.join("extension-settings-check.png"))?;
    if let Some(action) = action {
        browser.click(if action == "uninstall" {
            "#uninstall"
        } else {
            "#install"
        })?;
        browser.click("#confirm-yes")?;
        let expected = if action == "uninstall" {
            "Connection removed."
        } else {
            "Connection installed."
        };
        let deadline = Instant::now() + Duration::from_secs(25);
        loop {
            let result = browser.script("return document.querySelector('#result').textContent;")?;
            if result.as_str().is_some_and(|text| text.contains(expected)) {
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "Settings {action} response failed: {result}"
            );
            thread::sleep(Duration::from_millis(100));
        }
    }
    // Windows removal closes its manager automatically after delivering the
    // response, so locked binaries can be cleaned up. Other paths close via UI.
    if action != Some("uninstall") {
        browser.click("#close")?;
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let result = browser.script("return document.querySelector('#result').textContent;")?;
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
        .client
        .delete(format!(
            "{}/session/{}/window",
            browser.base, browser.session
        ))
        .send()?
        .error_for_status()?;
    browser.call("/window", json!({"handle":original}))?;
    ensure!(
        handles()?.len() == before_handles.len(),
        "Settings left an extra tab"
    );
    Ok(())
}
fn fixture() -> Result<String> {
    let socket = TcpListener::bind("127.0.0.1:0")?;
    let url = format!("http://{}", socket.local_addr()?);
    thread::spawn(move || {
        for mut stream in socket.incoming().flatten() {
            let mut scratch = [0u8; 4096];
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            if !matches!(stream.read(&mut scratch), Ok(n) if n > 0) {
                continue;
            }
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
#[ignore = "disposable Windows runner: actual repair process and Windows setup locks"]
fn repair_waits_for_cleanup_and_installer_locks() -> Result<()> {
    use std::os::windows::fs::OpenOptionsExt;

    ensure!(
        env::var("FIREFOX_MCP_WINDOWS_E2E").as_deref() == Ok("1"),
        "Requires disposable Windows test account"
    );
    let temporary = tempfile::tempdir()?;
    let binary = env::var_os("FIREFOX_SETUP_SERVER_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_firefox-codex-mcp")));
    let mut results = Vec::new();
    for exclusive_share in [true, false] {
        let kind = if exclusive_share {
            "cleanup-exclusive-share"
        } else {
            "installer-byte-range"
        };
        let root = temporary.path().join(format!("{kind} 한글 user's & space"));
        fs::create_dir_all(root.join("codex"))?;
        let helper = install_root(&root)?;
        fs::create_dir_all(&helper)?;
        let config = "# Preserve this comment\n[mcp_servers.other]\ncommand = \"other.exe\"\n";
        let guidance = "Prefer Rust.\n";
        fs::write(root.join("codex/config.toml"), config)?;
        fs::write(root.join("codex/AGENTS.md"), guidance)?;
        let cleanup = helper.join("cleanup.json");
        fs::write(&cleanup, br#"{"files":[]}"#)?;
        let lock_path = helper.join("install.lock");
        fs::write(&lock_path, b"Keep the existing lock file.\n")?;
        let mut options = fs::OpenOptions::new();
        options.read(true).write(true);
        if exclusive_share {
            // The deferred PowerShell cleanup worker opens this same path with
            // FileShare.None, so even opening it must be retried by repair.
            options.share_mode(0);
        }
        let guard = options.open(&lock_path)?;
        if !exclusive_share {
            // Ordinary setup uses a byte-range lock on a shared-open handle.
            guard.lock()?;
        }
        let mut process = Process(
            cmd(&binary, &root)
                .arg("--repair")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?,
        );
        let mut stdout = process.0.stdout.take().context("repair stdout")?;
        let stderr = process.0.stderr.take().context("repair stderr")?;
        let (send_output, output) = mpsc::channel();
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = stdout.read_to_end(&mut bytes).map(|_| bytes);
            let _ = send_output.send(result);
        });
        let (send_line, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                if send_line.send(line).is_err() {
                    break;
                }
            }
        });
        // Wait for the child to actually encounter contention, rather than
        // sleeping and assuming that Windows has scheduled it by then.
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut observed = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let line = lines.recv_timeout(remaining).with_context(|| {
                format!("Repair never waited on {kind}; stderr: {observed:?}")
            })??;
            if line == "Windows setup lock busy; waiting" {
                break;
            }
            observed.push(line);
        }
        ensure!(
            process.0.try_wait()?.is_none(),
            "Repair exited while {kind} was still held"
        );
        ensure!(cleanup.is_file(), "Repair bypassed the held {kind}");
        ensure!(fs::read_to_string(root.join("codex/config.toml"))? == config);
        ensure!(fs::read_to_string(root.join("codex/AGENTS.md"))? == guidance);
        drop(guard);
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = process.0.try_wait()? {
                break status;
            }
            ensure!(
                Instant::now() < deadline,
                "Repair did not resume after {kind}"
            );
            thread::sleep(Duration::from_millis(20));
        };
        ensure!(status.success(), "Repair failed after {kind}: {status}");
        let bytes = output.recv_timeout(Duration::from_secs(2))??;
        let result: Value = serde_json::from_slice(&bytes)?;
        ensure!(result["repaired"] == true, "Repair result: {result}");
        ensure!(
            !cleanup.exists(),
            "Repair did not finish the pending cleanup"
        );
        ensure!(fs::read(&lock_path)? == b"Keep the existing lock file.\n");
        ensure!(fs::read_to_string(root.join("codex/config.toml"))? == config);
        ensure!(fs::read_to_string(root.join("codex/AGENTS.md"))? == guidance);
        results.push(json!({"lock":kind,"contentionObserved":true,"blockedUntilReleased":true,"repairResumed":true,"unrelatedSettingsPreserved":true}));
    }
    let artifacts = PathBuf::from(env::var("FIREFOX_WINDOWS_ARTIFACTS")?);
    fs::create_dir_all(&artifacts)?;
    fs::write(
        artifacts.join("repair-lock-result.json"),
        serde_json::to_vec_pretty(&json!({"cases":results}))?,
    )?;
    Ok(())
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
    let discovered = cmd(&binary, &root)
        .env_remove("FIREFOX_BINARY")
        .arg("--doctor")
        .output()?;
    ensure!(
        discovered.status.success(),
        "normal Firefox discovery failed"
    );
    let doctor: Value = serde_json::from_slice(&discovered.stdout)?;
    ensure!(
        doctor["firefoxAvailable"] == true,
        "App Paths Firefox discovery failed: {doctor}"
    );
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
    let b = Mcp::start(&root, B)?;
    a.connected()?;
    let ordinary_url = format!("{url}/ordinary");
    browser.goto(&ordinary_url)?;
    settings_from_popup(&browser, &mut a, &artifacts, true, Some("update"))?;
    drop(a);
    drop(b);
    let mut a = Mcp::start(&root, A)?;
    let mut b = Mcp::start(&root, B)?;
    a.connected()?;
    ensure!(browser.script("return document.querySelector('#count').textContent;")? == "Count 0");
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
    let mut ordinary_unchanged = false;
    for handle in handles["value"].as_array().context("window handles")? {
        browser.call("/window", json!({"handle":handle}))?;
        let page_url = browser.script("return location.href;")?;
        if page_url == ordinary_url {
            let dom=browser.script("return {count:document.querySelector('#count').textContent,message:document.querySelector('#message').value};")?;
            ensure!(
                dom["count"] == "Count 0" && dom["message"] == "",
                "Ordinary tab changed during MCP work: {dom}"
            );
            ordinary_unchanged = true;
        }
        if page_url == format!("{url}/") {
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
    ensure!(ordinary_unchanged, "Ordinary Firefox tab missing");
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
    let paused_tab = a.ok("firefox_open_tab", json!({"url":url}))?["tabId"].clone();
    snapshot(&mut a)?;
    browser.click("#toggle")?;
    let until = Instant::now() + Duration::from_secs(8);
    while a.ok("firefox_status", json!({}))?["enabled"] != false {
        ensure!(Instant::now() < until, "pause not applied");
        thread::sleep(Duration::from_millis(100));
    }
    a.denied("firefox_snapshot", json!({}))?;
    browser.click("#toggle")?;
    let until = Instant::now() + Duration::from_secs(8);
    while a.ok("firefox_status", json!({}))?["enabled"] != true {
        ensure!(Instant::now() < until, "resume not applied");
        thread::sleep(Duration::from_millis(100));
    }
    a.denied("firefox_select_tab", json!({"tabId":paused_tab}))?;
    codex_roundtrip(&root, &url, &artifacts, &browser)?;
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
    let original_profile = fresh.ok("firefox_status", json!({}))?["profileId"].clone();
    let second_browser = Browser::start(&root, &root.join("profile-b"))?;
    second_browser.call(
        "/moz/addon/install",
        json!({"path":env::var("FIREFOX_SIGNED_XPI")?,"temporary":false}),
    )?;
    second_browser.goto(&format!("moz-extension://{UUID}/native/popup.html"))?;
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        if second_browser
            .script("return document.querySelector('#status').textContent;")?
            .as_str()
            .is_some_and(|v| v.contains("Connected"))
        {
            break;
        }
        ensure!(Instant::now() < until, "second profile did not connect");
        thread::sleep(Duration::from_millis(100));
    }
    ensure!(
        fresh.ok("firefox_status", json!({}))?["profileId"] == original_profile,
        "second profile stole selection"
    );
    second_browser.goto(&url)?;
    settings_from_popup(&second_browser, &mut fresh, &artifacts, false, None)?;
    second_browser.goto(&format!("moz-extension://{UUID}/native/popup.html"))?;
    second_browser.click("#choose")?;
    let until = Instant::now() + Duration::from_secs(10);
    while fresh.ok("firefox_status", json!({}))?["profileId"] == original_profile {
        ensure!(Instant::now() < until, "profile choice not applied");
        thread::sleep(Duration::from_millis(100));
    }
    fresh.denied("firefox_snapshot", json!({}))?;
    fresh.ok("firefox_open_tab", json!({"url":url}))?;
    snapshot(&mut fresh)?;
    drop(second_browser);
    let until = Instant::now() + Duration::from_secs(15);
    while fresh.ok("firefox_status", json!({}))?["connected"] != false {
        ensure!(
            Instant::now() < until,
            "selected profile did not disconnect"
        );
        thread::sleep(Duration::from_millis(100));
    }
    fresh.denied("firefox_open_tab", json!({"url":url}))?;
    browser.goto(&format!("moz-extension://{UUID}/native/popup.html"))?;
    browser.click("#choose")?;
    fresh.connected()?;
    ensure!(
        fresh.ok("firefox_status", json!({}))?["profileId"] == original_profile,
        "original profile selection failed"
    );
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
    after_restart.denied("firefox_snapshot", json!({}))?;
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
    let until = Instant::now() + Duration::from_secs(30);
    while first.exists() || second.exists() {
        if Instant::now() >= until {
            let log = fs::read_to_string(root.join("helper/cleanup-test.log"))
                .unwrap_or_else(|e| format!("No worker log: {e}"));
            eprintln!("Cleanup diagnostics: {log}");
            fs::write(artifacts.join("cleanup-test.log"), log)?;
            eprintln!(
                "Remaining EXEs: first={} second={}",
                first.exists(),
                second.exists()
            );
        }
        ensure!(
            Instant::now() < until,
            "Deferred removal did not delete unlocked EXEs (Unicode path)"
        );
        thread::sleep(Duration::from_millis(200));
    }
    run(&binary, &root, "--repair")?;
    // Normal production endpoint: neither Firefox nor Codex gets an endpoint
    // override. Only Codex's standard CODEX_HOME remains isolated for CI.
    let normal = tempfile::tempdir()?;
    fs::write(normal.path().join("use-known-folders"), b"1")?;
    ensure!(
        !install_root(normal.path())?.join("install.json").exists(),
        "Disposable runner already has a normal installation"
    );
    let retained_selection = install_root(normal.path())?
        .join("state/profiles.json")
        .exists();
    run(&binary, normal.path(), "--install")?;
    let normal_browser = Browser::start(normal.path(), &normal.path().join("profile"))?;
    normal_browser.call(
        "/moz/addon/install",
        json!({"path":env::var("FIREFOX_SIGNED_XPI")?,"temporary":false}),
    )?;
    if retained_selection {
        normal_browser.goto(&format!("moz-extension://{UUID}/native/popup.html"))?;
        let until = Instant::now() + Duration::from_secs(15);
        while normal_browser
            .script("return document.querySelector('#status').textContent;")?
            .as_str()
            .is_none_or(|v| !v.contains("Connected"))
        {
            ensure!(Instant::now() < until, "normal profile reconnect");
            thread::sleep(Duration::from_millis(100));
        }
        normal_browser.click("#choose")?;
    }
    let mut normal_mcp = Mcp::start(normal.path(), A)?;
    normal_mcp.connected()?;
    let normal_artifacts = artifacts.join("known-folders");
    fs::create_dir_all(&normal_artifacts)?;
    codex_roundtrip(normal.path(), &url, &normal_artifacts, &normal_browser)?;
    normal_browser.goto(&url)?;
    settings_from_popup(
        &normal_browser,
        &mut normal_mcp,
        &normal_artifacts,
        false,
        Some("uninstall"),
    )?;
    ensure!(
        normal_browser.script("return document.querySelector('#count').textContent;")? == "Count 0"
    );
    normal_mcp.denied("firefox_snapshot", json!({}))?;
    drop(normal_mcp);
    drop(normal_browser);
    run(&binary, normal.path(), "--repair")?;
    fs::write(
        artifacts.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"os":env::var("OS").ok(),"runner":env::var("ImageOS").ok(),"firefoxVersion":version,"signedExtension":serde_json::from_str::<Value>(include_str!("../../extension/manifest.json"))?["version"],"verified":["Unicode and spaces install","official Codex CLI MCP discovery","actual Codex app-server tool dispatch","normal Known Folders endpoint without MCP environment overrides","signed permanent extension","native messaging + named pipes","single-profile automatic selection","extension toolbar Settings in same Firefox profile","Settings excluded from work tabs","Settings update response after native reset","nonselected-profile Settings","Settings removal response and automatic service shutdown","read text and status","click and type","two-chat isolation","MCP replacement continuity","irreversible revoke","extension UI revoke and pause","two-profile selection and no offline fallback","update with running old EXE","Firefox restart","uninstall preserves unrelated settings"],"notVerified":["Windows 10 desktop","Windows 11 desktop","Codex desktop UI or live model dispatch","Authenticode and SmartScreen"]}),
        )?,
    )?;
    Ok(())
}
