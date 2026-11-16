//! Local extension router. Native Messaging and MCP have separate stdio owners.
//! All local peers have the same OS uid; this is not isolation from that user.
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::PathBuf,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    process::Command,
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
    // Desktop MCP children may omit XDG_RUNTIME_DIR while Firefox inherits it.
    // Both sides must choose the same endpoint without sharing launch environments.
    let directory = env::var_os("FIREFOX_MCP_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or(crate::onboarding::home()?.join(".local/state/firefox-codex-mcp/runtime"));
    Ok(crate::debug::private_runtime_directory(directory)?.join("extension-v1.sock"))
}
fn settings_path() -> Result<PathBuf> {
    let dir = crate::onboarding::home()?.join(".local/state/firefox-codex-mcp");
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
        use std::{
            io::Read,
            os::unix::fs::{MetadataExt, OpenOptionsExt},
        };
        let read = || -> Result<Option<Vec<u8>>> {
            let file = match fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(settings_path()?)
            {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e.into()),
            };
            let meta = file.metadata()?;
            ensure!(
                meta.is_file() && meta.uid() == unsafe { libc::geteuid() } && meta.len() < 65536,
                "Unsafe profile configuration"
            );
            let mut bytes = Vec::new();
            file.take(65536).read_to_end(&mut bytes)?;
            Ok(Some(bytes))
        };
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
            return self.peers.get(id).filter(|p|p.alive.load(Ordering::SeqCst)).cloned().context("선택한 Firefox가 연결되지 않았습니다. 해당 Firefox를 여세요. 다른 프로필로 자동 전환하지 않습니다.");
        }
        ensure!(self.peers.len()<=1,"Firefox 프로필이 여러 개 연결됐습니다. 사용할 Firefox 확장에서 ‘이 Firefox 사용’을 한 번 누르세요.");
        self.peers.values().next().filter(|p|p.alive.load(Ordering::SeqCst)).cloned().context("Firefox 확장이 연결되지 않았습니다. 확장 설치 후 Firefox를 여세요. 코드·토큰·프로필 경로는 필요 없습니다.")
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
pub async fn chat_client(identity: Option<&str>) -> Result<BufReader<UnixStream>> {
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
    let hello = timeout(Duration::from_secs(5), crate::debug::read_line(&mut socket)).await??;
    ensure!(
        hello.is_some_and(|v| v["ready"] == VERSION),
        "Extension broker version mismatch. Close old Firefox MCP chats and reopen Firefox."
    );
    Ok(socket)
}
async fn connect() -> Result<BufReader<UnixStream>> {
    let path = socket_path()?;
    if let Ok(stream) = crate::debug::connect_socket(&path).await {
        return Ok(BufReader::new(stream));
    }
    let mut child = Command::new(env::current_exe()?)
        .arg("--extension-broker")
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    for _ in 0..80 {
        sleep(Duration::from_millis(50)).await;
        if let Ok(s) = crate::debug::connect_socket(&path).await {
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
    let (read, mut write) = socket.into_inner().into_split();
    let up = async {
        while let Some(value) = read_frame(&mut input).await? {
            write.write_all(format!("{value}\n").as_bytes()).await?;
        }
        Ok::<(), anyhow::Error>(())
    };
    let down = async {
        let mut read = BufReader::new(read);
        let mut output = tokio::io::stdout();
        while let Some(value) = crate::debug::read_line(&mut read).await? {
            let bytes = serde_json::to_vec(&value)?;
            ensure!(bytes.len() < crate::LIMIT, "Native reply too large");
            output
                .write_all(&(bytes.len() as u32).to_ne_bytes())
                .await?;
            output.write_all(&bytes).await?;
            output.flush().await?;
        }
        Ok::<(), anyhow::Error>(())
    };
    tokio::select! {r=up=>r,r=down=>r}
}
async fn launch_if_needed(state: &Arc<Mutex<Router>>) -> Result<()> {
    let mut s = state.lock().await;
    if s.peers.is_empty()
        && s.last_launch
            .is_none_or(|t| t.elapsed() > Duration::from_secs(30))
    {
        s.last_launch = Some(tokio::time::Instant::now());
        let mut child = Command::new(
            env::var_os("FIREFOX_BINARY").unwrap_or_else(|| "/usr/bin/firefox".into()),
        )
        .process_group(0)
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
    mut socket: BufReader<UnixStream>,
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
    let (read, mut write) = socket.into_inner().into_split();
    let mut read = BufReader::new(std::io::Cursor::new(buffered).chain(read));
    let (tx, mut requests) = mpsc::channel(32);
    let (ended, closed) = oneshot::channel();
    let reader_state = state.clone();
    let reader_attachment = a.clone();
    let reader = tokio::spawn(async move {
        while let Ok(Some(request)) = crate::debug::read_line(&mut read).await {
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
                crate::debug::validate(name, &args)?;
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
    socket: BufReader<UnixStream>,
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
    let (read, mut write) = socket.into_inner().into_split();
    let mut read = BufReader::new(std::io::Cursor::new(buffered).chain(read));
    let work=async {
        loop {if !peer.alive.load(Ordering::SeqCst){break;} tokio::select!{
            _=peer.closed.notified()=>break,
            message=rx.recv()=>{let Some(message)=message else {break;};write.write_all(format!("{message}\n").as_bytes()).await?;},
            message=crate::debug::read_line(&mut read)=>{let Some(message)=message? else{break;};
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
    use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
    let path = socket_path()?;
    let _lock = crate::debug::broker_lock(&path.with_extension("lock"))?;
    if let Ok(meta) = fs::symlink_metadata(&path) {
        ensure!(
            meta.file_type().is_socket() && meta.uid() == unsafe { libc::geteuid() },
            "Unsafe router socket"
        );
        fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    let state = Arc::new(Mutex::new(Router::load()?));
    let shutdown = state.lock().await.shutdown.clone();
    let mut tasks = tokio::task::JoinSet::new();
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut expiry = tokio::time::interval(Duration::from_secs(60));
    loop {
        tokio::select! {
            _=expiry.tick()=>state.lock().await.expire(),
            _=shutdown.notified()=>break,
            _=term.recv()=>break,
            _=tokio::signal::ctrl_c()=>break,
            Some(_)=tasks.join_next(),if !tasks.is_empty()=>{},
            accepted=timeout(Duration::from_secs(10),listener.accept())=>match accepted {
                Ok(Ok((socket,_)))=>{ensure!(socket.peer_cred()?.uid()==unsafe{libc::geteuid()},"Unexpected router peer");let state=state.clone();tasks.spawn(async move {
                    let mut socket=BufReader::new(socket);
                    let work=async {let hello=timeout(Duration::from_secs(5),crate::debug::read_line(&mut socket)).await??.context("Missing hello")?;ensure!(hello["version"]==VERSION,"Unsupported router version");match hello["role"].as_str(){Some("chat")=>chat(socket,state,hello["identity"].as_str()).await,Some("extension")=>extension(socket,hello,state).await,Some("reload")=>{socket.get_mut().write_all(b"{\"reloading\":true}\n").await?;state.lock().await.shutdown.notify_one();Ok(())},_=>bail!("Unknown peer")}}.await;
                    if let Err(e)=work {eprintln!("Firefox extension connection: {e}");}
                });},
                Ok(Err(e))=>return Err(e.into()),Err(_)=>if tasks.is_empty(){break;}
            }
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    fs::remove_file(path)?;
    Ok(())
}
/// Update/remove hook. Close only our private transport, never Firefox itself.
pub async fn reset() -> Result<()> {
    let mut paths = vec![socket_path()?];
    // Migrate a running 0.3.0 native host from its old XDG endpoint as well.
    // An explicit test endpoint must never reach the real user's router.
    if env::var_os("FIREFOX_MCP_RUNTIME_DIR").is_none() {
        let legacy = crate::debug::runtime_directory()?.join("extension-v1.sock");
        if !paths.contains(&legacy) {
            paths.push(legacy);
        }
    }
    for path in paths {
        let Ok(stream) = crate::debug::connect_socket(&path).await else {
            continue;
        };
        let mut stream = BufReader::new(stream);
        stream
            .get_mut()
            .write_all(format!("{}\n", json!({"role":"reload","version":VERSION})).as_bytes())
            .await?;
        let reply = timeout(Duration::from_secs(3), crate::debug::read_line(&mut stream)).await??;
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
        let (client, server) = UnixStream::pair().unwrap();
        let mut client = BufReader::new(client);
        let work = tokio::spawn(chat(
            BufReader::new(server),
            Arc::new(Mutex::new(router)),
            None,
        ));
        crate::debug::read_line(&mut client).await.unwrap();
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
        let (client, server) = UnixStream::pair().unwrap();
        let mut client = BufReader::new(client);
        let state = Arc::new(Mutex::new(router));
        let copy = state.clone();
        let work =
            tokio::spawn(async move { chat(BufReader::new(server), copy, Some(&identity)).await });
        crate::debug::read_line(&mut client).await.unwrap();
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
