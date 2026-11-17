//! Trusted, system-managed Linux installation identity.
//!
//! Package hooks replace `installation.json` with a fresh generation after an
//! upgrade and remove it before removal. Running processes retain this identity
//! and must stop accepting work as soon as `validate` fails. No hook needs to
//! enumerate or modify users' home directories.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

pub const EXECUTABLE: &str = "/usr/libexec/firefox-codex-mcp/firefox-codex-mcp";
pub const MARKER: &str = "/usr/libexec/firefox-codex-mcp/installation.json";
pub const NATIVE_HOST: &str = "/usr/libexec/firefox-codex-mcp/native-host";
pub const MANIFESTS: [&str; 2] = [
    "/usr/lib/mozilla/native-messaging-hosts/firefox_codex_mcp.json",
    "/usr/lib64/mozilla/native-messaging-hosts/firefox_codex_mcp.json",
];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct Marker {
    schema: u32,
    version: String,
    generation: String,
    executable_sha256: String,
}

#[derive(Clone, Debug)]
pub struct Installation {
    marker: Marker,
    executable_id: (u64, u64),
}

fn trusted_metadata(path: &Path, owner: u32, directory: bool) -> Result<Metadata> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("System package file is unavailable: {}", path.display()))?;
    ensure!(
        metadata.uid() == owner
            && metadata.mode() & 0o022 == 0
            && if directory {
                metadata.is_dir()
            } else {
                metadata.is_file()
            },
        "System package path is not trusted: {}",
        path.display()
    );
    Ok(metadata)
}

fn trusted_file(path: &Path, owner: u32) -> Result<File> {
    for parent in path.ancestors().skip(1) {
        trusted_metadata(parent, owner, true)?;
    }
    let expected = trusted_metadata(path, owner, false)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.dev() == expected.dev() && metadata.ino() == expected.ino(),
        "System package file changed while opening: {}",
        path.display()
    );
    Ok(file)
}

fn marker_at(path: &Path, owner: u32) -> Result<Marker> {
    let file = trusted_file(path, owner)?;
    ensure!(
        file.metadata()?.len() <= 4096,
        "Invalid system package marker"
    );
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 4096, "Invalid system package marker");
    parse_marker(&bytes)
}

fn parse_marker(bytes: &[u8]) -> Result<Marker> {
    let marker: Marker = serde_json::from_slice(bytes).context("Invalid system package marker")?;
    ensure!(
        marker.schema == 1
            && !marker.version.is_empty()
            && marker.version.len() <= 100
            && (16..=128).contains(&marker.generation.len())
            && marker
                .generation
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && marker.executable_sha256.len() == 64
            && marker
                .executable_sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit()),
        "Invalid system package marker"
    );
    Ok(marker)
}

fn file_digest(mut file: File) -> Result<String> {
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

impl Installation {
    pub fn executable(&self) -> &Path {
        Path::new(EXECUTABLE)
    }

    pub fn generation(&self) -> &str {
        &self.marker.generation
    }

    pub fn version(&self) -> &str {
        &self.marker.version
    }

    pub fn executable_hash(&self) -> &str {
        &self.marker.executable_sha256
    }

    /// A missing marker, package replacement or untrusted path revokes work.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            marker_at(Path::new(MARKER), 0)? == self.marker,
            "The system package was updated or removed. Reconnect using the installed version."
        );
        let metadata = trusted_metadata(Path::new(EXECUTABLE), 0, false)?;
        ensure!(
            (metadata.dev(), metadata.ino()) == self.executable_id,
            "The system package executable changed. Reconnect using the installed version."
        );
        Ok(())
    }

    pub fn native_host_registered(&self) -> bool {
        self.validate().is_ok()
            && trusted_file(Path::new(NATIVE_HOST), 0).is_ok()
            && MANIFESTS.iter().any(|path| {
                let Ok(file) = trusted_file(Path::new(path), 0) else {
                    return false;
                };
                let Ok(value) = serde_json::from_reader::<_, serde_json::Value>(file.take(4097))
                else {
                    return false;
                };
                value["name"] == crate::native::HOST
                    && value["path"] == NATIVE_HOST
                    && value["type"] == "stdio"
                    && value["allowed_extensions"]
                        == serde_json::json!([crate::native::EXTENSION_ID])
            })
    }
}

/// Inspect the fixed package slot. A damaged slot never falls back to a user
/// installation, which would hide the package manager's ownership from users.
pub fn installed() -> Result<Option<Installation>> {
    let path = Path::new(EXECUTABLE);
    let present = |path: &Path| match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    };
    if !present(path)? && !present(Path::new(MARKER))? {
        return Ok(None);
    }
    let marker = marker_at(Path::new(MARKER), 0)?;
    let file = trusted_file(path, 0)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.mode() & 0o111 != 0
            && file_digest(file)?.eq_ignore_ascii_case(&marker.executable_sha256),
        "The system package executable does not match its installation marker. Reinstall the package."
    );
    let installation = Installation {
        marker,
        executable_id: (metadata.dev(), metadata.ino()),
    };
    installation.validate()?;
    Ok(Some(installation))
}

/// Identify the running binary, including an old Linux executable whose inode
/// has been unlinked by an upgrade. Portable tools have no package identity.
pub fn current() -> Result<Option<Installation>> {
    let executable = std::env::current_exe()?;
    let old = PathBuf::from(format!("{EXECUTABLE} (deleted)"));
    if executable != Path::new(EXECUTABLE) && executable != old {
        return Ok(None);
    }
    let installation = installed()?.context("The system package has been removed")?;
    let running =
        File::open("/proc/self/exe").context("Cannot verify the running package executable")?;
    let running_metadata = running.metadata()?;
    ensure!(
        (running_metadata.dev(), running_metadata.ino()) == installation.executable_id
            && file_digest(running)?.eq_ignore_ascii_case(installation.executable_hash()),
        "This process belongs to an older system package. Reconnect using the installed version."
    );
    Ok(Some(installation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn validates_marker_schema_and_rejects_user_writable_parent() -> Result<()> {
        let marker = Marker {
            schema: 1,
            version: "0.4.3".into(),
            generation: "a".repeat(64),
            executable_sha256: "b".repeat(64),
        };
        assert_eq!(parse_marker(&serde_json::to_vec(&marker)?)?, marker);
        for changed in [
            Marker {
                schema: 2,
                ..marker.clone()
            },
            Marker {
                generation: "../invalid".into(),
                ..marker.clone()
            },
            Marker {
                executable_sha256: "short".into(),
                ..marker.clone()
            },
        ] {
            assert!(parse_marker(&serde_json::to_vec(&changed)?).is_err());
        }
        let temp = tempfile::tempdir()?;
        // marker_at's parent-chain check deliberately rejects /tmp. Production
        // package paths cannot be redirected to user-writable test roots.
        let path = temp.path().join("installation.json");
        fs::write(&path, serde_json::to_vec(&marker)?)?;
        assert!(marker_at(&path, unsafe { libc::geteuid() }).is_err());
        Ok(())
    }

    #[test]
    fn rejects_symlinks_and_writable_package_files() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let file = temp.path().join("binary");
        fs::write(&file, b"payload")?;
        let owner = unsafe { libc::geteuid() };
        fs::set_permissions(&file, fs::Permissions::from_mode(0o666))?;
        assert!(trusted_metadata(&file, owner, false).is_err());
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644))?;
        assert!(trusted_metadata(&file, owner, false).is_ok());
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(&file, &link)?;
        assert!(trusted_metadata(&link, owner, false).is_err());
        Ok(())
    }
}
