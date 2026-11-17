use super::*;
use crate::transport::Stream;
use sha2::{Digest, Sha256};
use std::os::windows::io::AsRawHandle;
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions};
use windows_sys::Win32::System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId};

pub fn state_dir() -> Result<PathBuf> {
    Ok(local_root()?.join("state"))
}
pub fn endpoint() -> Result<PathBuf> {
    let identity = Identity::current()?;
    let root = local_root()?;
    let namespace =
        std::env::var_os("FIREFOX_MCP_RUNTIME_DIR").unwrap_or_else(|| root.into_os_string());
    let key = format!(
        "{}:{}:{}:{}",
        identity.user,
        identity.logon,
        identity.session,
        namespace.to_string_lossy()
    );
    Ok(PathBuf::from(format!(
        r"\\.\pipe\firefox-codex-mcp-v1-{:x}",
        Sha256::digest(key.as_bytes())
    )))
}
pub async fn connect(path: &Path) -> Result<Stream> {
    // Identification only: a server must never impersonate a connecting client.
    let socket = ClientOptions::new()
        .security_qos_flags(0x0010_0000 | 0x0001_0000)
        .open(path)?;
    let mut pid = 0;
    check(unsafe { GetNamedPipeServerProcessId(socket.as_raw_handle(), &mut pid) })?;
    ensure!(
        Identity::pid(pid)? == Identity::current()?,
        "Unexpected pipe server user or logon session"
    );
    Ok(Box::new(socket))
}
pub struct Listener {
    socket: NamedPipeServer,
    path: PathBuf,
}
impl Listener {
    fn create(path: &Path, first: bool) -> Result<NamedPipeServer> {
        let identity = Identity::current()?;
        // Only this interactive logon. No Everyone/anonymous/default DACL access.
        let descriptor = security(&format!("D:P(A;;GA;;;{})", identity.logon))?;
        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        let socket = unsafe {
            ServerOptions::new()
                .first_pipe_instance(first)
                .reject_remote_clients(true)
                .create_with_security_attributes_raw(
                    path,
                    (&mut attributes as *mut SECURITY_ATTRIBUTES).cast(),
                )
        }?;
        Ok(socket)
    }
    pub fn bind(path: &Path) -> Result<Self> {
        Ok(Self {
            socket: Self::create(path, true)?,
            path: path.into(),
        })
    }
    pub async fn accept(&mut self) -> Result<Stream> {
        loop {
            self.socket.connect().await?;
            // Keep a server instance alive across accept: no name-squatting gap.
            let replacement = Self::create(&self.path, false)?;
            let socket = std::mem::replace(&mut self.socket, replacement);
            let mut pid = 0;
            let verified =
                check(unsafe { GetNamedPipeClientProcessId(socket.as_raw_handle(), &mut pid) })
                    .and_then(|_| Identity::pid(pid));
            if verified.is_ok_and(|v| Identity::current().is_ok_and(|ours| v == ours)) {
                return Ok(Box::new(socket));
            }
            // Reject a peer without taking down unrelated chats.
        }
    }
}
pub fn read_private(path: &Path, limit: u64) -> Result<Option<Vec<u8>>> {
    use std::io::Read;
    no_reparse(path)?;
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() < limit,
        "Unsafe private configuration"
    );
    let mut bytes = Vec::new();
    file.take(limit).read_to_end(&mut bytes)?;
    Ok(Some(bytes))
}
pub fn detach(command: &mut tokio::process::Command) {
    // std::process uses an explicit inherited handle list; the caller sets all
    // three standard handles to NUL. Never inherit Firefox's native stdio.
    command.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
}
pub fn firefox() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("FIREFOX_BINARY") {
        let path = PathBuf::from(path);
        ensure!(
            path.is_absolute() && path.is_file(),
            "FIREFOX_BINARY must name an existing absolute executable"
        );
        return Ok(path);
    }
    use winreg::{enums::*, RegKey};
    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
            if let Ok(key) = RegKey::predef(hive).open_subkey_with_flags(
                r"Software\Microsoft\Windows\CurrentVersion\App Paths\firefox.exe",
                KEY_READ | view,
            ) {
                if let Ok(value) = key.get_value::<String, _>("") {
                    let path = PathBuf::from(value);
                    if path.is_absolute() && path.is_file() {
                        return Ok(path);
                    }
                }
            }
        }
    }
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(variable) {
            let path = PathBuf::from(root).join("Mozilla Firefox/firefox.exe");
            if path.is_file() {
                return Ok(path);
            }
        }
    }
    anyhow::bail!("Install desktop Firefox from https://www.mozilla.org/firefox/ and run this installer again. Microsoft Store Firefox is not yet validated.")
}
pub async fn terminate() {
    std::future::pending::<()>().await;
}
