//! Windows filesystem primitives; advanced BiDi setup remains Linux-only.
use crate::windows::{self as w, check, wide};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use windows_sys::Win32::Storage::FileSystem::{
    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};

pub fn private_dir(path: &Path) -> Result<()> {
    w::no_reparse(path)?;
    fs::create_dir_all(path)?;
    w::no_reparse(path)
}
pub fn write_atomic(path: &Path, bytes: &[u8], _mode: u32) -> Result<()> {
    w::no_reparse(path)?;
    private_dir(path.parent().context("Missing parent directory")?)?;
    let temp = path.with_file_name(format!(".firefox-codex-{}.tmp", crate::random_hex(12)?));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        w::no_reparse(path)?;
        check(unsafe {
            MoveFileExW(
                wide(&temp).as_ptr(),
                wide(path).as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
pub fn profile(_: &str) -> Result<PathBuf> {
    bail!("BiDi mode is Linux-only; use the Firefox extension on Windows")
}
pub fn running(_: &Path) -> bool {
    false
}
pub fn install(_: &Path) -> Result<Value> {
    bail!("Use --install for the Windows extension connection")
}
pub fn launch(_: &Path) -> Result<Value> {
    bail!("Use normal Firefox with the extension on Windows")
}
pub fn notify_launch_error(message: &str) {
    eprintln!("{message}");
}
