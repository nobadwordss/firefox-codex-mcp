//! One per-user BiDi broker, with ownership bound to each live MCP connection.
//! Firefox accepts one BiDi session, so stdio chats must not each create one.
use crate::bidi::{self, Browser};
#[cfg(test)]
use crate::mcp::tools;
use crate::mcp::{read_line, validate};
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    env,
    fs::{self, File, OpenOptions},
    os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    process::Command,
    sync::Mutex,
    time::{sleep, timeout},
};

fn uid() -> u32 {
    unsafe { libc::geteuid() }
} // No pointers or preconditions.
fn address() -> Result<String> {
    let value = if let Ok(value) = env::var("FIREFOX_MCP_BIDI_URL") {
        value
    } else if let Some(profile) = env::var_os("FIREFOX_MCP_PROFILE") {
        let path = PathBuf::from(profile).join("WebDriverBiDiServer.json");
        let file = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(&path)
            .context("Firefox has no debugging connection. Start the same profile with --remote-debugging-port 0")?;
        ensure!(
            file.metadata()?.len() <= 4096 && file.metadata()?.uid() == uid(),
            "Invalid Firefox debug descriptor"
        );
        let value: Value = serde_json::from_reader(file)?;
        ensure!(
            value["ws_host"] == "127.0.0.1",
            "Firefox debug listener must be local"
        );
        format!(
            "ws://127.0.0.1:{}/session",
            value["ws_port"]
                .as_u64()
                .context("Missing Firefox debug port")?
        )
    } else {
        "ws://127.0.0.1:9222/session".into()
    };
    bidi::endpoint(&value)?;
    Ok(value)
}
pub(crate) fn runtime_directory() -> Result<PathBuf> {
    let path = if let Some(path) = env::var_os("FIREFOX_MCP_RUNTIME_DIR") {
        PathBuf::from(path)
    } else if let Some(path) = env::var_os("XDG_RUNTIME_DIR") {
        PathBuf::from(path).join("firefox-codex-mcp")
    } else {
        PathBuf::from(env::var_os("HOME").context("HOME is unavailable")?)
            .join(".local/state/firefox-codex-mcp/runtime")
    };
    private_runtime_directory(path)
}
pub(crate) fn private_runtime_directory(path: PathBuf) -> Result<PathBuf> {
    ensure!(path.is_absolute(), "Runtime directory must be absolute");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&path)?;
    let meta = fs::symlink_metadata(&path)?;
    ensure!(
        meta.is_dir() && meta.uid() == uid() && meta.mode() & 0o077 == 0,
        "Broker runtime directory must be private and owned by this user"
    );
    Ok(path)
}
fn socket_path() -> Result<PathBuf> {
    use sha2::{Digest, Sha256};
    let key = if env::var_os("FIREFOX_MCP_BIDI_URL").is_none() {
        if let Some(profile) = env::var_os("FIREFOX_MCP_PROFILE") {
            let path = PathBuf::from(profile)
                .canonicalize()
                .context("Firefox profile directory does not exist")?;
            ensure!(path.is_dir(), "Firefox profile must be a directory");
            format!("profile:{}", path.to_string_lossy())
        } else {
            address()?
        }
    } else {
        address()?
    };
    let hash = Sha256::digest(key.as_bytes());
    let suffix: String = hash[..8].iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(runtime_directory()?.join(format!("bidi-{suffix}.sock")))
}
pub(crate) fn broker_lock(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file() && meta.uid() == uid() && meta.nlink() == 1 && meta.mode() & 0o077 == 0,
        "Invalid broker lock file"
    );
    file.try_lock()
        .context("Another broker already owns this endpoint")?;
    // The lock inode is retained after exit so simultaneous starters cannot split it.
    Ok(file)
}
pub(crate) async fn connect_socket(path: &Path) -> Result<UnixStream> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.file_type().is_socket() && meta.uid() == uid() && meta.mode() & 0o077 == 0,
        "Invalid broker socket"
    );
    let socket = UnixStream::connect(path).await?;
    ensure!(
        socket.peer_cred()?.uid() == uid(),
        "Unexpected broker owner"
    );
    Ok(socket)
}
pub(crate) async fn client() -> Result<BufReader<UnixStream>> {
    let path = socket_path()?;
    if let Ok(socket) = connect_socket(&path).await {
        return handshake(socket).await;
    }
    let mut child = Command::new(env::current_exe()?)
        .arg("--bidi-broker")
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()?;
    // Losing starters exit after try_lock; reap our child without tying it to stdio lifetime.
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    for _ in 0..60 {
        sleep(Duration::from_millis(50)).await;
        if let Ok(socket) = connect_socket(&path).await {
            return handshake(socket).await;
        }
    }
    bail!("Could not start the local Firefox connection broker")
}

const BROKER_PROTOCOL: u64 = 1;
async fn handshake(socket: UnixStream) -> Result<BufReader<UnixStream>> {
    let mut socket = BufReader::new(socket);
    socket
        .get_mut()
        .write_all(format!("{}\n", json!({"hello":BROKER_PROTOCOL})).as_bytes())
        .await?;
    socket.get_mut().flush().await?;
    let reply = timeout(Duration::from_secs(3), read_line(&mut socket)).await??;
    ensure!(reply.is_some_and(|r| r["ok"] == true && r["value"]["protocol"] == BROKER_PROTOCOL),
        "A connection from before the Firefox MCP update is still active. Close existing Firefox tool chats, wait 10 seconds, then try a new chat. Firefox was not forcibly closed.");
    Ok(socket)
}

#[derive(Default)]
struct Owner {
    tabs: HashSet<String>,
    selected: Option<String>,
}
impl Owner {
    fn selected(&self) -> Result<String> {
        let id = self.selected.as_ref().context("No AI work tab. Call firefox_open_tab with a page URL; no extension approval is needed")?;
        ensure!(self.tabs.contains(id), "This tab is not owned by this chat");
        Ok(id.clone())
    }
    fn select(&mut self, id: &str) -> Result<()> {
        ensure!(self.tabs.contains(id),"This tab is not owned by this chat; existing user tabs and other chats cannot be selected");
        self.selected = Some(id.into());
        Ok(())
    }
    fn remove(&mut self, id: &str) {
        self.tabs.remove(id);
        if self.selected.as_deref() == Some(id) {
            self.selected = None;
        }
    }
}
struct State {
    browser: Option<Browser>,
    owners: HashMap<String, Owner>,
}
impl State {
    fn clear_tabs(&mut self) {
        for owner in self.owners.values_mut() {
            *owner = Owner::default();
        }
    }
    fn reconcile(&mut self) {
        if self.browser.as_ref().is_some_and(|browser| !browser.alive) {
            self.browser = None;
            self.clear_tabs();
        } else if let Some(browser) = self.browser.as_mut() {
            for (id, opener) in browser.take_created() {
                if let Some(owner) = self
                    .owners
                    .values_mut()
                    .find(|owner| owner.tabs.contains(&opener))
                {
                    if owner.tabs.len() < 8 {
                        owner.tabs.insert(id);
                    }
                }
            }
            for id in browser.take_destroyed() {
                for owner in self.owners.values_mut() {
                    owner.remove(&id);
                }
            }
        }
    }
    async fn connect(&mut self) -> Result<()> {
        self.reconcile();
        if self.browser.is_none() {
            self.browser = Some(Browser::connect(&address()?).await?);
        }
        Ok(())
    }
    async fn execute(&mut self, owner_id: &str, name: &str, args: Value) -> Result<Value> {
        validate(name, &args)?;
        ensure!(self.owners.contains_key(owner_id), "MCP connection ended");
        if name == "firefox_revoke" {
            let owner = self.owners.get_mut(owner_id).context("Missing owner")?;
            let count = owner.tabs.len();
            *owner = Owner::default();
            return Ok(
                json!({"revoked":true,"releasedTabs":count,"nextStep":"Released tabs remain open for you. This chat cannot reclaim them; firefox_open_tab creates a new work tab."}),
            );
        }
        if name == "firefox_open_tab"
            && self.browser.is_none()
            && env::var_os("FIREFOX_MCP_BIDI_URL").is_none()
        {
            if let Some(path) = env::var_os("FIREFOX_MCP_PROFILE") {
                let profile = crate::setup::profile(&path.to_string_lossy())?;
                if !crate::setup::running(&profile) {
                    tokio::task::spawn_blocking(move || crate::setup::launch(&profile)).await??;
                }
            }
        }
        if let Err(reason) = self.connect().await {
            if name == "firefox_status" {
                return Ok(
                    json!({"connected":false,"mode":"bidi","ownedTabs":0,"tabEnabled":false,
                    "nextStep":"If Firefox is running, save your work, close it once and use --launch-firefox PROFILE for advanced debugging. If Firefox is closed and a profile is configured, firefox_open_tab starts it automatically. This advanced mode needs no extension, token or per-tab grant.","diagnostic":reason.to_string()}),
                );
            }
            return Err(reason);
        }
        if matches!(name, "firefox_status" | "firefox_tabs") {
            self.browser
                .as_mut()
                .context("Firefox connection ended")?
                .call("session.status", json!({}))
                .await?;
            self.reconcile();
        }
        let before = self
            .owners
            .get(owner_id)
            .context("Missing owner")?
            .tabs
            .clone();
        let mut result = self.connected_call(owner_id, name, args).await;
        self.reconcile();
        if let Ok(value) = result.as_mut() {
            let new_tabs: Vec<_> = self
                .owners
                .get(owner_id)
                .context("Missing owner")?
                .tabs
                .difference(&before)
                .cloned()
                .collect();
            if !new_tabs.is_empty() {
                value["newTabs"] = json!(new_tabs);
            }
        }
        result
    }
    async fn connected_call(&mut self, owner_id: &str, name: &str, args: Value) -> Result<Value> {
        let owner = self
            .owners
            .get_mut(owner_id)
            .context("MCP connection ended")?;
        let browser = self.browser.as_mut().context("Firefox connection ended")?;
        match name {
            "firefox_status" => Ok(
                json!({"connected":true,"mode":"bidi","clientId":owner_id,"ownedTabs":owner.tabs.len(),
                    "tabId":owner.selected,"tabEnabled":owner.selected.is_some(),
                    "nextStep":if owner.tabs.is_empty() {"Call firefox_open_tab with a URL. The AI creates and owns that tab automatically."} else {"Use this chat's work tabs. Navigation preserves ownership; take a fresh snapshot afterwards."}}),
            ),
            "firefox_open_tab" => {
                ensure!(
                    owner.tabs.len() < 8,
                    "Close or release an unused work tab first (maximum 8 per chat)"
                );
                let created = browser
                    .call(
                        "browsingContext.create",
                        json!({"type":"tab","background":true}),
                    )
                    .await?;
                let id = created["context"]
                    .as_str()
                    .context("Firefox did not return a new tab")?
                    .to_owned();
                // Record ownership before navigation, including navigation failures.
                owner.tabs.insert(id.clone());
                owner.selected = Some(id.clone());
                let navigation = browser
                    .call(
                        "browsingContext.navigate",
                        json!({"context":id,"url":args["url"],"wait":"interactive"}),
                    )
                    .await;
                match navigation {
                    Ok(_) => Ok(json!({"tabId":id,"created":true,"url":args["url"]})),
                    Err(error) => bail!("Work tab {id} was created but navigation failed: {error}. Inspect before retrying"),
                }
            }
            "firefox_tabs" => {
                let mut tabs = Vec::new();
                for id in owner.tabs.clone() {
                    match browser.tree(&id).await {
                        Ok(mut tab) => {
                            tab["selected"] = json!(owner.selected.as_deref() == Some(&id));
                            tabs.push(tab);
                        }
                        Err(error) => {
                            if browser.was_destroyed(&id) {
                                owner.remove(&id);
                            } else {
                                return Err(error);
                            }
                        }
                    }
                }
                Ok(json!({"tabs":tabs}))
            }
            "firefox_select_tab" => {
                let id = args["tabId"].as_str().context("Missing tab ID")?;
                owner.select(id)?;
                browser.tree(id).await
            }
            "firefox_close_tab" => {
                let id = owner.selected()?;
                // Fence locally first. A timeout must not make this tab reusable.
                owner.remove(&id);
                browser
                    .call(
                        "browsingContext.close",
                        json!({"context":id,"promptUnload":true}),
                    )
                    .await?;
                Ok(json!({"closed":true,"tabId":id}))
            }
            "firefox_navigate" => {
                let id = owner.selected()?;
                browser
                    .call(
                        "browsingContext.navigate",
                        json!({"context":id,"url":args["url"],"wait":"interactive"}),
                    )
                    .await?;
                Ok(json!({"navigated":true,"tabId":id,"url":args["url"]}))
            }
            _ => {
                let id = owner.selected()?;
                browser
                    .observe_or_act(&id, owner_id, name.trim_start_matches("firefox_"), args)
                    .await
            }
        }
    }
}

pub async fn broker() -> Result<()> {
    let path = socket_path()?;
    let _lock = broker_lock(&path.with_extension("lock"))?;
    if let Ok(meta) = fs::symlink_metadata(&path) {
        ensure!(
            meta.file_type().is_socket() && meta.uid() == uid(),
            "Refusing to replace another file at the broker socket path"
        );
        fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    let state = Arc::new(Mutex::new(State {
        browser: None,
        owners: HashMap::new(),
    }));
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    loop {
        let incoming = tokio::select! {
            result = timeout(Duration::from_secs(10), listener.accept()) => Some(result),
            _ = terminate.recv() => None,
            _ = tokio::signal::ctrl_c() => None,
        };
        let Some(incoming) = incoming else {
            if let Some(browser) = state.lock().await.browser.as_mut() {
                browser.end().await;
            }
            break;
        };
        match incoming {
            Ok(Ok((socket, _))) => {
                if socket.peer_cred()?.uid() != uid() {
                    continue;
                }
                let id = crate::random_hex(16)?;
                state
                    .lock()
                    .await
                    .owners
                    .insert(id.clone(), Owner::default());
                let shared = state.clone();
                tokio::spawn(async move {
                    let mut socket = BufReader::new(socket);
                    let work = async {
                        let mut compatible = false;
                        while let Some(request) = read_line(&mut socket).await? {
                            let response = if request == json!({"hello":BROKER_PROTOCOL}) {
                                compatible = true;
                                Ok(json!({"protocol":BROKER_PROTOCOL}))
                            } else if compatible
                                && request.as_object().is_some_and(|object| object.len() == 2)
                                && request["name"].is_string()
                                && request["args"].is_object()
                            {
                                shared
                                    .lock()
                                    .await
                                    .execute(
                                        &id,
                                        request["name"].as_str().unwrap_or(""),
                                        request["args"].clone(),
                                    )
                                    .await
                            } else {
                                Err(anyhow::anyhow!("Invalid broker request"))
                            };
                            let response = match response {
                                Ok(value) => json!({"ok":true,"value":value}),
                                Err(error) => json!({"ok":false,"error":error.to_string()}),
                            };
                            socket
                                .get_mut()
                                .write_all(format!("{response}\n").as_bytes())
                                .await?;
                            socket.get_mut().flush().await?;
                        }
                        Ok::<(), anyhow::Error>(())
                    }
                    .await;
                    if work.is_err() {
                        eprintln!("Firefox MCP client disconnected after a transport error");
                    }
                    // Keep visible tabs, discard their capabilities. Never adopt them on reconnect.
                    shared.lock().await.owners.remove(&id);
                });
            }
            Ok(Err(error)) => return Err(error.into()),
            Err(_) => {
                let mut current = state.lock().await;
                if current.owners.is_empty() {
                    if let Some(browser) = current.browser.as_mut() {
                        browser.end().await;
                    }
                    break;
                }
            }
        }
    }
    fs::remove_file(&path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn old_broker_is_rejected_before_dispatching_browser_actions() {
        let (client, server) = UnixStream::pair().unwrap();
        let old = tokio::spawn(async move {
            let mut server = BufReader::new(server);
            let first = read_line(&mut server).await.unwrap().unwrap();
            assert!(first.get("name").is_none());
            server
                .get_mut()
                .write_all(b"{\"ok\":false,\"error\":\"Invalid broker request\"}\n")
                .await
                .unwrap();
        });
        let error = handshake(client).await.err().unwrap();
        assert!(error.to_string().contains("before the Firefox MCP update"));
        old.await.unwrap();
    }
    #[test]
    fn ownership_cannot_be_selected_or_reclaimed_by_another_chat() {
        let mut a = Owner::default();
        let mut b = Owner::default();
        a.tabs.insert("a".into());
        a.select("a").unwrap();
        b.tabs.insert("b".into());
        b.select("b").unwrap();
        assert!(b.select("a").is_err());
        assert!(a.select("existing-user-tab").is_err());
        a.remove("a");
        assert!(a.select("a").is_err());
        assert_eq!(b.selected().unwrap(), "b");
    }
    #[test]
    fn debug_tools_do_not_expose_adoption_or_arbitrary_protocol() {
        for (name, args) in [
            ("firefox_select_tab", json!({"tabId":"x","owner":"other"})),
            (
                "firefox_open_tab",
                json!({"url":"https://example.org","tabId":"existing"}),
            ),
            ("script.evaluate", json!({})),
            ("firefox_revoke", json!({"clientId":"other"})),
        ] {
            assert!(validate(name, &args).is_err());
        }
        assert_eq!(tools()["tools"].as_array().unwrap().len(), 11);
    }
}
