//! Windows adapters. No elevation, browser debugging, or TCP IPC listener.
pub mod transport;
use anyhow::{ensure, Context, Result};
use std::{
    ffi::{c_void, OsStr, OsString},
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, LocalFree, HANDLE},
    Security::*,
    System::Threading::*,
};

pub fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}
pub struct Handle(pub HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
// HANDLEs are kernel references. This owner closes once and never aliases a mutable buffer.
unsafe impl Send for Handle {}
pub struct Local(pub *mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
pub fn check(ok: i32) -> Result<()> {
    if ok == 0 {
        Err(std::io::Error::last_os_error().into())
    } else {
        Ok(())
    }
}
fn token_info(token: HANDLE, class: TOKEN_INFORMATION_CLASS) -> Result<Vec<usize>> {
    let mut size = 0;
    unsafe {
        GetTokenInformation(token, class, std::ptr::null_mut(), 0, &mut size);
    }
    ensure!(size > 0 && size < 65536, "Cannot query process token");
    let mut bytes = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
    check(unsafe {
        GetTokenInformation(token, class, bytes.as_mut_ptr().cast(), size, &mut size)
    })?;
    Ok(bytes)
}
fn sid_string(sid: PSID) -> Result<String> {
    let mut text = std::ptr::null_mut();
    check(unsafe { Authorization::ConvertSidToStringSidW(sid, &mut text) })?;
    let _allocation = Local(text.cast());
    let mut len = 0;
    unsafe {
        while *text.add(len) != 0 {
            len += 1;
        }
    }
    Ok(String::from_utf16(unsafe {
        std::slice::from_raw_parts(text, len)
    })?)
}
#[derive(Debug, PartialEq, Eq)]
pub struct Identity {
    pub user: String,
    pub logon: String,
    pub session: u32,
}
impl Identity {
    pub fn current() -> Result<Self> {
        Self::process(unsafe { GetCurrentProcess() })
    }
    pub fn pid(pid: u32) -> Result<Self> {
        let process = Handle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) });
        ensure!(!process.0.is_null(), "Cannot verify pipe peer process");
        Self::process(process.0)
    }
    fn process(process: HANDLE) -> Result<Self> {
        let mut handle = std::ptr::null_mut();
        check(unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut handle) })?;
        let token = Handle(handle);
        let user = token_info(token.0, TokenUser)?;
        let user = sid_string(unsafe { (*(user.as_ptr().cast::<TOKEN_USER>())).User.Sid })?;
        let groups = token_info(token.0, TokenGroups)?;
        let groups = unsafe { &*groups.as_ptr().cast::<TOKEN_GROUPS>() };
        let list = unsafe {
            std::slice::from_raw_parts(groups.Groups.as_ptr(), groups.GroupCount as usize)
        };
        let group = list
            .iter()
            .find(|g| g.Attributes & 0xc000_0000 == 0xc000_0000)
            .context("Missing logon SID")?;
        let logon = sid_string(group.Sid)?;
        let session = token_info(token.0, TokenSessionId)?;
        let session = unsafe { *session.as_ptr().cast::<u32>() };
        Ok(Self {
            user,
            logon,
            session,
        })
    }
}
pub fn security(sddl: &str) -> Result<Local> {
    let mut descriptor = std::ptr::null_mut();
    check(unsafe {
        Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide(sddl).as_ptr(),
            1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    })?;
    Ok(Local(descriptor))
}
pub fn known_folder(id: &windows_sys::core::GUID) -> Result<PathBuf> {
    let mut path = std::ptr::null_mut();
    let hr = unsafe {
        windows_sys::Win32::UI::Shell::SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut path)
    };
    ensure!(hr >= 0, "Cannot find Windows user folder: {hr:x}");
    let mut len = 0;
    unsafe {
        while *path.add(len) != 0 {
            len += 1;
        }
    }
    let result = PathBuf::from(OsString::from_wide(unsafe {
        std::slice::from_raw_parts(path, len)
    }));
    unsafe {
        windows_sys::Win32::System::Com::CoTaskMemFree(path.cast());
    }
    Ok(result)
}
pub fn local_root() -> Result<PathBuf> {
    // Explicit override is for isolated tests and portable installations only.
    let root = std::env::var_os("FIREFOX_MCP_INSTALL_ROOT")
        .map(PathBuf::from)
        .unwrap_or(
            known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_LocalAppData)?
                .join("FirefoxCodexMCP"),
        );
    ensure!(root.is_absolute(), "Installation root must be absolute");
    Ok(root)
}
pub fn no_reparse(path: &Path) -> Result<()> {
    use std::os::windows::fs::MetadataExt;
    for part in path.ancestors() {
        match std::fs::symlink_metadata(part) {
            Ok(meta) => ensure!(
                meta.file_attributes() & 0x400 == 0,
                "Reparse points are not supported for managed paths: {}",
                part.display()
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
pub fn open_manager(url: &str) -> Result<()> {
    use std::process::{Command, Stdio};
    if let Ok(binary) = transport::firefox() {
        Command::new(binary)
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
    } else {
        let result = unsafe {
            windows_sys::Win32::UI::Shell::ShellExecuteW(
                std::ptr::null_mut(),
                wide("open").as_ptr(),
                wide(url).as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            )
        };
        ensure!(
            result as isize > 32,
            "Cannot open setup. Open the localhost URL printed above in a browser."
        );
    }
    Ok(())
}
