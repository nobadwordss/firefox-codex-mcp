//! Direct BiDi tests: no extension, no Marionette, no system-access permission.
use super::*;
use futures_util::{SinkExt, StreamExt};
use std::os::unix::process::CommandExt;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

struct Firefox {
    _child: ChildGuard,
    address: String,
}
impl Firefox {
    fn start(root: &std::path::Path) -> Result<Self> {
        let profile = root.join("profile");
        fs::create_dir_all(&profile)?;
        if !profile.join("user.js").exists() {
            fs::write(
                profile.join("user.js"),
                r#"
user_pref("remote.prefs.recommended", false);
user_pref("browser.shell.checkDefaultBrowser", false);
user_pref("browser.startup.homepage", "about:blank");
user_pref("browser.aboutwelcome.enabled", false);
user_pref("browser.startup.homepage_override.mstone", "ignore");
user_pref("datareporting.policy.dataSubmissionEnabled", false);
"#,
            )?;
        }
        let mut command =
            Command::new(env::var("FIREFOX_BINARY").unwrap_or_else(|_| "/usr/bin/firefox".into()));
        command
            .args(["--no-remote", "--profile"])
            .arg(&profile)
            .args([
                "--remote-debugging-port",
                "0",
                "--width",
                "1280",
                "--height",
                "900",
            ])
            .env_remove("MOZ_REMOTE_ALLOW_SYSTEM_ACCESS")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        if env::var_os("FIREFOX_BIDI_VISIBLE").is_none() {
            command.arg("--headless");
        }
        let mut child = command.spawn()?;
        let stderr = child.stderr.take().context("missing Firefox stderr")?;
        let log = fs::File::create(root.join("firefox.log"))?;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut log = log;
            for line in BufReader::new(stderr)
                .lines()
                .map_while(std::result::Result::ok)
            {
                let _ = writeln!(log, "{line}");
                if let Some((_, address)) = line.split_once("WebDriver BiDi listening on ") {
                    let _ = tx.send(format!("{}/session", address.trim().trim_end_matches('/')));
                }
            }
        });
        let guard = ChildGuard(child);
        let address = rx
            .recv_timeout(Duration::from_secs(25))
            .context("Firefox BiDi did not start; inspect firefox.log")?;
        Ok(Self {
            _child: guard,
            address,
        })
    }
}
struct Raw {
    runtime: tokio::runtime::Runtime,
    socket: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    id: u64,
}
impl Raw {
    fn connect(address: &str) -> Result<Self> {
        let runtime = tokio::runtime::Runtime::new()?;
        let (socket, _) = runtime.block_on(connect_async(address))?;
        let mut raw = Self {
            runtime,
            socket,
            id: 0,
        };
        raw.call("session.new", json!({"capabilities":{}}))?;
        Ok(raw)
    }
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.id += 1;
        let id = self.id;
        self.runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(20), async {
                self.socket
                    .send(Message::Text(
                        json!({"id":id,"method":method,"params":params})
                            .to_string()
                            .into(),
                    ))
                    .await?;
                while let Some(message) = self.socket.next().await {
                    if let Message::Text(text) = message? {
                        let reply: Value = serde_json::from_str(&text)?;
                        if reply["id"] == id {
                            ensure!(reply["type"] == "success", "BiDi failed: {reply}");
                            return Ok(reply["result"].clone());
                        }
                    }
                }
                bail!("BiDi connection ended")
            })
            .await?
        })
    }
    fn script(&mut self, context: &Value, expression: &str) -> Result<Value> {
        let reply = self.call(
            "script.evaluate",
            json!({"expression":expression,"target":{"context":context},"awaitPromise":true}),
        )?;
        ensure!(reply["type"] == "success", "Script failed: {reply}");
        Ok(reply["result"]["value"].clone())
    }
    fn end(mut self) -> Result<()> {
        self.call("session.end", json!({}))?;
        Ok(())
    }
}
fn server(root: &std::path::Path, _address: &str) -> Result<Server> {
    let mut command = Command::new(
        env::var_os("FIREFOX_TEST_SERVER_BINARY")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_firefox-codex-mcp").into()),
    );
    command
        .arg("--bidi")
        .process_group(0)
        .env_remove("FIREFOX_MCP_BIDI_URL")
        .env("FIREFOX_MCP_PROFILE", root.join("profile"))
        .env("FIREFOX_MCP_RUNTIME_DIR", root.join("runtime"));
    Server::from_command(command)
}

#[test]
#[ignore = "starts a disposable Firefox and tests launcher process-group lifetime"]
fn launcher_lifetime_e2e() -> Result<()> {
    let root = tempfile::tempdir()?;
    fs::write(root.path().join("prefs.js"), "")?;
    fs::write(root.path().join("user.js"), "user_pref(\"remote.prefs.recommended\", false);\nuser_pref(\"browser.aboutwelcome.enabled\", false);\n")?;
    // A crashed Firefox may leave a stale descriptor. The launcher must replace
    // it with the newly started browser's endpoint rather than reuse its port.
    fs::write(
        root.path().join("WebDriverBiDiServer.json"),
        r#"{"ws_host":"127.0.0.1","ws_port":9}"#,
    )?;
    let child = Command::new(env!("CARGO_BIN_EXE_firefox-codex-mcp"))
        .arg("--launch-firefox")
        .arg(root.path())
        .env("MOZ_HEADLESS", "1")
        .process_group(0)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let launcher_pid = child.id();
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "Launcher failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout)?;
    let firefox_pid = i32::try_from(result["pid"].as_u64().context("Missing browser PID")?)?;
    struct Cleanup(i32);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            unsafe {
                libc::kill(self.0, libc::SIGTERM);
            }
            for _ in 0..100 {
                if unsafe { libc::kill(self.0, 0) } != 0 {
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
    let _cleanup = Cleanup(firefox_pid);
    // A terminal/session may clean up the launcher's process group after exit.
    // Firefox must be in a different group and remain usable afterwards.
    unsafe {
        libc::kill(-(launcher_pid as i32), libc::SIGTERM);
    }
    thread::sleep(Duration::from_millis(150));
    let descriptor: Value =
        serde_json::from_slice(&fs::read(root.path().join("WebDriverBiDiServer.json"))?)?;
    let address = format!(
        "ws://127.0.0.1:{}/session",
        descriptor["ws_port"]
            .as_u64()
            .context("Missing debug port")?
    );
    let mut raw = Raw::connect(&address)?;
    raw.call("session.status", json!({}))?;
    raw.end()?;
    Ok(())
}

#[test]
#[ignore = "launches a real disposable Firefox with WebDriver BiDi"]
fn debug_broker_e2e() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let root = env::var_os("FIREFOX_BIDI_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| scratch.path().join("artifacts"));
    fs::create_dir_all(&root)?;
    let fixture = Fixture::start()?;
    let firefox = Firefox::start(&root)?;
    let mut raw = Raw::connect(&firefox.address)?;
    let baseline = raw.call("browsingContext.create", json!({"type":"tab"}))?["context"].clone();
    raw.call(
        "browsingContext.navigate",
        json!({"context":baseline,"url":format!("{}/user",fixture.url),"wait":"complete"}),
    )?;
    raw.script(
        &baseline,
        "localStorage.setItem('bidi_test_login','preserved'); 'ok'",
    )?;
    raw.end()?;

    let mut a = server(&root, &firefox.address)?;
    let mut b = server(&root, &firefox.address)?;
    ensure!(
        a.rpc("tools/list", json!({}))?["tools"]
            .as_array()
            .is_some_and(|tools| tools.len() == 11),
        "Missing debugging tools"
    );
    ensure!(
        a.ok("firefox_status", json!({}))?["connected"] == true,
        "Debugging unavailable"
    );
    ensure!(
        b.ok("firefox_tabs", json!({}))?["tabs"] == json!([]),
        "Existing tab was imported"
    );
    a.denied("firefox_select_tab", json!({"tabId":baseline}), "not owned")?;
    let tab_a = a.ok(
        "firefox_open_tab",
        json!({"url":format!("{}/a",fixture.url)}),
    )?["tabId"]
        .clone();
    let tab_b = b.ok(
        "firefox_open_tab",
        json!({"url":format!("{}/b",fixture.url)}),
    )?["tabId"]
        .clone();
    ensure!(
        tab_a != tab_b && tab_a != baseline && tab_b != baseline,
        "Tab ownership collision"
    );
    b.denied("firefox_select_tab", json!({"tabId":tab_a}), "not owned")?;
    let a_before = a.ok("firefox_snapshot", json!({}))?;
    let b_before = b.ok("firefox_snapshot", json!({}))?;
    ensure!(
        a_before["text"]
            .as_str()
            .is_some_and(|text| text.contains("Observation code:")),
        "Missing body observation"
    );
    ensure!(
        !a_before.to_string().contains("PRIVATE_"),
        "Private buffers leaked"
    );
    b.denied(
        "firefox_click",
        action(&a_before, "Count 0")?,
        "Stale snapshot",
    )?;
    a.ok("firefox_click", action(&a_before, "Count 0")?)?;
    b.ok("firefox_click", action(&b_before, "Count 0")?)?;
    a.denied(
        "firefox_click",
        action(&a_before, "Count 0")?,
        "Stale snapshot",
    )?;
    let a_after = a.ok("firefox_snapshot", json!({}))?;
    ensure!(
        a_after["statuses"]
            .to_string()
            .contains("Count updated to 1"),
        "Click not observed"
    );
    let mut typing = action(&a_after, "Message")?;
    typing["text"] = json!("BiDi chat A");
    a.ok("firefox_type", typing)?;
    let b_after = b.ok("firefox_snapshot", json!({}))?;
    let mut typing = action(&b_after, "Message")?;
    typing["text"] = json!("BiDi chat B");
    b.ok("firefox_type", typing)?;
    let child_snapshot = b.ok("firefox_snapshot", json!({}))?;
    b.ok(
        "firefox_click",
        action(&child_snapshot, "Open child work tab")?,
    )?;
    let deadline = Instant::now() + Duration::from_secs(3);
    let child = loop {
        let tabs = b.ok("firefox_tabs", json!({}))?;
        if let Some(tab) = tabs["tabs"]
            .as_array()
            .context("Missing tabs")?
            .iter()
            .find(|tab| tab["url"] == format!("{}/child", fixture.url))
        {
            break tab["tabId"].clone();
        }
        ensure!(
            Instant::now() < deadline,
            "Child tab did not inherit the owning chat"
        );
        thread::sleep(Duration::from_millis(50));
    };
    a.denied("firefox_select_tab", json!({"tabId":child}), "not owned")?;
    b.ok("firefox_select_tab", json!({"tabId":child}))?;
    b.ok("firefox_close_tab", json!({}))?;
    b.ok("firefox_select_tab", json!({"tabId":tab_b}))?;
    a.ok("firefox_revoke", json!({}))?;
    a.denied("firefox_snapshot", json!({}), "No AI work tab")?;
    a.denied("firefox_select_tab", json!({"tabId":tab_a}), "not owned")?;
    ensure!(
        b.ok("firefox_status", json!({}))?["tabEnabled"] == true,
        "A revoke affected B"
    );
    a.ok(
        "firefox_open_tab",
        json!({"url":format!("{}/replacement",fixture.url)}),
    )?;
    a.ok(
        "firefox_navigate",
        json!({"url":format!("{}/next",fixture.url)}),
    )?;
    ensure!(
        a.ok("firefox_snapshot", json!({}))?["url"] == format!("{}/next", fixture.url),
        "Navigation required manual approval"
    );
    a.ok("firefox_close_tab", json!({}))?;
    ensure!(
        a.ok("firefox_tabs", json!({}))?["tabs"] == json!([]),
        "Closed tab survived"
    );
    let disconnect_tab = a.ok(
        "firefox_open_tab",
        json!({"url":format!("{}/disconnect",fixture.url)}),
    )?["tabId"]
        .clone();
    // Simulate desktop cleanup of the entire first chat's process group.
    // The independently grouped broker must survive for chat B.
    unsafe {
        libc::kill(-(a.child.id() as i32), libc::SIGTERM);
    }
    drop(a);
    let mut c = server(&root, &firefox.address)?;
    c.denied(
        "firefox_select_tab",
        json!({"tabId":disconnect_tab}),
        "not owned",
    )?;
    ensure!(
        c.ok("firefox_tabs", json!({}))?["tabs"] == json!([]),
        "Reconnect inherited permission"
    );
    ensure!(
        b.ok("firefox_snapshot", json!({}))?["statuses"]
            .to_string()
            .contains("Count updated to 1"),
        "Disconnect affected B"
    );
    drop(b);
    drop(c);
    // Broker releases the sole Firefox session when all MCP clients have gone.
    thread::sleep(Duration::from_secs(11));
    let mut raw = Raw::connect(&firefox.address)?;
    // Firefox context IDs are scoped to a BiDi session, even for surviving physical tabs.
    let tree = raw.call("browsingContext.getTree", json!({"maxDepth":0}))?;
    let by_path = |path: &str| -> Result<Value> {
        tree["contexts"]
            .as_array()
            .context("Missing tree")?
            .iter()
            .find(|tab| tab["url"] == format!("{}{path}", fixture.url))
            .map(|tab| tab["context"].clone())
            .context("Test page disappeared")
    };
    let tab_a = by_path("/a")?;
    let tab_b = by_path("/b")?;
    let baseline = by_path("/user")?;
    let final_state: Value = serde_json::from_str(raw.script(&tab_a,"JSON.stringify({count:document.querySelector('#count').textContent,message:document.querySelector('#message').value,login:localStorage.getItem('bidi_test_login')})")?.as_str().context("Missing final A state")?)?;
    ensure!(
        final_state == json!({"count":"Count 1","message":"BiDi chat A","login":"preserved"}),
        "A control or profile session failed: {final_state}"
    );
    let final_b: Value = serde_json::from_str(raw.script(&tab_b,"JSON.stringify({count:document.querySelector('#count').textContent,message:document.querySelector('#message').value})")?.as_str().context("Missing final B state")?)?;
    ensure!(
        final_b == json!({"count":"Count 1","message":"BiDi chat B"}),
        "B changed unexpectedly"
    );
    ensure!(
        raw.script(&baseline, "document.querySelector('#count').textContent")? == "Count 0",
        "Existing user tab was modified"
    );
    ensure!(
        !tree["contexts"]
            .as_array()
            .context("Missing tree")?
            .iter()
            .any(|tab| tab["url"] == format!("{}/next", fixture.url)),
        "Close did not close the owned tab"
    );
    let png = raw.call(
        "browsingContext.captureScreenshot",
        json!({"context":tab_b,"origin":"viewport"}),
    )?;
    fs::write(
        root.join("bidi-control.png.b64"),
        png["data"].as_str().context("Missing screenshot")?,
    )?;
    fs::write(
        root.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"ok":true,"mode":"direct-bidi","extensionInstalled":false,"systemAccess":false,"a":final_state,"b":final_b,"existingTabUntouched":true,"profileStoragePreserved":true,"chatIsolation":true,"revocation":true,"reconnectDoesNotAdopt":true,"navigationWithoutApproval":true}),
        )?,
    )?;
    raw.end()?;
    eprintln!("PASS direct Firefox BiDi: automatic tabs, reading/actions, profile session, two-chat isolation, revoke/reconnect, existing tab untouched");
    Ok(())
}

#[test]
#[ignore = "starts disposable Firefox and verifies broker SIGTERM releases its BiDi session"]
fn broker_termination_releases_firefox_session() -> Result<()> {
    let root = tempfile::tempdir()?;
    let firefox = Firefox::start(root.path())?;
    let mut broker = ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_firefox-codex-mcp"))
            .arg("--bidi-broker")
            .env_remove("FIREFOX_MCP_BIDI_URL")
            .env("FIREFOX_MCP_PROFILE", root.path().join("profile"))
            .env("FIREFOX_MCP_RUNTIME_DIR", root.path().join("runtime"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()?,
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let ready = fs::read_dir(root.path().join("runtime")).is_ok_and(|items| {
            items
                .flatten()
                .any(|item| item.path().extension().is_some_and(|e| e == "sock"))
        });
        if ready {
            break;
        }
        ensure!(Instant::now() < deadline, "Broker did not start");
        thread::sleep(Duration::from_millis(30));
    }
    let mut chat = server(root.path(), &firefox.address)?;
    ensure!(chat.ok("firefox_status", json!({}))?["connected"] == true);
    unsafe {
        libc::kill(broker.0.id() as i32, libc::SIGTERM);
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = broker.0.try_wait()? {
            ensure!(status.success(), "Broker did not exit cleanly");
            break;
        }
        ensure!(Instant::now() < deadline, "Broker shutdown timed out");
        thread::sleep(Duration::from_millis(30));
    }
    // Same live Firefox must accept a fresh session without a browser restart.
    let raw = Raw::connect(&firefox.address)?;
    raw.end()?;
    Ok(())
}
