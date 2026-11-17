//! The router protocol is shared; only local IPC and process setup vary by OS.
#[cfg(unix)]
use anyhow::Result;
#[cfg(unix)]
use std::path::{Path, PathBuf};
use tokio::io::{AsyncRead, AsyncWrite};
pub trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}
pub type Stream = Box<dyn Io>;

#[cfg(windows)]
pub use crate::windows::transport::*;

#[cfg(unix)]
pub fn state_dir() -> Result<PathBuf> {
    Ok(crate::onboarding::home()?.join(".local/state/firefox-codex-mcp"))
}
#[cfg(unix)]
pub fn endpoint() -> Result<PathBuf> {
    let directory = std::env::var_os("FIREFOX_MCP_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or(state_dir()?.join("runtime"));
    Ok(crate::debug::private_runtime_directory(directory)?.join("extension-v1.sock"))
}
#[cfg(unix)]
pub async fn connect(path: &Path) -> Result<Stream> {
    Ok(Box::new(crate::debug::connect_socket(path).await?))
}
#[cfg(unix)]
pub struct Listener {
    socket: tokio::net::UnixListener,
    path: PathBuf,
    _lock: std::fs::File,
}
#[cfg(unix)]
impl Listener {
    pub fn bind(path: &Path) -> Result<Self> {
        use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
        let lock = crate::debug::broker_lock(&path.with_extension("lock"))?;
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            anyhow::ensure!(
                meta.file_type().is_socket() && meta.uid() == unsafe { libc::geteuid() },
                "Unsafe router socket"
            );
            std::fs::remove_file(path)?;
        }
        let socket = tokio::net::UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(Self {
            socket,
            path: path.into(),
            _lock: lock,
        })
    }
    pub async fn accept(&mut self) -> Result<Stream> {
        let (stream, _) = self.socket.accept().await?;
        anyhow::ensure!(
            stream.peer_cred()?.uid() == unsafe { libc::geteuid() },
            "Unexpected router peer"
        );
        Ok(Box::new(stream))
    }
}
#[cfg(unix)]
impl Drop for Listener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
#[cfg(unix)]
pub fn read_private(path: &Path, limit: u64) -> Result<Option<Vec<u8>>> {
    use std::{
        io::Read,
        os::unix::fs::{MetadataExt, OpenOptionsExt},
    };
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let meta = file.metadata()?;
    anyhow::ensure!(
        meta.is_file() && meta.uid() == unsafe { libc::geteuid() } && meta.len() < limit,
        "Unsafe private configuration"
    );
    let mut bytes = Vec::new();
    file.take(limit).read_to_end(&mut bytes)?;
    Ok(Some(bytes))
}
#[cfg(unix)]
pub fn detach(command: &mut tokio::process::Command) {
    command.process_group(0);
}
#[cfg(unix)]
pub fn firefox() -> Result<PathBuf> {
    Ok(std::env::var_os("FIREFOX_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| "/usr/bin/firefox".into()))
}
#[cfg(unix)]
pub async fn terminate() {
    if let Ok(mut signal) =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
    {
        signal.recv().await;
    } else {
        std::future::pending::<()>().await;
    }
}
