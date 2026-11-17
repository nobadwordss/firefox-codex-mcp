//! Local extension router. Native Messaging and MCP have separate stdio owners.
//! All local peers have the same OS uid; this is not isolation from that user.
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    env,
    path::PathBuf,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::{mpsc, oneshot, Mutex, Notify},
    time::{sleep, timeout},
};
const VERSION: u64 = 1;
pub const EXTENSION_ID: &str = "{a171de16-96b6-4e50-94b7-e0b54d94436a}";
pub const HOST: &str = "firefox_codex_mcp";
fn valid_id(value: &str) -> bool {
    (8..=80).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
fn socket_path() -> Result<PathBuf> {
    crate::transport::endpoint()
}
fn settings_path() -> Result<PathBuf> {
    let dir = crate::transport::state_dir()?;
    crate::setup::private_dir(&dir)?;
    Ok(dir.join("profiles.json"))
}
#[derive(Default)]
struct Router {
    chats: crate::continuity::Chats,
    peers: HashMap<String, Arc<Peer>>,
    selected: Option<String>,
    blocked: HashSet<String>,
    revision: u64,
    last_launch: Option<tokio::time::Instant>,
    shutdown: Arc<Notify>,
}
impl Router {
    fn detach(&mut self, a: &crate::continuity::Attachment) {
        if self.chats.detach(a) {
            for peer in self.peers.values() {
                peer.send(json!({"kind":if a.persistent {"suspendOwner"} else {"dropOwner"},"owner":a.owner,"attachment":a.generation}));
            }
        }
    }
    fn expire(&mut self) {
        for a in self.chats.expire() {
            for peer in self.peers.values() {
                peer.send(json!({"kind":"dropOwner","owner":a.owner}));
            }
        }
    }
    fn load() -> Result<Self> {
        let mut state = Self::default();
        let read = || crate::transport::read_private(&settings_path()?, 65536);
        if let Some(bytes) = read()? {
            let value: Value = serde_json::from_slice(&bytes)?;
            state.selected = value["selected"].as_str().map(str::to_owned);
            state.blocked = serde_json::from_value(value["blocked"].clone())?;
        }
        Ok(state)
    }
    fn save(&self) -> Result<()> {
        crate::setup::write_atomic(
            &settings_path()?,
            &serde_json::to_vec(&json!({"selected":self.selected,"blocked":self.blocked}))?,
            0o600,
        )
    }
    fn broadcast(&self) {
        for peer in self.peers.values() {
            peer.send(
                json!({"kind":"configuration","selected":self.selected,"count":self.peers.len()}),
            );
        }
    }
    fn select(&mut self, id: &str) -> Result<()> {
        ensure!(self.peers.contains_key(id), "Profile is disconnected");
        let previous = self.selected.replace(id.into());
        if let Err(error) = self.save() {
            self.selected = previous;
            return Err(error);
        }
        self.revision += 1;
        for peer in self.peers.values() {
            peer.send(json!({"kind":"revokeAll"}));
        }
        self.broadcast();
        Ok(())
    }
    fn choose(&self) -> Result<Arc<Peer>> {
        if let Some(id) = &self.selected {
            return self.peers.get(id).filter(|p|p.alive.load(Ordering::SeqCst)).cloned().context("The selected Firefox is offline. Open that Firefox profile. Another profile will not be selected automatically.");
        }
        ensure!(self.peers.len()<=1,"Several Firefox profiles are connected. Choose Use this Firefox in the extension of the profile you want to use.");
        self.peers.values().next().filter(|p|p.alive.load(Ordering::SeqCst)).cloned().context("The Firefox extension is disconnected. Install the extension and open Firefox. No code, token or profile path is needed.")
    }
}
struct Peer {
    id: String,
    epoch: String,
    tx: mpsc::Sender<Value>,
    pending: Mutex<HashMap<String, oneshot::Sender<Value>>>,
    alive: AtomicBool,
    closed: Notify,
    continuity: bool,
}
impl Peer {
    fn send(&self, value: Value) {
        if self.tx.try_send(value).is_err() {
            self.stop();
        }
    }
    fn stop(&self) {
        self.alive.store(false, Ordering::SeqCst);
        self.closed.notify_one();
    }
    async fn call(&self, owner: &str, attachment: &str, name: &str, args: Value) -> Result<Value> {
        ensure!(
            self.alive.load(Ordering::SeqCst),
            "Firefox connection ended"
        );
        let id = crate::random_hex(16)?;
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);
        self.send(json!({"kind":"request","id":id,"owner":owner,"attachment":attachment,"name":name,"args":args}));
        let result = timeout(Duration::from_secs(20), rx).await;
        self.pending.lock().await.remove(&id);
        match result {
            Ok(Ok(value)) if self.alive.load(Ordering::SeqCst) => {
                if let Some(error) = value["error"].as_str() {
                    bail!("{error}");
                }
                Ok(value["result"].clone())
            }
            _ => {
                self.stop();
                bail!("Firefox connection ended or timed out; action outcome may be unknown. Inspect before retrying. Previous tab access was released.")
            }
        }
    }
}
pub async fn chat_client(identity: Option<&str>) -> Result<BufReader<crate::transport::Stream>> {
    let mut socket = connect().await?;
    socket
        .get_mut()
        .write_all(
            format!(
                "{}\n",
                json!({"role":"chat","version":VERSION,"identity":identity})
            )
            .as_bytes(),
        )
        .await?;
    let hello = timeout(Duration::from_secs(5), crate::mcp::read_line(&mut socket)).await??;
    ensure!(
        hello.is_some_and(|v| v["ready"] == VERSION),
        "Extension broker version mismatch. Close old Firefox MCP chats and reopen Firefox."
    );
    Ok(socket)
}
async fn connect() -> Result<BufReader<crate::transport::Stream>> {
    let path = socket_path()?;
    if let Ok(stream) = crate::transport::connect(&path).await {
        return Ok(BufReader::new(stream));
    }
    let mut command = Command::new(env::current_exe()?);
    crate::transport::detach(&mut command);
    let mut child = command
        .arg("--extension-broker")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    for _ in 0..80 {
        sleep(Duration::from_millis(50)).await;
        if let Ok(s) = crate::transport::connect(&path).await {
            return Ok(BufReader::new(s));
        }
    }
    bail!("Could not start Firefox extension connection service")
}
async fn read_frame(input: &mut (impl AsyncRead + Unpin)) -> Result<Option<Value>> {
    let mut length = [0u8; 4];
    if input.read(&mut length[..1]).await? == 0 {
        return Ok(None);
    }
    input.read_exact(&mut length[1..]).await?;
    let size = u32::from_ne_bytes(length) as usize;
    ensure!(
        size > 0 && size < crate::LIMIT,
        "Invalid native message length"
    );
    let mut bytes = vec![0; size];
    input.read_exact(&mut bytes).await?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}
fn valid_settings_id(value: &str) -> bool {
    (value.len() == 32 && value.bytes().all(|b| b.is_ascii_hexdigit()))
        || (value.len() == 36
            && value.bytes().enumerate().all(|(i, b)| {
                if [8, 13, 18, 23].contains(&i) {
                    b == b'-'
                } else {
                    b.is_ascii_hexdigit()
                }
            }))
}
pub(crate) fn valid_settings_url(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("http://127.0.0.1:") else {
        return false;
    };
    let Some((port, token)) = rest.split_once("/#") else {
        return false;
    };
    let valid_port = port
        .parse::<u16>()
        .is_ok_and(|number| number > 0 && number.to_string() == port);
    valid_port
        && token.len() == 64
        && token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
async fn read_settings_url(input: &mut (impl AsyncRead + Unpin)) -> Result<String> {
    // The helper prints exactly one bounded startup line. Never pass arbitrary
    // subprocess output to Firefox as a URL or into the native stdout stream.
    let mut line = Vec::new();
    loop {
        ensure!(line.len() < 256, "Invalid Settings startup response");
        let byte = input.read_u8().await?;
        if byte == b'\n' {
            break;
        }
        line.push(byte);
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    let line = std::str::from_utf8(&line)?;
    let url = line
        .strip_prefix("Firefox Codex setup: ")
        .filter(|value| valid_settings_url(value))
        .context("Invalid Settings startup response")?;
    Ok(url.into())
}
struct RunningSettings {
    url: String,
    alive: Arc<AtomicBool>,
}
struct StartingSettings(Option<Child>);
impl Drop for StartingSettings {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            // Cancelling the startup future must stop an unseen manager before
            // this dedicated native host exits. After validation, ownership is
            // explicitly transferred to the detached manager's reaper below.
            let _ = child.start_kill();
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    let _ = child.wait().await;
                });
            }
        }
    }
}
#[derive(Default)]
struct Settings {
    running: Option<RunningSettings>,
}
impl Settings {
    async fn open(&mut self) -> Result<String> {
        if let Some(running) = &self.running {
            if running.alive.load(Ordering::SeqCst) {
                return Ok(running.url.clone());
            }
        }
        self.running = None;
        // The extension opens this URL using browser.tabs.create, preserving
        // the profile that requested Settings. Never launch Firefox here.
        let mut command = Command::new(env::current_exe()?);
        crate::transport::detach(&mut command);
        let mut starting = StartingSettings(Some(
            command
                .arg("--manage-no-open")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .context("Could not start connection settings")?,
        ));
        let startup = async {
            let child = starting.0.as_mut().context("Missing Settings process")?;
            let mut output = child
                .stdout
                .take()
                .context("Missing Settings startup pipe")?;
            let url = timeout(Duration::from_secs(10), read_settings_url(&mut output))
                .await
                .context("Connection settings did not start in time")??;
            ensure!(
                child.try_wait()?.is_none(),
                "Connection settings closed during startup"
            );
            Ok::<_, anyhow::Error>((url, output))
        }
        .await;
        let (url, mut output) = match startup {
            Ok(value) => value,
            Err(error) => {
                if let Some(child) = &mut starting.0 {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                }
                return Err(error);
            }
        };
        let mut child = starting.0.take().context("Missing Settings process")?;
        let alive = Arc::new(AtomicBool::new(true));
        let watching = alive.clone();
        tokio::spawn(async move {
            let wait = async {
                let _ = child.wait().await;
                watching.store(false, Ordering::SeqCst);
            };
            let drain = async {
                let _ = tokio::io::copy(&mut output, &mut tokio::io::sink()).await;
            };
            tokio::join!(wait, drain);
        });
        // Do not kill Settings when Native Messaging disconnects: installing
        // or removing the connection resets that port during the HTTP action.
        // The manager already has its own fixed 20-minute lifetime.
        self.running = Some(RunningSettings {
            url: url.clone(),
            alive,
        });
        Ok(url)
    }
}
fn settings_capability(mut value: Value) -> Value {
    if value["kind"] == "ready" {
        let mut capabilities = value["capabilities"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if !capabilities.iter().any(|item| item == "settings-ui-v1") {
            capabilities.push(json!("settings-ui-v1"));
        }
        value["capabilities"] = json!(capabilities);
    }
    value
}
async fn write_frames(
    output: &mut (impl AsyncWrite + Unpin),
    messages: &mut mpsc::Receiver<Value>,
) -> Result<()> {
    while let Some(value) = messages.recv().await {
        let bytes = serde_json::to_vec(&value)?;
        ensure!(bytes.len() < crate::LIMIT, "Native reply too large");
        output
            .write_all(&(bytes.len() as u32).to_ne_bytes())
            .await?;
        output.write_all(&bytes).await?;
        output.flush().await?;
    }
    Ok(())
}
pub async fn host() -> Result<()> {
    // Firefox passes host manifest path and the permitted extension ID.
    ensure!(
        env::args().next_back().as_deref() == Some(EXTENSION_ID),
        "Unexpected native messaging caller"
    );
    let mut input = tokio::io::stdin();
    let first = timeout(Duration::from_secs(10), read_frame(&mut input))
        .await??
        .context("Missing extension hello")?;
    ensure!(
        first["role"] == "extension" && first["version"] == VERSION,
        "Invalid extension hello"
    );
    let mut socket = connect().await?;
    socket
        .get_mut()
        .write_all(format!("{first}\n").as_bytes())
        .await?;
    let (read, mut write) = tokio::io::split(socket.into_inner());
    let (outgoing, mut messages) = mpsc::channel::<Value>(128);
    let (settings_requests, mut requests) = mpsc::channel::<String>(4);
    let up_outgoing = outgoing.clone();
    let up = async {
        while let Some(value) = read_frame(&mut input).await? {
            if value["kind"] == "openSettings" {
                let id = value["id"]
                    .as_str()
                    .filter(|id| valid_settings_id(id))
                    .context("Invalid Settings request ID")?;
                if settings_requests.try_send(id.into()).is_err() {
                    up_outgoing
                        .send(json!({"kind":"settingsResult","id":id,
                        "error":"Settings is already opening. Try again."}))
                        .await?;
                }
            } else {
                write.write_all(format!("{value}\n").as_bytes()).await?;
            }
        }
        Ok::<(), anyhow::Error>(())
    };
    let down_outgoing = outgoing.clone();
    let down = async {
        let mut read = BufReader::new(read);
        while let Some(value) = crate::mcp::read_line(&mut read).await? {
            down_outgoing.send(settings_capability(value)).await?;
        }
        Ok::<(), anyhow::Error>(())
    };
    let settings = async {
        let mut settings = Settings::default();
        while let Some(id) = requests.recv().await {
            let response = match settings.open().await {
                Ok(url) => json!({"kind":"settingsResult","id":id,"url":url}),
                Err(error) => json!({"kind":"settingsResult","id":id,"error":error.to_string()}),
            };
            outgoing.send(response).await?;
        }
        Ok::<(), anyhow::Error>(())
    };
    let output = async { write_frames(&mut tokio::io::stdout(), &mut messages).await };
    tokio::select! {r=up=>r,r=down=>r,r=settings=>r,r=output=>r}
}
async fn launch_if_needed(state: &Arc<Mutex<Router>>) -> Result<()> {
    let mut s = state.lock().await;
    if s.peers.is_empty()
        && s.last_launch
            .is_none_or(|t| t.elapsed() > Duration::from_secs(30))
    {
        s.last_launch = Some(tokio::time::Instant::now());
        let mut command = Command::new(crate::transport::firefox()?);
        crate::transport::detach(&mut command);
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        tokio::spawn(async move {
            let _ = child.wait().await;
        });
    }
    drop(s);
    for _ in 0..80 {
        if !state.lock().await.peers.is_empty() {
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }
    Ok(())
}
async fn chat(
    mut socket: BufReader<crate::transport::Stream>,
    state: Arc<Mutex<Router>>,
    identity: Option<&str>,
) -> Result<()> {
    if let Some(id) = identity {
        ensure!(
            id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid host identity digest"
        );
    }
    let a = {
        let mut s = state.lock().await;
        s.expire();
        let previous = s.chats.previous(identity);
        let attachment = s.chats.attach(identity)?;
        // Fence extension work at registration, even if the replacement sends
        // no tool request. Waiting for its first request leaves old IPC alive.
        if let Some(previous) = previous {
            for peer in s.peers.values() {
                peer.send(json!({"kind":"suspendOwner","owner":previous.owner,"attachment":previous.generation}));
            }
        }
        attachment
    };
    if let Err(error) = socket
        .get_mut()
        .write_all(format!("{}\n", json!({"ready":VERSION})).as_bytes())
        .await
    {
        state.lock().await.detach(&a);
        return Err(error.into());
    }
    let buffered = socket.buffer().to_vec();
    let (read, mut write) = tokio::io::split(socket.into_inner());
    let mut read = BufReader::new(std::io::Cursor::new(buffered).chain(read));
    let (tx, mut requests) = mpsc::channel(32);
    let (ended, closed) = oneshot::channel();
    let reader_state = state.clone();
    let reader_attachment = a.clone();
    let reader = tokio::spawn(async move {
        while let Ok(Some(request)) = crate::mcp::read_line(&mut read).await {
            if tx.try_send(request).is_err() {
                break;
            }
        }
        reader_state.lock().await.detach(&reader_attachment);
        let _ = ended.send(());
    });
    let mut bound: Option<(Arc<Peer>, u64)> = None;
    let work = async {
        while let Some(request) = requests.recv().await {
            let name = request["name"].as_str().unwrap_or("");
            let args = request["args"].clone();
            let result = async {
                crate::mcp::validate(name, &args)?;
                if name == "firefox_open_tab" && state.lock().await.peers.is_empty() { launch_if_needed(&state).await?; }
                let mut s = state.lock().await;
                s.chats.touch(&a)?;
                if let Some((peer,revision)) = &bound {
                    if !peer.alive.load(Ordering::SeqCst) || *revision != s.revision { bound = None; }
                }
                let peer = match &bound {
                    Some((p,_)) => p.clone(),
                    None => match s.choose() {
                        Ok(p) => p,
                        Err(error) => {
                            if name == "firefox_revoke" {
                                for p in s.peers.values() { p.send(json!({"kind":"dropOwner","owner":a.owner})); }
                                return Ok(json!({"revoked":true,"ownedTabs":0}));
                            }
                            if name == "firefox_status" { return Ok(json!({"connected":false,"mode":"extension","ownedTabs":0,"nextStep":error.to_string()})); }
                            return Err(error);
                        }
                    }
                };
                if !peer.continuity {
                    if name == "firefox_status" { return Ok(json!({"connected":false,"mode":"extension","ownedTabs":0,"nextStep":"Update the Codex Work Tabs extension to 0.4.0 or newer."})); }
                    bail!("Update the Codex Work Tabs extension to 0.4.0 or newer.");
                }
                if name == "firefox_open_tab" && s.selected.is_none() {
                    s.selected = Some(peer.id.clone());
                    if let Err(e) = s.save() { s.selected=None; return Err(e); }
                    s.broadcast();
                }
                let revision = s.revision;
                if bound.is_none() {
                    peer.send(json!({"kind":"attachOwner","owner":a.owner,"attachment":a.generation}));
                    bound = Some((peer.clone(),revision));
                }
                drop(s);
                let mut value = peer.call(&a.owner,&a.generation,name,args).await?;
                let s = state.lock().await;
                ensure!(s.chats.current(&a) && s.revision == revision && peer.alive.load(Ordering::SeqCst), "Connection or profile changed during operation; action outcome may be unknown. Take a fresh snapshot.");
                if name == "firefox_status" {
                    value["mode"] = json!("extension"); value["profileId"] = json!(peer.id);
                    value["continuity"] = json!(if a.persistent {"chat"} else {"connection"});
                    value["idleExpiryHours"] = json!(24);
                    value["helperVersion"] = json!(env!("CARGO_PKG_VERSION"));
                }
                Ok::<Value,anyhow::Error>(value)
            }.await;
            let reply = match result {
                Ok(value) => json!({"ok":true,"value":value}),
                Err(e) => json!({"ok":false,"error":e.to_string()}),
            };
            write.write_all(format!("{reply}\n").as_bytes()).await?;
        }
        Ok::<(), anyhow::Error>(())
    };
    let result = tokio::select! {r=work=>r,_=closed=>Ok(()),_=a.cancelled.notified()=>Ok(())};
    reader.abort();
    let _ = reader.await;
    state.lock().await.detach(&a);
    result
}
async fn extension(
    socket: BufReader<crate::transport::Stream>,
    hello: Value,
    state: Arc<Mutex<Router>>,
) -> Result<()> {
    let id = hello["profileId"]
        .as_str()
        .filter(|s| valid_id(s))
        .context("Invalid profile identity")?
        .to_owned();
    let (tx, mut rx) = mpsc::channel::<Value>(128);
    let peer = Arc::new(Peer {
        id: id.clone(),
        epoch: crate::random_hex(16)?,
        tx,
        pending: Mutex::new(HashMap::new()),
        alive: AtomicBool::new(true),
        closed: Notify::new(),
        continuity: hello["capabilities"]
            .as_array()
            .is_some_and(|v| v.iter().any(|v| v == "chat-continuity-v1")),
    });
    {
        let mut s = state.lock().await;
        ensure!(
            !s.blocked.contains(&id),
            "Copied profile identity. Reset this extension's identity in its settings."
        );
        if let Some(old) = s.peers.remove(&id) {
            old.stop();
            s.blocked.insert(id.clone());
            s.save()?;
            s.broadcast();
            bail!("Copied profile identity rejected");
        }
        s.peers.insert(id.clone(), peer.clone());
        peer.send(json!({"kind":"ready","epoch":peer.epoch}));
        s.broadcast();
    }
    let buffered = socket.buffer().to_vec();
    let (read, mut write) = tokio::io::split(socket.into_inner());
    let mut read = BufReader::new(std::io::Cursor::new(buffered).chain(read));
    let work=async {
        loop {if !peer.alive.load(Ordering::SeqCst){break;} tokio::select!{
            _=peer.closed.notified()=>break,
            message=rx.recv()=>{let Some(message)=message else {break;};write.write_all(format!("{message}\n").as_bytes()).await?;},
            message=crate::mcp::read_line(&mut read)=>{let Some(message)=message? else{break;};
                match message["kind"].as_str(){
                    Some("response")=>{if let Some(id)=message["id"].as_str(){if let Some(waiter)=peer.pending.lock().await.remove(id){let _=waiter.send(message);}}},
                    Some("selectProfile")=>{state.lock().await.select(&id)?;},
                    _=>bail!("Invalid extension message")
                }
            }
        }} Ok::<(),anyhow::Error>(())
    }.await;
    peer.stop();
    peer.pending.lock().await.clear();
    let mut s = state.lock().await;
    if s.peers
        .get(&id)
        .is_some_and(|current| Arc::ptr_eq(current, &peer))
    {
        s.peers.remove(&id);
        s.broadcast();
    }
    work
}
pub async fn broker() -> Result<()> {
    let path = socket_path()?;
    let mut listener = crate::transport::Listener::bind(&path)?;
    let state = Arc::new(Mutex::new(Router::load()?));
    let shutdown = state.lock().await.shutdown.clone();
    let mut tasks = tokio::task::JoinSet::new();
    let terminate = crate::transport::terminate();
    tokio::pin!(terminate);
    let mut expiry = tokio::time::interval(Duration::from_secs(60));
    loop {
        tokio::select! {
            _=expiry.tick()=>state.lock().await.expire(),
            _=shutdown.notified()=>break,
            _= &mut terminate=>break,
            _=tokio::signal::ctrl_c()=>break,
            Some(_)=tasks.join_next(),if !tasks.is_empty()=>{},
            accepted=timeout(Duration::from_secs(10),listener.accept())=>match accepted {
                Ok(Ok(socket))=>{let state=state.clone();tasks.spawn(async move {
                    let mut socket=BufReader::new(socket);
                    let work=async {let hello=timeout(Duration::from_secs(5),crate::mcp::read_line(&mut socket)).await??.context("Missing hello")?;ensure!(hello["version"]==VERSION,"Unsupported router version");match hello["role"].as_str(){Some("chat")=>chat(socket,state,hello["identity"].as_str()).await,Some("extension")=>extension(socket,hello,state).await,Some("reload")=>{socket.get_mut().write_all(b"{\"reloading\":true}\n").await?;state.lock().await.shutdown.notify_one();Ok(())},_=>bail!("Unknown peer")}}.await;
                    if let Err(e)=work {eprintln!("Firefox extension connection: {e}");}
                });},
                Ok(Err(e))=>return Err(e),Err(_)=>if tasks.is_empty(){break;}
            }
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    Ok(())
}
/// Update/remove hook. Close only our private transport, never Firefox itself.
pub async fn reset() -> Result<()> {
    #[cfg(unix)]
    let mut paths = vec![socket_path()?];
    #[cfg(windows)]
    let paths = vec![socket_path()?];
    // Migrate a running 0.3.0 native host from its old XDG endpoint as well.
    // An explicit test endpoint must never reach the real user's router.
    #[cfg(unix)]
    if env::var_os("FIREFOX_MCP_RUNTIME_DIR").is_none() {
        let legacy = crate::debug::runtime_directory()?.join("extension-v1.sock");
        if !paths.contains(&legacy) {
            paths.push(legacy);
        }
    }
    for path in paths {
        let stream = match crate::transport::connect(&path).await {
            Ok(stream) => stream,
            Err(error) => {
                #[cfg(windows)]
                if !error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
                {
                    return Err(error);
                }
                #[cfg(unix)]
                let _ = error;
                continue;
            }
        };
        let mut stream = BufReader::new(stream);
        stream
            .get_mut()
            .write_all(format!("{}\n", json!({"role":"reload","version":VERSION})).as_bytes())
            .await?;
        let reply = timeout(Duration::from_secs(3), crate::mcp::read_line(&mut stream)).await??;
        ensure!(
            reply.is_some_and(|v| v["reloading"] == true),
            "Old extension connection must be closed manually"
        );
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn native_frames_reject_oversized_or_partial_messages() {
        let mut bytes = &[255, 255, 255, 255][..];
        assert!(read_frame(&mut bytes).await.is_err());
        let mut partial = &[1, 0][..];
        assert!(read_frame(&mut partial).await.is_err());
    }
    #[test]
    fn settings_controls_are_separate_from_mcp_tools() {
        assert!(crate::mcp::validate("openSettings", &json!({})).is_err());
        assert!(crate::mcp::validate("firefox_open_settings", &json!({})).is_err());
        assert!(valid_settings_id("d735d44e-1234-4567-8901-f6bd1d255cb3"));
        assert!(valid_settings_id("0123456789abcdef0123456789abcdef"));
        for invalid in [
            "",
            "settings",
            "../profile",
            "d735d44ex1234-4567-8901-f6bd1d255cb3",
        ] {
            assert!(!valid_settings_id(invalid));
        }
    }
    #[test]
    fn settings_urls_allow_only_the_canonical_loopback_manager() {
        let token = "a".repeat(64);
        assert!(valid_settings_url(&format!("http://127.0.0.1:1/#{token}")));
        assert!(valid_settings_url(&format!(
            "http://127.0.0.1:65535/#{token}"
        )));
        for invalid in [
            format!("https://127.0.0.1:1234/#{token}"),
            format!("http://localhost:1234/#{token}"),
            format!("http://127.0.0.1.evil:1234/#{token}"),
            format!("http://user@127.0.0.1:1234/#{token}"),
            format!("http://127.0.0.1:0/#{token}"),
            format!("http://127.0.0.1:01234/#{token}"),
            format!("http://127.0.0.1:65536/#{token}"),
            format!("http://127.0.0.1:1234/other/#{token}"),
            format!("http://127.0.0.1:1234/?q=value#{token}"),
            format!("http://127.0.0.1:1234/#{token}\n"),
            "http://127.0.0.1:1234/#short".into(),
            format!("http://127.0.0.1:1234/#{}", "g".repeat(64)),
        ] {
            assert!(!valid_settings_url(&invalid), "accepted {invalid}");
        }
    }
    #[tokio::test]
    async fn settings_startup_output_is_bounded_and_requires_the_fixed_prefix() -> Result<()> {
        let url = format!("http://127.0.0.1:1234/#{}", "a".repeat(64));
        for ending in ["\n", "\r\n"] {
            let line = format!("Firefox Codex setup: {url}{ending}");
            assert_eq!(read_settings_url(&mut line.as_bytes()).await?, url);
        }
        for line in [
            format!("{url}\n"),
            format!("Firefox Codex setup: {url}"),
            "Firefox Codex setup: http://example.com/\n".into(),
            "x".repeat(300),
        ] {
            assert!(read_settings_url(&mut line.as_bytes()).await.is_err());
        }
        Ok(())
    }
    #[test]
    fn settings_capability_is_added_by_this_host_without_changing_broker_state() {
        let ready = settings_capability(json!({"kind":"ready","epoch":"old-broker"}));
        assert_eq!(ready["epoch"], "old-broker");
        assert_eq!(ready["capabilities"], json!(["settings-ui-v1"]));
        let ready = settings_capability(json!({"kind":"ready","capabilities":["other"]}));
        assert_eq!(ready["capabilities"], json!(["other", "settings-ui-v1"]));
        assert_eq!(settings_capability(ready.clone()), ready);
        let configuration = json!({"kind":"configuration","selected":true});
        assert_eq!(settings_capability(configuration.clone()), configuration);
    }
    #[tokio::test]
    async fn opening_settings_reuses_the_running_manager() -> Result<()> {
        let url = format!("http://127.0.0.1:1234/#{}", "a".repeat(64));
        let mut settings = Settings {
            running: Some(RunningSettings {
                url: url.clone(),
                alive: Arc::new(AtomicBool::new(true)),
            }),
        };
        assert_eq!(settings.open().await?, url);
        assert_eq!(settings.open().await?, url);
        Ok(())
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn cancelling_settings_startup_kills_the_unseen_process() -> Result<()> {
        let child = Command::new("sleep")
            .arg("60")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let pid = child.id().context("Missing child process ID")?;
        let (started, ready) = oneshot::channel();
        let work = tokio::spawn(async move {
            let _starting = StartingSettings(Some(child));
            let _ = started.send(());
            std::future::pending::<()>().await;
        });
        ready.await?;
        work.abort();
        let error = work.await.expect_err("cancelled startup completed");
        assert!(error.is_cancelled());
        timeout(Duration::from_secs(2), async {
            while unsafe { libc::kill(pid as libc::pid_t, 0) } == 0 {
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .context("Cancelled Settings startup left its child running")?;
        Ok(())
    }
    #[tokio::test]
    async fn one_native_writer_preserves_frames_from_broker_and_settings() -> Result<()> {
        let (mut input, mut output) = tokio::io::duplex(64);
        let (tx, mut messages) = mpsc::channel(4);
        let broker = tx.clone();
        let settings = tx.clone();
        drop(tx);
        let writer = tokio::spawn(async move { write_frames(&mut output, &mut messages).await });
        let first = tokio::spawn(async move {
            for id in 0..32 {
                broker.send(json!({"kind":"request","id":id})).await?;
            }
            Ok::<(), anyhow::Error>(())
        });
        let second = tokio::spawn(async move {
            for id in 0..32 {
                settings
                    .send(json!({"kind":"settingsResult","id":id}))
                    .await?;
            }
            Ok::<(), anyhow::Error>(())
        });
        let mut seen = HashSet::new();
        for _ in 0..64 {
            let frame = read_frame(&mut input)
                .await?
                .context("Missing native frame")?;
            assert!(seen.insert((frame["kind"].to_string(), frame["id"].to_string())));
        }
        assert!(read_frame(&mut input).await?.is_none());
        first.await??;
        second.await??;
        writer.await??;
        Ok(())
    }
    #[test]
    fn identity_validation() {
        assert!(valid_id("d735d44e-1234-4567-8901-f6bd1d255cb3"));
        assert!(!valid_id("../profile"));
    }
    fn peer(id: &str) -> (Arc<Peer>, mpsc::Receiver<Value>) {
        let (tx, rx) = mpsc::channel(16);
        (
            Arc::new(Peer {
                id: id.into(),
                epoch: "test".into(),
                tx,
                pending: Mutex::new(HashMap::new()),
                alive: AtomicBool::new(true),
                closed: Notify::new(),
                continuity: true,
            }),
            rx,
        )
    }
    #[test]
    fn selection_never_falls_back_or_resolves_ambiguity() {
        let (a, _) = peer("profile-a");
        let (b, _) = peer("profile-b");
        let mut s = Router::default();
        s.peers.insert(a.id.clone(), a);
        s.peers.insert(b.id.clone(), b);
        assert!(s.choose().is_err());
        s.selected = Some("profile-a".into());
        assert_eq!(s.choose().unwrap().id, "profile-a");
        s.peers.remove("profile-a");
        assert!(s.choose().is_err());
    }
    #[tokio::test]
    async fn chat_eof_cancels_pending_dispatch_without_waiting_for_reply() {
        let (p, mut messages) = peer("profile-a");
        let mut router = Router {
            selected: Some(p.id.clone()),
            ..Default::default()
        };
        router.peers.insert(p.id.clone(), p);
        let (client, server) = tokio::io::duplex(65536);
        let mut client = BufReader::new(client);
        let work = tokio::spawn(chat(
            BufReader::new(Box::new(server)),
            Arc::new(Mutex::new(router)),
            None,
        ));
        crate::mcp::read_line(&mut client).await.unwrap();
        client
            .get_mut()
            .write_all(b"{\"name\":\"firefox_snapshot\",\"args\":{}}\n")
            .await
            .unwrap();
        assert_eq!(messages.recv().await.unwrap()["kind"], "attachOwner");
        let request = messages.recv().await.unwrap();
        assert_eq!(request["kind"], "request");
        drop(client);
        let drop = timeout(Duration::from_secs(1), messages.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(drop["kind"], "dropOwner");
        assert_eq!(drop["owner"], request["owner"]);
        timeout(Duration::from_secs(1), work)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
    #[tokio::test]
    async fn replacement_without_a_request_fences_old_extension_work() {
        let (p, mut messages) = peer("profile-a");
        let mut router = Router::default();
        router.peers.insert(p.id.clone(), p);
        let identity = "a".repeat(64);
        let old = router.chats.attach(Some(&identity)).unwrap();
        let (client, server) = tokio::io::duplex(65536);
        let mut client = BufReader::new(client);
        let state = Arc::new(Mutex::new(router));
        let copy = state.clone();
        let work = tokio::spawn(async move {
            chat(BufReader::new(Box::new(server)), copy, Some(&identity)).await
        });
        crate::mcp::read_line(&mut client).await.unwrap();
        let suspended = timeout(Duration::from_secs(1), messages.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(suspended["kind"], "suspendOwner");
        assert_eq!(suspended["owner"], old.owner);
        assert_eq!(suspended["attachment"], old.generation);
        state.lock().await.detach(&old);
        assert!(messages.try_recv().is_err());
        drop(client);
        timeout(Duration::from_secs(1), work)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}
