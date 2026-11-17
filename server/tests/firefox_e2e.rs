#![cfg(unix)]
//! Opt-in real Firefox integration test. Uses a throwaway WebDriver profile.
mod debug_e2e;
mod first_run_e2e;
mod native_e2e;
use anyhow::{bail, ensure, Context, Result};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::{
    cell::Cell,
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const EXTENSION_ID: &str = "firefox-codex-mcp@local.invalid";
const EXTENSION_UUID: &str = "c888164b-78d2-49ba-a0af-1742f9da4102";
const FIXTURE: &str = include_str!("fixtures/page.html");

/// Serves the same disposable page for a manually opened, existing Firefox profile.
/// Does not start Firefox, install an addon, or grant any access. Write finish.json
/// in the artifact directory to stop; otherwise the page expires after 30 minutes.
#[test]
#[ignore = "interactive current-profile check; no WebDriver or browser launch"]
fn current_profile_fixture() -> Result<()> {
    let artifacts = PathBuf::from(env::var("FIREFOX_PROFILE_ARTIFACTS")?);
    fs::create_dir_all(&artifacts)?;
    ensure!(
        !artifacts.join("finish.json").exists(),
        "Use a fresh artifact directory for each manual check"
    );
    let fixture = Fixture::start()?;
    fs::write(
        artifacts.join("ready.json"),
        serde_json::to_vec_pretty(
            &json!({"url":fixture.url,"status":"ready","expiresAfterSeconds":1800}),
        )?,
    )?;
    eprintln!("Open {} in your current Firefox profile", fixture.url);
    let deadline = Instant::now() + Duration::from_secs(1800);
    while Instant::now() < deadline {
        if artifacts.join("finish.json").exists() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!("Current-profile fixture expired after 30 minutes; no browser was changed")
}

/// Holds a disposable Firefox fixture while the real desktop chat calls its MCP.
/// This harness never invokes MCP tools and never changes extension grant state
/// through internal APIs: grant/revoke/connect use the actual toolbar buttons.
#[test]
#[ignore = "requires geckodriver --allow-system-access and an actual Codex desktop chat"]
fn desktop_firefox_fixture() -> Result<()> {
    let artifacts = PathBuf::from(env::var("FIREFOX_DESKTOP_ARTIFACTS")?);
    fs::create_dir_all(&artifacts)?;
    let token = fs::read_to_string(env::var("FIREFOX_MCP_TOKEN_FILE")?)?;
    let legacy = legacy_extension()?;
    let fixture = Fixture::start()?;
    let driver = Driver::start()?;
    driver.call("/window/rect", json!({"width":1280,"height":900}))?;
    driver.call(
        "/moz/addon/install",
        json!({"path":legacy.path(),"temporary":true}),
    )?;
    driver.goto(&format!("{}/a", fixture.url))?;
    let mut tabs = vec![("A", driver.handle()?)];
    if env::var_os("FIREFOX_DESKTOP_MULTI").is_some() {
        let tab = driver.call("/window/new", json!({"type":"tab"}))?;
        let handle = tab["handle"]
            .as_str()
            .context("missing new tab handle")?
            .to_owned();
        driver.call("/window", json!({"handle":handle}))?;
        driver.goto(&format!("{}/b", fixture.url))?;
        tabs.push(("B", handle));
        driver.call("/window", json!({"handle":tabs[0].1}))?;
    }
    write_desktop_result(
        &artifacts,
        json!({"seq":0,"status":"fixture_ready","url":fixture.url}),
    )?;
    let deadline = Instant::now() + Duration::from_secs(1800);
    let mut last_seq = 0;
    while Instant::now() < deadline {
        let command = fs::read(artifacts.join("command.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        let Some(command) = command else {
            thread::sleep(Duration::from_millis(100));
            continue;
        };
        let seq = command["seq"]
            .as_u64()
            .context("missing command sequence")?;
        if seq <= last_seq {
            thread::sleep(Duration::from_millis(100));
            continue;
        }
        last_seq = seq;
        let action = command["action"]
            .as_str()
            .context("missing command action")?;
        let result = (|| -> Result<Value> {
            let tab_name = command["tab"].as_str().unwrap_or("A");
            let handle = &tabs
                .iter()
                .find(|(name, _)| *name == tab_name)
                .context("unknown fixture tab")?
                .1;
            driver.call("/window", json!({"handle":handle}))?;
            let mut popup_status = Value::Null;
            match action {
                "connect" | "enable" | "conflict" | "revoke" | "disconnect" | "forget"
                | "disconnectAll" => {
                    driver.popup()?;
                    if let Some(id) = command["clientId"].as_str() {
                        driver.select_client(id)?;
                    }
                    if action == "connect" {
                        driver.script(
                            "document.querySelector('#token').value=arguments[0];",
                            json!([token.trim()]),
                        )?;
                        if let Some(code) = command["connectionCode"].as_str() {
                            driver.script(
                                "document.querySelector('#connection').value=arguments[0];",
                                json!([code]),
                            )?;
                        }
                    }
                    driver.click(&format!(
                        "#{}",
                        if action == "conflict" {
                            "enable"
                        } else {
                            action
                        }
                    ))?;
                    driver.wait_status(match action {
                        "connect" => "Connected",
                        "enable" => "Enabled tab:",
                        "conflict" => "another chat",
                        "disconnect" | "disconnectAll" => "Disconnected",
                        _ => "No tab enabled.",
                    })?;
                    popup_status = driver.script(
                        "return document.querySelector('#status').textContent;",
                        json!([]),
                    )?;
                    save_desktop_screenshot(&driver, &artifacts, seq, "popup")?;
                    driver.close_popup()?;
                }
                "navigate" => driver.goto(&format!("{}/next", fixture.url))?,
                "inspect" => {}
                "finish" => {
                    write_desktop_result(&artifacts, json!({"seq":seq,"action":action,"ok":true}))?;
                    return Ok(Value::Null);
                }
                _ => bail!("unknown desktop fixture action"),
            }
            let page = driver.script("return {url:location.href, code:document.querySelector('#observation-code').textContent, count:document.querySelector('#count').textContent, message:document.querySelector('#message').value, status:document.querySelector('#state').textContent, scrollY};", json!([]))?;
            save_desktop_screenshot(&driver, &artifacts, seq, "page")?;
            let mut pages = serde_json::Map::new();
            for (name, tab_handle) in &tabs {
                driver.call("/window", json!({"handle":tab_handle}))?;
                pages.insert((*name).into(),driver.script("return {url:location.href,code:document.querySelector('#observation-code').textContent,count:document.querySelector('#count').textContent,message:document.querySelector('#message').value,status:document.querySelector('#state').textContent,scrollY};",json!([]))?);
            }
            driver.call("/window", json!({"handle":handle}))?;
            Ok(
                json!({"seq":seq,"action":action,"ok":true,"popupStatus":popup_status,"page":page,"pages":pages}),
            )
        })();
        if action == "finish" && result.is_ok() {
            return Ok(());
        }
        let value = match result {
            Ok(value) => value,
            Err(error) => {
                if driver.popup_open.get() {
                    let _ = driver.close_popup();
                }
                json!({"seq":seq,"action":action,"ok":false,"error":error.to_string()})
            }
        };
        fs::write(
            artifacts.join(format!("step-{seq}.json")),
            serde_json::to_vec_pretty(&value)?,
        )?;
        write_desktop_result(&artifacts, value)?;
    }
    bail!("desktop fixture timed out after 30 minutes")
}

fn write_desktop_result(directory: &std::path::Path, value: Value) -> Result<()> {
    let temporary = directory.join("result.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(&value)?)?;
    fs::rename(temporary, directory.join("result.json"))?;
    Ok(())
}

fn save_desktop_screenshot(
    driver: &Driver,
    directory: &std::path::Path,
    seq: u64,
    view: &str,
) -> Result<()> {
    let value: Value = driver
        .client
        .get(format!(
            "{}/session/{}/screenshot",
            driver.base, driver.session
        ))
        .send()?
        .json()?;
    let png = value["value"]
        .as_str()
        .context("missing WebDriver screenshot")?;
    // Preserve the protocol's PNG base64 without adding a production dependency.
    fs::write(directory.join(format!("step-{seq}-{view}.png.b64")), png)?;
    Ok(())
}

struct Driver {
    client: Client,
    base: String,
    session: String,
    popup_open: Cell<bool>,
}
impl Driver {
    fn handle(&self) -> Result<String> {
        let value: Value = self
            .client
            .get(format!("{}/session/{}/window", self.base, self.session))
            .send()?
            .json()?;
        Ok(value["value"]
            .as_str()
            .context("missing window handle")?
            .to_owned())
    }
    fn select_client(&self, id: &str) -> Result<()> {
        ensure!(
            id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid client id"
        );
        self.script("const select=document.querySelector('#clients'); select.value=arguments[0]; select.dispatchEvent(new Event('change',{bubbles:true}));",json!([id]))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let text = self.script(
                "return document.querySelector('#identity').textContent;",
                json!([]),
            )?;
            if text.as_str().is_some_and(|text| text.ends_with(id)) {
                return Ok(());
            }
            ensure!(Instant::now() < deadline, "chat selection did not update");
            thread::sleep(Duration::from_millis(20));
        }
    }
    fn start() -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(40))
            .no_proxy()
            .build()?;
        let base =
            env::var("FIREFOX_WEBDRIVER_URL").unwrap_or_else(|_| "http://127.0.0.1:4444".into());
        ensure!(
            base.starts_with("http://127.0.0.1:"),
            "WebDriver must be bound to loopback"
        );
        let value: Value = client.post(format!("{base}/session")).json(&json!({"capabilities":{"alwaysMatch":{
            "browserName":"firefox", "moz:firefoxOptions":{
                "args":["-headless"],
                "prefs":{"extensions.webextensions.uuids":json!({EXTENSION_ID:EXTENSION_UUID}).to_string()}
            }
        }}})).send()?.json()?;
        let session = value["value"]["sessionId"]
            .as_str()
            .with_context(|| format!("Firefox startup failed: {value}"))?
            .to_owned();
        eprintln!(
            "Firefox version: {}",
            value["value"]["capabilities"]["browserVersion"]
        );
        Ok(Self {
            client,
            base,
            session,
            popup_open: Cell::new(false),
        })
    }
    fn call(&self, path: &str, body: Value) -> Result<Value> {
        let value: Value = self
            .client
            .post(format!("{}/session/{}{}", self.base, self.session, path))
            .json(&body)
            .send()?
            .json()?;
        if value["value"].get("error").is_some() {
            bail!("WebDriver {path}: {}", value["value"]);
        }
        Ok(value["value"].clone())
    }
    fn script(&self, script: &str, args: Value) -> Result<Value> {
        if self.popup_open.get() {
            return self.popup_command("return actor.executeScript(arguments[0], arguments[1], {timeout:10000, newSandbox:true});", json!([script,args]));
        }
        self.call("/execute/sync", json!({"script":script,"args":args}))
    }
    fn popup_command(&self, command: &str, args: Value) -> Result<Value> {
        // Remote XUL popups are absent from WebDriver window/BiDi inventories.
        // Use Firefox's own WebDriver actor, retaining its system-access check.
        let script = format!(
            r#"
            const {{getMarionetteCommandsActorProxy}} = ChromeUtils.importESModule('chrome://remote/content/marionette/actors/MarionetteCommandsParent.sys.mjs');
            const actor=getMarionetteCommandsActorProxy(()=>document.querySelector('.webextension-popup-browser').browsingContext);
            {command}
        "#
        );
        self.call("/execute/sync", json!({"script":script,"args":args}))
    }
    fn goto(&self, url: &str) -> Result<()> {
        self.call("/url", json!({"url":url}))?;
        Ok(())
    }
    fn context(&self, context: &str) -> Result<()> {
        self.call("/moz/context", json!({"context":context}))?;
        Ok(())
    }
    fn element(&self, selector: &str) -> Result<Value> {
        self.call("/element", json!({"using":"css selector","value":selector}))
    }
    fn click(&self, selector: &str) -> Result<()> {
        if self.popup_open.get() {
            self.popup_command(r#"
                return actor.findElement('css selector', arguments[0], {timeout:5000,all:false})
                  .then(element=>actor.clickElement(element,new Map([['moz:accessibilityChecks',false],['moz:webdriverClick',true]])));
            "#,json!([selector]))?;
            return Ok(());
        }
        let element = self.element(selector)?;
        let id = element["element-6066-11e4-a52e-4f735466cecf"]
            .as_str()
            .context("missing element ID")?;
        self.call(&format!("/element/{id}/click"), json!({}))?;
        Ok(())
    }
    fn popup(&self) -> Result<()> {
        self.popup_for(EXTENSION_ID)
    }
    fn popup_for(&self, extension_id: &str) -> Result<()> {
        self.context("chrome")?;
        let widget = self.script(r#"
            const {CustomizableUI} = ChromeUtils.importESModule('moz-src:///browser/components/customizableui/CustomizableUI.sys.mjs');
            const id = arguments[0].toLowerCase().replace(/[^a-z0-9_-]/g,'_') + '-browser-action';
            CustomizableUI.addWidgetToArea(id, CustomizableUI.AREA_NAVBAR);
            return id;
        "#,json!([extension_id]))?;
        self.click(&format!(
            "#{} .unified-extensions-item-action-button",
            widget.as_str().context("missing toolbar widget")?
        ))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if self.element(".webextension-popup-browser").is_ok() {
                break;
            }
            if Instant::now() >= deadline {
                let detail=self.script("return Array.from(document.querySelectorAll('browser,panel')).map(e=>({tag:e.tagName,id:e.id,cls:e.className,src:e.getAttribute('src'),state:e.state}));",json!([]))?;
                bail!("extension popup did not open: {detail}");
            }
            thread::sleep(Duration::from_millis(50));
        }
        self.popup_open.set(true);
        eprintln!(
            "Popup frame: {}",
            self.script("return location.href;", json!([]))?
        );
        Ok(())
    }
    fn close_popup(&self) -> Result<()> {
        self.popup_open.set(false);
        self.context("chrome")?;
        self.call("/actions",json!({"actions":[{"type":"key","id":"keyboard","actions":[{"type":"keyDown","value":"\u{e00c}"},{"type":"keyUp","value":"\u{e00c}"}]}]}))?;
        self.context("content")?;
        self.call("/frame", json!({"id":null}))?;
        self.settle_rendering()?;
        Ok(())
    }
    fn settle_rendering(&self) -> Result<()> {
        // Scroll/resize events can be delivered after an instantaneous DOM action.
        self.script("return new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve(true))));",json!([]))?;
        Ok(())
    }
    fn wait_status(&self, text: &str) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let status = self.script(
                "return document.querySelector('#status')?.textContent;",
                json!([]),
            )?;
            if status.as_str().is_some_and(|s| s.contains(text)) {
                return Ok(());
            }
            ensure!(
                Instant::now() < deadline,
                "Expected popup status {text}, got {status}"
            );
            thread::sleep(Duration::from_millis(100));
        }
    }
}
impl Drop for Driver {
    fn drop(&mut self) {
        let _ = self
            .client
            .delete(format!("{}/session/{}", self.base, self.session))
            .send();
    }
}

struct Server {
    child: Child,
    input: ChildStdin,
    output: Receiver<std::io::Result<String>>,
    id: u64,
    thread_id: Option<String>,
}
impl Server {
    fn start(token_file: &std::path::Path) -> Result<Self> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_firefox-codex-mcp"));
        command.env("FIREFOX_MCP_TOKEN_FILE", token_file);
        Self::from_command(command)
    }
    fn from_command(mut command: Command) -> Result<Self> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let input = child.stdin.take().context("missing MCP stdin")?;
        let stdout = child.stdout.take().context("missing MCP stdout")?;
        let (send, output) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        let mut server = Self {
            child,
            input,
            output,
            id: 0,
            thread_id: None,
        };
        server.rpc("initialize",json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"firefox-e2e","version":"1"}}))?;
        writeln!(
            server.input,
            "{}",
            json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )?;
        server.input.flush()?;
        Ok(server)
    }
    fn rpc(&mut self, method: &str, params: Value) -> Result<Value> {
        self.id += 1;
        writeln!(
            self.input,
            "{}",
            json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params})
        )?;
        self.input.flush()?;
        let line = self
            .output
            .recv_timeout(Duration::from_secs(20))
            .context("MCP response deadline exceeded")??;
        let response: Value = serde_json::from_str(&line).context("invalid MCP response")?;
        ensure!(
            response["id"] == self.id && response.get("error").is_none(),
            "MCP failed: {response}"
        );
        Ok(response["result"].clone())
    }
    fn tool(&mut self, name: &str, args: Value) -> Result<Value> {
        let mut params = json!({"name":name,"arguments":args});
        if let Some(id) = &self.thread_id {
            params["_meta"] = json!({"threadId":id});
        }
        self.rpc("tools/call", params)
    }
    fn ok(&mut self, name: &str, args: Value) -> Result<Value> {
        let result = self.tool(name, args)?;
        ensure!(result["isError"] != true, "{name} failed: {result}");
        Ok(serde_json::from_str(
            result["content"][0]["text"]
                .as_str()
                .context("missing tool content")?,
        )?)
    }
    fn denied(&mut self, name: &str, args: Value, expected: &str) -> Result<()> {
        let result = self.tool(name, args)?;
        ensure!(
            result["isError"] == true && result.to_string().contains(expected),
            "Expected {name} rejection containing {expected}: {result}"
        );
        Ok(())
    }
    fn wait_connected(&mut self, connected: bool) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if self.ok("firefox_status", json!({}))?["connected"] == connected {
                return Ok(());
            }
            ensure!(Instant::now() < deadline, "Expected connected={connected}");
            thread::sleep(Duration::from_millis(50));
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn concurrent_stdio_sessions_have_distinct_ports_and_identities() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let token = scratch.path().join("bridge.token");
    fs::write(&token, "ab".repeat(32))?;
    let mut a = Server::start(&token)?;
    let mut b = Server::start(&token)?;
    let sa = a.ok("firefox_status", json!({}))?;
    let sb = b.ok("firefox_status", json!({}))?;
    ensure!(
        sa["clientId"] != sb["clientId"],
        "clients shared an identity"
    );
    let ca = sa["connectionCode"].as_str().context("missing A code")?;
    let cb = sb["connectionCode"].as_str().context("missing B code")?;
    ensure!(
        ca.split(':').next() != cb.split(':').next(),
        "clients shared a port"
    );
    drop(a);
    ensure!(
        b.ok("firefox_status", json!({}))?["clientId"] == sb["clientId"],
        "A exit changed B identity"
    );
    let mut replacement = Server::start(&token)?;
    ensure!(
        replacement.ok("firefox_status", json!({}))?["clientId"] != sa["clientId"],
        "restart reused identity"
    );
    Ok(())
}

struct Fixture {
    url: String,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Fixture {
    fn start() -> Result<Self> {
        Self::with_page(FIXTURE)
    }
    fn with_page(page: &'static str) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        // Deliberately NOT the manifest's 127.0.0.1 host permission: needs activeTab.
        let url = format!("http://localhost:{}", listener.local_addr()?.port());
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let worker = thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let mut buffer = [0u8; 4096];
                        let _ = stream.read(&mut buffer);
                        let _ = write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",page.len(),page);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            url,
            stop,
            worker: Some(worker),
        })
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn element(snapshot: &Value, label: &str) -> Result<Value> {
    snapshot["elements"]
        .as_array()
        .context("missing elements")?
        .iter()
        .find(|element| element["label"] == label)
        .map(|element| element["id"].clone())
        .with_context(|| format!("No element labelled {label}: {snapshot}"))
}
fn action(snapshot: &Value, label: &str) -> Result<Value> {
    Ok(json!({"snapshotId":snapshot["snapshotId"],"elementId":element(snapshot,label)?}))
}

fn observation_roundtrip(driver: &Driver, fixture: &Fixture, server: &mut Server) -> Result<()> {
    driver.goto(&format!("{}/observation", fixture.url))?;
    driver.popup()?;
    driver.click("#enable")?;
    driver.wait_status("Enabled tab:")?;
    driver.close_popup()?;
    let snapshot = server.ok("firefox_snapshot", json!({}))?;
    let encoded = snapshot.to_string();
    ensure!(
        snapshot["text"]
            .as_str()
            .is_some_and(|text| text.contains("Rendered body observation")
                && text.contains("Observation code:")),
        "Missing body text: {snapshot}"
    );
    ensure!(
        snapshot["headings"][0]["text"] == "Firefox MCP E2E fixture",
        "Missing heading"
    );
    ensure!(
        snapshot["statuses"][0]["text"] == "Ready",
        "Missing live status"
    );
    ensure!(snapshot["readyState"] == "complete", "Missing ready state");
    for excluded in ["PRIVATE_", "HIDDEN_", "OUTSIDE_VIEWPORT_TEXT"] {
        ensure!(
            !encoded.contains(excluded),
            "Snapshot exposed excluded content: {excluded}"
        );
    }
    ensure!(
        encoded.contains("Visible clipped line") && encoded.contains("Visible prefix"),
        "Partially clipped text lost its visible prefix"
    );
    for (label, key, value) in [
        ("Disabled action", "disabled", json!(true)),
        ("Read only field", "readOnly", json!(true)),
        ("Toggle details", "expanded", json!("false")),
    ] {
        let id = element(&snapshot, label)?;
        let control = snapshot["elements"]
            .as_array()
            .context("missing elements")?
            .iter()
            .find(|element| element["id"] == id)
            .context("missing control")?;
        ensure!(control[key] == value, "Wrong state for {label}: {control}");
    }
    let tick = driver.script("return document.documentElement.dataset.tick;", json!([]))?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while driver.script("return document.documentElement.dataset.tick;", json!([]))? == tick {
        ensure!(
            Instant::now() < deadline,
            "Fixture clock did not mutate DOM"
        );
        thread::sleep(Duration::from_millis(30));
    }
    server.ok("firefox_click", action(&snapshot, "Count 0")?)?;
    ensure!(
        driver.script("return document.documentElement.dataset.clicks;", json!([]))? == "1",
        "Clock mutation blocked unchanged target"
    );
    eprintln!("PASS rendered body/headings/status, private-buffer exclusion, control states and action during continuous unrelated mutations");
    ensure!(driver.script("const e=document.querySelector('#wrapped'),r=e.getBoundingClientRect(),hit=document.elementFromPoint((r.left+r.right)/2,(r.top+r.bottom)/2);return hit!==e&&!e.contains(hit);",json!([]))?==true,"Wrapped-link fixture must have a union-box center outside its fragments");
    let snapshot = server.ok("firefox_snapshot", json!({}))?;
    server.ok(
        "firefox_click",
        action(&snapshot, "A wrapped link label that takes two lines.")?,
    )?;
    ensure!(
        driver.script(
            "return document.querySelector('#wrapped').dataset.clicked;",
            json!([])
        )? == "yes",
        "Visible wrapped link was incorrectly blocked"
    );

    for (name,change,restore,expected) in [
        ("target label","document.querySelector('#count').textContent='Unexpected action';","document.querySelector('#count').textContent='Count 1';","Stale snapshot"),
        ("form destination","document.querySelector('#test-form').action='/changed';","document.querySelector('#test-form').action='/submit';","Stale snapshot"),
        ("nearby warning","document.querySelector('#context-note').textContent='Price increased';","document.querySelector('#context-note').textContent='This is a disposable test form.';","Stale snapshot"),
        ("geometry","document.querySelector('#count').style.transform='translateX(20px)';","document.querySelector('#count').style.transform='';","Stale snapshot"),
        ("disabled","document.querySelector('#count').disabled=true;","document.querySelector('#count').disabled=false;","no longer available"),
        ("busy form","document.querySelector('#test-form').setAttribute('aria-busy','true');","document.querySelector('#test-form').removeAttribute('aria-busy');","no longer available"),
        ("new status","document.querySelector('#state').textContent='A new warning';","document.querySelector('#state').textContent='Count updated to 1';","Stale snapshot"),
        ("new modal","const d=document.createElement('div'); d.id='test-overlay'; d.setAttribute('aria-modal','true'); d.style='position:fixed;right:0;bottom:0;width:100px;height:100px';d.textContent='Confirm';document.body.append(d);","document.querySelector('#test-overlay').remove();","Stale snapshot"),
        ("covering overlay","const r=document.querySelector('#count').getBoundingClientRect(); const d=document.createElement('div');d.id='test-overlay';Object.assign(d.style,{position:'fixed',left:r.left+'px',top:r.top+'px',width:r.width+'px',height:r.height+'px',zIndex:'9999',background:'red'});document.body.append(d);","document.querySelector('#test-overlay').remove();","no longer available"),
        ("replaced node","const e=document.querySelector('#count');e.replaceWith(e.cloneNode(true));","","no longer available"),
    ] {
        let snapshot=server.ok("firefox_snapshot",json!({}))?;
        let args=action(&snapshot,"Count 1")?;
        driver.script(change,json!([]))?;
        server.denied("firefox_click",args,expected).with_context(||format!("Guard failed: {name}"))?;
        ensure!(driver.script("return document.documentElement.dataset.clicks;",json!([]))?=="1","Rejected {name} action still clicked");
        driver.script(restore,json!([]))?;
    }
    let snapshot = server.ok("firefox_snapshot", json!({}))?;
    driver.script(
        "document.querySelector('#next').href='/changed';",
        json!([]),
    )?;
    server.denied(
        "firefox_click",
        action(&snapshot, "Navigate to next document")?,
        "Stale snapshot",
    )?;
    driver.script("document.querySelector('#next').href='/next';", json!([]))?;
    let snapshot = server.ok("firefox_snapshot", json!({}))?;
    driver.script(
        "document.querySelector('#message').value='New user edit';",
        json!([]),
    )?;
    let mut args = action(&snapshot, "Message")?;
    args["text"] = json!("Must not overwrite");
    server.denied("firefox_type", args, "Stale snapshot")?;
    ensure!(
        driver.script(
            "return document.querySelector('#message').value;",
            json!([])
        )? == "New user edit",
        "Overwrote changed field"
    );
    let snapshot = server.ok("firefox_snapshot", json!({}))?;
    server.ok("firefox_click", action(&snapshot, "Count 1")?)?;
    let snapshot = server.ok("firefox_snapshot", json!({}))?;
    ensure!(
        snapshot["statuses"][0]["text"] == "Count updated to 2",
        "Fresh observation did not recover status/action"
    );
    driver.script("const p=document.createElement('p');p.style='position:fixed;top:0;left:0;width:1000px;font:8px/9px sans-serif';p.textContent='Bounded text '.repeat(2000);document.body.append(p);",json!([]))?;
    let bounded = server.ok("firefox_snapshot", json!({}))?;
    ensure!(
        bounded["truncation"]["text"] == true
            && bounded["text"]
                .as_str()
                .is_some_and(|text| text.len() <= 12000),
        "Text budget not enforced"
    );
    eprintln!("PASS target/URL/form/context/geometry/disabled/busy/status/modal/overlay/replacement/edit guards, fresh recovery and text budget");
    Ok(())
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn find_connection_code(value: &Value) -> Option<String> {
    match value {
        Value::Object(fields) => fields
            .get("connectionCode")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| fields.values().find_map(find_connection_code)),
        Value::Array(values) => values.iter().find_map(find_connection_code),
        Value::String(text) if text.starts_with('{') || text.starts_with('[') => {
            serde_json::from_str::<Value>(text)
                .ok()
                .and_then(|value| find_connection_code(&value))
        }
        _ => None,
    }
}

fn codex_roundtrip(
    driver: &Driver,
    fixture: &Fixture,
    scratch: &std::path::Path,
    token_file: &std::path::Path,
    executable: &str,
) -> Result<()> {
    driver.goto(&format!("{}/codex", fixture.url))?;
    let config = format!(
        "mcp_servers.firefox={{command={},env={{FIREFOX_MCP_TOKEN_FILE={}}},required=true}}",
        serde_json::to_string(env!("CARGO_BIN_EXE_firefox-codex-mcp"))?,
        serde_json::to_string(&token_file.to_string_lossy())?
    );
    let prompt="This is an explicitly authorized integration test of the local Firefox MCP server and a disposable fixture page. Use only firefox MCP tools; no shell, files, other servers or navigation. The harness will enable the fixture shortly. First call firefox_status and firefox_snapshot. If disconnected or no tab enabled, retry status/snapshot at most 10 times. Read the random Observation code from the snapshot's body text (it is not a control label). (1) Click Count 0 exactly once, despite the unrelated clock continually updating. (2) Take a fresh snapshot and verify its status text says Count updated to 1; type the exact observed random code alone into Message. (3) Take a new snapshot, scroll down 600 pixels, and take a final snapshot. Use fresh snapshotId and returned elementId. If rejected as stale BEFORE dispatch, take a fresh snapshot and retry at most twice; never repeat an unknown-outcome action. Finish by stating the observed code and status.";
    let mut child = ChildGuard(
        Command::new(executable)
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
            .current_dir(scratch)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?,
    );
    let stdout = child.0.stdout.take().context("missing Codex stdout")?;
    let stderr = child.0.stderr.take().context("missing Codex stderr")?;
    let artifact_dir = env::var_os("FIREFOX_E2E_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| scratch.to_path_buf());
    fs::create_dir_all(&artifact_dir)?;
    let mut transcript = fs::File::create(artifact_dir.join("codex.jsonl"))?;
    let mut diagnostics = fs::File::create(artifact_dir.join("codex.stderr.log"))?;
    thread::spawn(move || {
        let _ = std::io::copy(&mut BufReader::new(stderr), &mut diagnostics);
    });
    let (send, receive) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if send.send(line).is_err() {
                break;
            }
        }
    });
    // The model's first status exposes the public code, never the private token.
    let pairing_deadline = Instant::now() + Duration::from_secs(90);
    let mut pairing_events = std::collections::VecDeque::new();
    let pairing_code = loop {
        ensure!(
            Instant::now() < pairing_deadline,
            "Codex did not report its pairing code"
        );
        let line = receive.recv_timeout(Duration::from_secs(20))??;
        let event: Value = serde_json::from_str(&line)?;
        pairing_events.push_back(line);
        if let Some(code) = find_connection_code(&event) {
            break code;
        }
    };
    driver.popup()?;
    driver.script(
        "document.querySelector('#connection').value=arguments[0];",
        json!([pairing_code]),
    )?;
    driver.click("#connect")?;
    driver.wait_status("Connected")?;
    driver.click("#enable")?;
    driver.wait_status("Enabled tab:")?;
    driver.close_popup()?;
    let deadline = Instant::now() + Duration::from_secs(240);
    let mut calls = Vec::new();
    let mut completed = false;
    let mut body_observed = false;
    let mut status_observed = false;
    loop {
        ensure!(
            Instant::now() < deadline,
            "Codex E2E exceeded four minutes; see {}",
            artifact_dir.display()
        );
        // Preserve the initial status in the transcript and tool-call assertions.
        let received = match pairing_events.pop_front() {
            Some(line) => Ok(Ok(line)),
            None => receive.recv_timeout(Duration::from_millis(250)),
        };
        match received {
            Ok(line) => {
                let line = line?;
                writeln!(transcript, "{line}")?;
                let event: Value = serde_json::from_str(&line)?;
                if event["type"] == "item.completed" && event["item"]["type"] == "mcp_tool_call" {
                    let item = &event["item"];
                    eprintln!("Codex MCP: {} {}", item["tool"], item["status"]);
                    if item["status"] == "completed"
                        && item["error"].is_null()
                        && item["result"]["isError"] != true
                    {
                        calls.push(item["tool"].as_str().unwrap_or_default().to_owned());
                    }
                    if item["tool"] == "firefox_snapshot" {
                        if let Some(text) = item["result"]["content"][0]["text"].as_str() {
                            if let Ok(snapshot) = serde_json::from_str::<Value>(text) {
                                body_observed |= snapshot["text"]
                                    .as_str()
                                    .is_some_and(|text| text.contains("Observation code:"));
                                status_observed |=
                                    snapshot["statuses"].as_array().is_some_and(|items| {
                                        items
                                            .iter()
                                            .any(|item| item["text"] == "Count updated to 1")
                                    });
                            }
                        }
                    }
                }
                if event["type"] == "turn.completed" {
                    completed = true;
                }
                if event["type"] == "turn.failed" || event["type"] == "error" {
                    bail!("Codex failed: {event}");
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    ensure!(
        child.0.wait()?.success() && completed,
        "Codex did not finish successfully; see {}",
        artifact_dir.display()
    );
    for name in [
        "firefox_status",
        "firefox_snapshot",
        "firefox_click",
        "firefox_type",
        "firefox_scroll",
    ] {
        ensure!(
            calls.iter().any(|tool| tool == name),
            "Codex did not call {name}; see {}",
            artifact_dir.display()
        );
    }
    ensure!(
        driver.script(
            "return document.querySelector('#count').textContent;",
            json!([])
        )? == "Count 1",
        "Codex did not click exactly once"
    );
    let expected_code = driver.script(
        "return document.querySelector('#observation-code').textContent.split(': ')[1];",
        json!([]),
    )?;
    ensure!(
        driver.script(
            "return document.querySelector('#message').value;",
            json!([])
        )? == expected_code,
        "Codex did not type the code read from rendered body text"
    );
    ensure!(
        body_observed && status_observed,
        "Codex did not observe body/status text through MCP"
    );
    ensure!(
        driver
            .script("return scrollY;", json!([]))?
            .as_i64()
            .is_some_and(|y| y > 0),
        "Codex did not scroll"
    );
    fs::write(
        artifact_dir.join("verified.json"),
        serde_json::to_vec_pretty(&json!({
            "passed": true, "successfulToolCalls": calls, "bodyCodeObservedAndTyped": true,
            "statusObserved": status_observed, "clickCount": 1, "scrolled": true,
            "scope": "Actual Codex CLI via standalone MCP; not native desktop Computer Use settings"
        }))?,
    )?;
    eprintln!("PASS actual Codex CLI → stdio MCP → Firefox: body code/status observation, click/type/scroll during continuous DOM updates; transcript {}",artifact_dir.join("codex.jsonl").display());
    Ok(())
}

#[test]
#[ignore = "requires Firefox and a loopback geckodriver; see docs/validation.md"]
fn real_firefox_mcp() -> Result<()> {
    let legacy = legacy_extension()?;
    let scratch = tempfile::tempdir()?;
    let token_file = scratch.path().join("bridge.token");
    let generated = Command::new(env!("CARGO_BIN_EXE_firefox-codex-mcp"))
        .arg("--generate-token")
        .output()?;
    ensure!(generated.status.success(), "token generator failed");
    fs::write(&token_file, &generated.stdout)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&token_file, fs::Permissions::from_mode(0o600))?;
    }
    let token = String::from_utf8(generated.stdout)?.trim().to_owned();
    let mut server = Server::start(&token_file)?;
    let identity = server.ok("firefox_status", json!({}))?;
    let fixture = Fixture::start()?;
    let driver = Driver::start()?;
    driver.call(
        "/moz/addon/install",
        json!({"path":legacy.path(),"temporary":true}),
    )?;
    driver.goto(&fixture.url)?;
    driver.popup()?;
    driver.script(
        "document.querySelector('#connection').value=arguments[0];",
        json!([identity["connectionCode"]]),
    )?;
    driver.script(
        "document.querySelector('#token').value=arguments[0];",
        json!([token]),
    )?;
    driver.click("#connect")?;
    driver.wait_status("Connected")?;
    server.wait_connected(true)?;
    eprintln!("Actual Firefox extension connected to Rust MCP");
    server.denied("firefox_snapshot", json!({}), "No tab enabled")?;
    driver.click("#enable")?;
    driver.wait_status("Enabled tab:")?;
    driver.close_popup()?;
    let snapshot = server.ok("firefox_snapshot", json!({}))?;
    let click = action(&snapshot, "Count 0")?;
    server.ok("firefox_click", click.clone())?;
    ensure!(
        driver.script(
            "return document.querySelector('#count').textContent;",
            json!([])
        )? == "Count 1",
        "Click did not affect actual page"
    );
    server.denied("firefox_click", click, "Stale snapshot")?;
    let snapshot = server.ok("firefox_snapshot", json!({}))?;
    let mut typing = action(&snapshot, "Message")?;
    typing["text"] = json!("Firefox MCP verified");
    server.ok("firefox_type", typing)?;
    ensure!(
        driver.script(
            "return document.querySelector('#message').value;",
            json!([])
        )? == "Firefox MCP verified",
        "Type did not affect actual input"
    );
    let snapshot = server.ok("firefox_snapshot", json!({}))?;
    server.ok(
        "firefox_scroll",
        json!({"snapshotId":snapshot["snapshotId"],"y":1200}),
    )?;
    ensure!(
        driver
            .script("return scrollY;", json!([]))?
            .as_i64()
            .is_some_and(|y| y > 0),
        "Scroll did not move actual viewport"
    );
    server.denied(
        "firefox_scroll",
        json!({"snapshotId":snapshot["snapshotId"],"y":-1200}),
        "Stale snapshot",
    )?;
    driver.settle_rendering()?;
    let snapshot = server.ok("firefox_snapshot", json!({}))?;
    server.ok(
        "firefox_scroll",
        json!({"snapshotId":snapshot["snapshotId"],"y":-1200}),
    )?;
    driver.settle_rendering()?;
    eprintln!("PASS actual page click, type, scroll and consumed snapshot rejection");

    let old = server.ok("firefox_snapshot", json!({}))?;
    let old_click = action(&old, "Count 1")?;
    driver.popup()?;
    driver.click("#revoke")?;
    driver.wait_status("No tab enabled.")?;
    let state = server.ok("firefox_status", json!({}))?;
    ensure!(
        state["connected"] == true && state["tabEnabled"] == false,
        "Independent revoke changed transport or retained permission"
    );
    server.denied("firefox_snapshot", json!({}), "No tab enabled")?;
    server.denied("firefox_click", old_click.clone(), "No tab enabled")?;
    driver.click("#enable")?;
    driver.wait_status("Enabled tab:")?;
    server.denied("firefox_click", old_click.clone(), "Stale snapshot")?;
    eprintln!(
        "PASS independent revoke keeps connection, denies reads/actions, and fences old snapshot"
    );
    driver.click("#disconnect")?;
    driver.wait_status("Disconnected")?;
    server.wait_connected(false)?;
    server.denied("firefox_click", old_click.clone(), "not connected")?;
    driver.click("#connect")?;
    driver.wait_status("Connected")?;
    server.wait_connected(true)?;
    server.denied("firefox_snapshot", json!({}), "No tab enabled")?;
    driver.click("#enable")?;
    driver.wait_status("Enabled tab:")?;
    driver.close_popup()?;
    // No intervening snapshot: regrant itself must invalidate the old one.
    server.denied("firefox_click", old_click, "Stale snapshot")?;
    ensure!(
        driver.script(
            "return document.querySelector('#count').textContent;",
            json!([])
        )? == "Count 1",
        "Old grant clicked page"
    );
    let fresh = server.ok("firefox_snapshot", json!({}))?;
    ensure!(
        fresh["context"]["documentId"] == old["context"]["documentId"],
        "Expected same document on regrant"
    );
    ensure!(
        fresh["context"]["grantId"] != old["context"]["grantId"]
            && fresh["context"]["sessionId"] != old["context"]["sessionId"],
        "Grant/session IDs were reused"
    );
    server.ok("firefox_click", action(&fresh, "Count 1")?)?;
    ensure!(
        driver.script(
            "return document.querySelector('#count').textContent;",
            json!([])
        )? == "Count 2",
        "Regranted click failed"
    );
    eprintln!(
        "PASS disconnect/reconnect requires manual enable; old snapshot rejected on same document"
    );

    let previous = server.ok("firefox_snapshot", json!({}))?;
    driver.goto(&format!("{}/next", fixture.url))?;
    server.denied("firefox_snapshot", json!({}), "No tab enabled")?;
    server.denied(
        "firefox_click",
        action(&previous, "Count 2")?,
        "No tab enabled",
    )?;
    driver.popup()?;
    driver.click("#enable")?;
    driver.wait_status("Enabled tab:")?;
    driver.close_popup()?;
    let next = server.ok("firefox_snapshot", json!({}))?;
    ensure!(
        next["context"]["documentId"] != previous["context"]["documentId"],
        "Navigation reused document ID"
    );
    driver.call("/back", json!({}))?;
    server.denied("firefox_snapshot", json!({}), "No tab enabled")?;
    driver.popup()?;
    driver.click("#enable")?;
    driver.wait_status("Enabled tab:")?;
    driver.close_popup()?;
    server.ok("firefox_snapshot", json!({}))?;
    eprintln!("PASS navigation/back revokes access; manual enable restores it");
    observation_roundtrip(&driver, &fixture, &mut server)?;
    drop(server);
    if let Ok(executable) = env::var("FIREFOX_E2E_CODEX") {
        codex_roundtrip(&driver, &fixture, scratch.path(), &token_file, &executable)?;
    }
    Ok(())
}

fn legacy_extension() -> Result<tempfile::TempDir> {
    let dir = tempfile::tempdir()?;
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("repository")?
        .join("extension");
    for name in [
        "background.js",
        "protocol.js",
        "observation.js",
        "page.js",
        "content.js",
        "popup.html",
        "popup.js",
    ] {
        fs::copy(source.join(name), dir.path().join(name))?;
    }
    fs::copy(
        source.join("manifest-legacy.json"),
        dir.path().join("manifest.json"),
    )?;
    Ok(dir)
}
