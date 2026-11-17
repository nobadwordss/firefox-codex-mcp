//! Transactional, user-level install/update/remove. Never stops user applications.
use crate::{onboarding as o, package_install, setup};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::Command,
};
use toml_edit::{value, Array, DocumentMut, Item, Table};

const PREF_START: &str = "// firefox-codex-mcp:start";
const PREF_END: &str = "// firefox-codex-mcp:end";
const PREF_BLOCK: &str = "// firefox-codex-mcp:start\nuser_pref(\"remote.prefs.recommended\", false);\n// firefox-codex-mcp:end\n";
#[derive(Clone, Serialize, Deserialize, PartialEq)]
struct Snapshot {
    #[serde(with = "bytes64")]
    bytes: Vec<u8>,
    mode: u32,
}
mod bytes64 {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(bytes))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        STANDARD
            .decode(String::deserialize(d)?)
            .map_err(serde::de::Error::custom)
    }
}
#[derive(Serialize, Deserialize)]
struct Change {
    path: PathBuf,
    before: Option<Snapshot>,
    after: Option<Snapshot>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Managed {
    path: PathBuf,
    before: Option<Snapshot>,
    installed: Snapshot,
    block: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct Receipt {
    version: String,
    #[serde(default = "legacy_mode")]
    mode: String,
    profile: PathBuf,
    codex_home: PathBuf,
    files: Vec<Managed>,
    executable_hash: String,
    #[serde(default = "portable_delivery")]
    delivery: String,
    #[serde(default)]
    executable: Option<PathBuf>,
    #[serde(default)]
    connection_generation: Option<String>,
}

fn legacy_mode() -> String {
    "bidi".into()
}
fn portable_delivery() -> String {
    "portable".into()
}
impl Receipt {
    fn executable(&self, home: &Path) -> PathBuf {
        self.executable
            .clone()
            .unwrap_or_else(|| user_executable(home))
    }
}
fn user_executable(home: &Path) -> PathBuf {
    home.join(".local/lib/firefox-codex-mcp/firefox-codex-mcp")
}

struct Layout {
    home: PathBuf,
    root: PathBuf,
    executable: PathBuf,
    package: Option<package_install::Installation>,
    receipt: PathBuf,
    pending: PathBuf,
    _lock: File,
}
impl Layout {
    fn open() -> Result<Self> {
        let home = o::home()?;
        ensure!(home.is_absolute(), "HOME must be absolute");
        let root = connection_codex_home(&home)?;
        ensure!(root.is_absolute(), "CODEX_HOME must be absolute");
        let package = package_install::installed()?;
        let executable = package
            .as_ref()
            .map(|installation| installation.executable().to_owned())
            .unwrap_or_else(|| user_executable(&home));
        let directory = home.join(".local/state/firefox-codex-mcp");
        setup::private_dir(&directory)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(directory.join("install.lock"))?;
        lock.try_lock().context(
            "Another installation, update or removal is running. Try again after it finishes.",
        )?;
        Ok(Self {
            executable,
            package,
            receipt: directory.join("install.json"),
            pending: directory.join("transaction.json"),
            home,
            root,
            _lock: lock,
        })
    }
    fn receipt(&self) -> Result<Option<Receipt>> {
        snapshot(&self.receipt)?
            .map(|s| serde_json::from_slice(&s.bytes).map_err(Into::into))
            .transpose()
    }
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn snapshot(path: &Path) -> Result<Option<Snapshot>> {
    match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(file) => {
            let meta = file.metadata()?;
            ensure!(
                meta.is_file()
                    && meta.uid() == unsafe { libc::geteuid() }
                    && meta.len() <= 32 * 1024 * 1024,
                "Cannot safely manage {}",
                path.display()
            );
            use std::io::Read;
            let mut bytes = Vec::new();
            file.take(32 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() <= 32 * 1024 * 1024,
                "File changed while reading"
            );
            Ok(Some(Snapshot {
                bytes,
                mode: meta.mode() & 0o777,
            }))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| {
            format!(
                "Cannot read {}; symbolic links are not overwritten",
                path.display()
            )
        }),
    }
}
/// Firefox may not inherit a custom CODEX_HOME used by the connecting shell.
/// Follow that user's trusted receipt when no explicit override was supplied.
pub fn connection_codex_home(home: &Path) -> Result<PathBuf> {
    connection_codex_home_with_override(home, std::env::var_os("CODEX_HOME"))
}
fn connection_codex_home_with_override(
    home: &Path,
    explicit: Option<std::ffi::OsString>,
) -> Result<PathBuf> {
    let root = if let Some(root) = explicit {
        PathBuf::from(root)
    } else if let Some(snapshot) =
        snapshot(&home.join(".local/state/firefox-codex-mcp/install.json"))?
    {
        serde_json::from_slice::<Receipt>(&snapshot.bytes)
            .context("Cannot read the existing installation record")?
            .codex_home
    } else {
        o::codex_home(home)
    };
    ensure!(root.is_absolute(), "CODEX_HOME must be absolute");
    match fs::symlink_metadata(&root) {
        Ok(metadata) => ensure!(
            metadata.is_dir() && metadata.uid() == unsafe { libc::geteuid() },
            "The Codex configuration directory is not owned by this user or is a symbolic link: {}",
            root.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(root)
}
fn put(path: &Path, value: &Option<Snapshot>) -> Result<()> {
    match value {
        Some(value) => {
            fs::create_dir_all(path.parent().context("Missing parent")?)?;
            setup::write_atomic(path, &value.bytes, value.mode)
        }
        None => {
            if path.exists() {
                remove_durable(path)?;
            }
            Ok(())
        }
    }
}
fn remove_durable(path: &Path) -> Result<()> {
    fs::remove_file(path)?;
    File::open(path.parent().context("Missing parent")?)?.sync_all()?;
    Ok(())
}
fn restore(changes: &[Change]) -> Result<()> {
    for change in changes.iter().rev() {
        let current = snapshot(&change.path)?;
        if current == change.before {
            continue;
        }
        ensure!(current==change.after,"Settings changed outside setup, so automatic recovery stopped: {}. The transaction.json backup was preserved.",change.path.display());
        put(&change.path, &change.before)?;
    }
    Ok(())
}
fn recover(layout: &Layout) -> Result<()> {
    if let Some(pending) = snapshot(&layout.pending)? {
        let changes: Vec<Change> = serde_json::from_slice(&pending.bytes)?;
        let completed = changes
            .iter()
            .all(|c| snapshot(&c.path).is_ok_and(|s| s == c.after));
        if !completed {
            restore(&changes)?;
        }
        remove_durable(&layout.pending)?;
    }
    Ok(())
}
fn transact(layout: &Layout, changes: Vec<Change>) -> Result<()> {
    for c in &changes {
        ensure!(
            snapshot(&c.path)? == c.before,
            "Settings changed. Try again: {}",
            c.path.display()
        );
    }
    setup::write_atomic(&layout.pending, &serde_json::to_vec(&changes)?, 0o600)?;
    for c in &changes {
        let write = (|| {
            ensure!(
                snapshot(&c.path)? == c.before,
                "Settings changed: {}",
                c.path.display()
            );
            put(&c.path, &c.after)
        })();
        if let Err(error) = write {
            restore(&changes)
                .context("Recovery after installation failure did not finish. Run --repair")?;
            remove_durable(&layout.pending)?;
            return Err(error);
        }
    }
    remove_durable(&layout.pending)?;
    Ok(())
}
fn text(value: &Option<Snapshot>) -> Result<String> {
    value
        .as_ref()
        .map(|v| String::from_utf8(v.bytes.clone()).map_err(Into::into))
        .unwrap_or_else(|| Ok(String::new()))
}
fn restore_managed_file(file: &Managed, preserved: &mut Vec<String>) -> Result<Option<Change>> {
    let current = snapshot(&file.path)?;
    // A user deletion is a change too. Never restore an older baseline over it.
    if current.is_none() {
        return Ok(None);
    }
    let after = if current.as_ref() == Some(&file.installed) {
        file.before.clone()
    } else if let Some(block) = &file.block {
        let content = text(&current)?;
        if content.matches(block).count() != 1 {
            preserved.push(file.path.display().to_string());
            return Ok(None);
        }
        Some(Snapshot {
            bytes: content.replacen(block, "", 1).into_bytes(),
            mode: current.as_ref().map_or(0o600, |s| s.mode),
        })
    } else {
        preserved.push(file.path.display().to_string());
        return Ok(None);
    };
    Ok((after != current).then(|| Change {
        path: file.path.clone(),
        before: current,
        after,
    }))
}
fn legacy_launcher(home: &Path, path: &Path) -> bool {
    let applications = home.join(".local/share/applications");
    path == applications.join("firefox-codex-manager.desktop")
        || path == applications.join("firefox-codex.desktop")
}
fn generated(path: PathBuf, bytes: Vec<u8>, mode: u32) -> Result<Change> {
    Ok(Change {
        before: snapshot(&path)?,
        path,
        after: Some(Snapshot { bytes, mode }),
    })
}
fn user_manifest(home: &Path) -> PathBuf {
    home.join(".mozilla/native-messaging-hosts")
        .join(format!("{}.json", crate::native::HOST))
}
fn user_wrapper(home: &Path) -> PathBuf {
    home.join(".local/lib/firefox-codex-mcp/native-host")
}
fn check_receipt_executable(home: &Path, receipt: &Receipt) -> Result<()> {
    let path = receipt.executable(home);
    ensure!(
        path == user_executable(home) || path == Path::new(package_install::EXECUTABLE),
        "The installation record has an unknown executable path; existing settings were preserved"
    );
    Ok(())
}
fn retire_user_binary(
    home: &Path,
    receipt: &Receipt,
    preserved: &mut Vec<String>,
) -> Result<Option<Change>> {
    // A receipt never grants permission to remove system package payloads.
    if receipt.delivery != "portable" || receipt.executable(home) != user_executable(home) {
        return Ok(None);
    }
    let path = user_executable(home);
    let before = snapshot(&path)?;
    if before
        .as_ref()
        .is_some_and(|s| digest(&s.bytes) == receipt.executable_hash)
    {
        Ok(Some(Change {
            path,
            before,
            after: None,
        }))
    } else {
        if before.is_some() {
            preserved.push(path.display().to_string());
        }
        Ok(None)
    }
}
fn reset_connection() -> bool {
    // /proc/self/exe also works after a portable installation removed its own
    // pathname. It runs only our existing reset command, never a shell.
    let executable = if cfg!(target_os = "linux") {
        Ok(PathBuf::from("/proc/self/exe"))
    } else {
        std::env::current_exe()
    };
    executable.is_ok_and(|executable| {
        Command::new(executable)
            .arg("--extension-reset")
            .output()
            .is_ok_and(|output| output.status.success())
    })
}
fn strip_marked(value: &str, start: &str, end: &str) -> Result<String> {
    match (value.find(start), value.find(end)) {
        (None, None) => Ok(value.into()),
        (Some(a), Some(b))
            if b > a && value.matches(start).count() == 1 && value.matches(end).count() == 1 =>
        {
            Ok(format!("{}{}", &value[..a], &value[b + end.len()..]))
        }
        _ => bail!("Managed configuration markers are incomplete. Existing content was preserved."),
    }
}
// Edit only this connection. Asking the Codex CLI to reserialize the complete
// config removes unrelated settings such as explicitly empty argument arrays.
fn config_document(before: &Option<Snapshot>) -> Result<DocumentMut> {
    text(before)?
        .parse::<DocumentMut>()
        .context("Cannot parse Codex config.toml; existing settings were preserved")
}
fn registration(document: &DocumentMut) -> Option<&Item> {
    document.get("mcp_servers")?.get("firefox")
}
fn registration_owned(layout: &Layout, prior: Option<&Receipt>) -> Result<bool> {
    let document = config_document(&snapshot(&layout.root.join("config.toml"))?)?;
    let Some(server) = registration(&document) else {
        return Ok(false);
    };
    let command = server.get("command").and_then(Item::as_str);
    let current = command == layout.executable.to_str();
    let previous =
        prior.is_some_and(|receipt| command == receipt.executable(&layout.home).to_str());
    let args = server.get("args").and_then(Item::as_array);
    Ok((current || previous)
        && args.is_some_and(|args| {
            args.len() == 1
                && args
                    .get(0)
                    .and_then(toml_edit::Value::as_str)
                    .is_some_and(|arg| arg == "--bidi" || arg == "--extension")
        }))
}
fn prepare_config(
    layout: &Layout,
    profile: &Path,
    extension: bool,
    remove: bool,
) -> Result<Change> {
    let path = layout.root.join("config.toml");
    let before = snapshot(&path)?;
    let mut document = config_document(&before)?;
    if remove {
        if let Some(servers) = document.get_mut("mcp_servers") {
            servers
                .as_table_like_mut()
                .context("Invalid mcp_servers table; existing settings were preserved")?
                .remove("firefox");
        }
    } else {
        if !document.contains_key("mcp_servers") {
            document.insert("mcp_servers", Item::Table(Table::new()));
        }
        let servers = document["mcp_servers"]
            .as_table_like_mut()
            .context("Invalid mcp_servers table; existing settings were preserved")?;
        if !servers.contains_key("firefox") {
            servers.insert("firefox", Item::Table(Table::new()));
        }
        let server = servers
            .get_mut("firefox")
            .and_then(Item::as_table_like_mut)
            .context("Invalid Firefox MCP table; existing settings were preserved")?;
        server.insert(
            "command",
            value(
                layout
                    .executable
                    .to_str()
                    .context("Installation path must be UTF-8")?,
            ),
        );
        let mut args = Array::new();
        args.push(if extension { "--extension" } else { "--bidi" });
        server.insert("args", value(args));
        if server.get("enabled").and_then(Item::as_bool) == Some(false) {
            server.insert("enabled", value(true));
        }
        if extension {
            if let Some(env) = server.get_mut("env").and_then(Item::as_table_like_mut) {
                env.remove("FIREFOX_MCP_PROFILE");
            }
        } else {
            if !server.contains_key("env") {
                server.insert("env", Item::Table(Table::new()));
            }
            server
                .get_mut("env")
                .and_then(Item::as_table_like_mut)
                .context("Invalid Firefox MCP environment; existing settings were preserved")?
                .insert(
                    "FIREFOX_MCP_PROFILE",
                    value(profile.to_str().context("Profile path must be UTF-8")?),
                );
        }
    }
    let after = Some(Snapshot {
        bytes: document.to_string().into_bytes(),
        mode: before.as_ref().map_or(0o600, |s| s.mode),
    });
    Ok(Change {
        path,
        before,
        after,
    })
}
pub fn install(explicit: Option<&str>, update_only: bool) -> Result<Value> {
    let layout = Layout::open()?;
    recover(&layout)?;
    let prior = layout.receipt()?;
    ensure!(
        !update_only || prior.is_some(),
        "The connection is not installed. Choose Install connection first."
    );
    let extension =
        explicit.is_none() && !(update_only && prior.as_ref().is_some_and(|p| p.mode == "bidi"));
    let profile = if extension {
        PathBuf::new()
    } else {
        match (explicit, prior.as_ref()) {
            (Some(value), _) => setup::profile(value)?,
            (None, Some(receipt)) if receipt.mode == "bidi" => {
                setup::profile(&receipt.profile.to_string_lossy())?
            }
            _ => o::choose(&o::profiles(&layout.home)?)?,
        }
    };
    if let Some(prior) = &prior {
        check_receipt_executable(&layout.home, prior)?;
        ensure!(
            prior.codex_home == layout.root,
            "Use the CODEX_HOME from the original installation."
        );
        ensure!(
            extension || prior.mode != "bidi" || prior.profile == profile,
            "Remove the existing installation before changing the BiDi profile."
        );
    }
    fs::create_dir_all(&layout.root)?;
    if registration(&config_document(&snapshot(
        &layout.root.join("config.toml"),
    )?)?)
    .is_some()
    {
        ensure!(
            registration_owned(&layout, prior.as_ref())?,
            "Another Firefox MCP configuration was preserved. Review the existing connection first."
        );
    }
    if layout.package.is_none() {
        setup::private_dir(
            layout
                .executable
                .parent()
                .context("Missing install directory")?,
        )?;
    }
    let mut changes = vec![prepare_config(&layout, &profile, extension, false)?];
    let mut files = Vec::new();
    let mut preserved = Vec::new();
    let agents = layout.root.join("AGENTS.md");
    let current_agents = snapshot(&agents)?;
    let baseline = if prior.is_none() {
        strip_marked(&text(&current_agents)?, o::START, o::END)?
    } else {
        text(&current_agents)?
    };
    let agents_bytes = o::guidance(&baseline)?.into_bytes();
    let mut entries = vec![
        (
            agents,
            agents_bytes,
            Some(format!("{}\n{}{}", o::START, o::GUIDANCE, o::END)),
        ),
        (
            layout.root.join("skills/firefox-browser/SKILL.md"),
            o::SKILL.as_bytes().to_vec(),
            None,
        ),
    ];
    let mut has_existing_pref = false;
    if extension && layout.package.is_some() {
        // Firefox checks per-user registrations before system registrations.
        // Only remove a receipt-owned, unchanged registration with no previous
        // foreign baseline; otherwise report the conflict before committing.
        let manifest = user_manifest(&layout.home);
        if let Some(current) = snapshot(&manifest)? {
            let old = prior
                .as_ref()
                .and_then(|prior| prior.files.iter().find(|file| file.path == manifest));
            ensure!(
                old.is_some_and(|old| old.installed == current && old.before.is_none()),
                "A custom Native Messaging registration masks the system package and was preserved: {}. Review or move it before connecting.",
                manifest.display()
            );
        }
        ensure!(
            layout.package.as_ref().is_some_and(|package| package.native_host_registered()),
            "The package Native Messaging registration is missing or invalid. Reinstall the RPM or DEB package."
        );
    } else if extension {
        let wrapper = layout
            .executable
            .parent()
            .context("Missing directory")?
            .join("native-host");
        let quoted = format!(
            "'{}'",
            layout.executable.to_string_lossy().replace('\'', "'\\''")
        );
        entries.push((
            wrapper.clone(),
            format!("#!/bin/sh\nexec {quoted} --native-host \"$@\"\n").into_bytes(),
            None,
        ));
        let manifest = user_manifest(&layout.home);
        let bytes = serde_json::to_vec_pretty(
            &json!({"name":crate::native::HOST,"description":"Firefox Codex local connection","path":wrapper,"type":"stdio","allowed_extensions":[crate::native::EXTENSION_ID]}),
        )?;
        if snapshot(&manifest)?.is_some()
            && !prior
                .as_ref()
                .is_some_and(|p| p.files.iter().any(|f| f.path == manifest))
        {
            anyhow::bail!(
                "Existing Native Messaging registration was preserved: {}",
                manifest.display()
            );
        }
        entries.push((manifest, bytes, None));
    } else {
        let pref = profile.join("user.js");
        let old_pref = text(&snapshot(&pref)?)?;
        has_existing_pref = prior.is_none()
            && old_pref
                .lines()
                .any(|l| l.trim() == "user_pref(\"remote.prefs.recommended\", false);");
        let clean = strip_marked(&old_pref, PREF_START, PREF_END)?;
        let pref_text = if has_existing_pref {
            old_pref
        } else {
            format!(
                "{}{PREF_BLOCK}",
                if clean.is_empty() {
                    String::new()
                } else {
                    format!("{}\n", clean.trim_end())
                }
            )
        };
        entries.push((
            pref,
            pref_text.into_bytes(),
            if has_existing_pref {
                None
            } else {
                Some(PREF_BLOCK.into())
            },
        ));
    }
    for (path, bytes, block) in entries {
        let before = snapshot(&path)?;
        let previous = prior
            .as_ref()
            .and_then(|p| p.files.iter().find(|f| f.path == path));
        if let Some(previous) = previous {
            if block.is_none() && before.as_ref() != Some(&previous.installed) {
                preserved.push(path.display().to_string());
                files.push(previous.clone());
                continue;
            }
            if let Some(expected) = &previous.block {
                ensure!(
                    text(&before)?.contains(expected),
                    "A managed section was changed. Its content was preserved: {}",
                    path.display()
                );
            }
        } else if path.ends_with("SKILL.md") && before.is_some() {
            ensure!(
                text(&before)? == o::SKILL,
                "The existing firefox-browser skill was preserved"
            );
        }
        if has_existing_pref && path.ends_with("user.js") {
            continue;
        }
        let installed = Snapshot {
            bytes,
            mode: if path.ends_with("native-host") {
                0o700
            } else {
                before.as_ref().map_or(0o600, |s| s.mode)
            },
        };
        let mut original = previous
            .map(|p| p.before.clone())
            .unwrap_or_else(|| before.clone());
        if let Some(previous) = previous {
            if before.as_ref() != Some(&previous.installed) {
                if let Some(block) = &previous.block {
                    // Carry user edits outside our block forward into the removal
                    // baseline; an update must not make those edits our property.
                    let clean = text(&before)?.replacen(block, "", 1);
                    original = Some(Snapshot {
                        bytes: clean.into_bytes(),
                        mode: before.as_ref().map_or(0o600, |s| s.mode),
                    });
                }
            }
        }
        // Adopt only recognizable files from the pre-receipt development installer.
        // Otherwise removal would restore the old integration instead of removing it.
        if prior.is_none() && registration_owned(&layout, prior.as_ref())? {
            if path.ends_with("AGENTS.md") && text(&before)?.contains(o::START) {
                let clean = strip_marked(&text(&before)?, o::START, o::END)?;
                original = (!clean.trim().is_empty()).then(|| Snapshot {
                    bytes: clean.into_bytes(),
                    mode: before.as_ref().map_or(0o600, |s| s.mode),
                });
            } else if path.ends_with("SKILL.md") && text(&before)? == o::SKILL {
                original = None;
            }
        }
        files.push(Managed {
            path: path.clone(),
            before: original,
            installed: installed.clone(),
            block,
        });
        changes.push(Change {
            path,
            before,
            after: Some(installed),
        });
    }
    // Retire old app-menu entries, and undo only our known debugging preference
    // when switching to extension mode. Keep everything in the same transaction
    // as configuration and receipt changes; custom edits remain the user's files.
    if let Some(prior) = &prior {
        for old in &prior.files {
            if !files.iter().any(|f| f.path == old.path) {
                if legacy_launcher(&layout.home, &old.path)
                    || layout.package.is_some()
                        && (old.path == user_manifest(&layout.home)
                            || old.path == user_wrapper(&layout.home))
                    || extension
                        && old.path.ends_with("user.js")
                        && old.block.as_deref() == Some(PREF_BLOCK)
                {
                    if let Some(change) = restore_managed_file(old, &mut preserved)? {
                        changes.push(change);
                    }
                } else {
                    files.push(old.clone());
                }
            }
        }
    }
    let (version, executable_hash, delivery) = if let Some(package) = &layout.package {
        package.validate()?;
        if let Some(prior) = &prior {
            if let Some(change) = retire_user_binary(&layout.home, prior, &mut preserved)? {
                changes.push(change);
            }
        }
        (
            package.version().to_owned(),
            package.executable_hash().to_owned(),
            "package",
        )
    } else {
        let bytes = fs::read(std::env::current_exe()?)?;
        let hash = digest(&bytes);
        changes.push(generated(layout.executable.clone(), bytes, 0o700)?);
        (env!("CARGO_PKG_VERSION").to_owned(), hash, "portable")
    };
    let receipt = Receipt {
        version,
        mode: if extension { "extension" } else { "bidi" }.into(),
        profile: profile.clone(),
        codex_home: layout.root.clone(),
        files,
        executable_hash,
        delivery: delivery.into(),
        executable: Some(layout.executable.clone()),
        // Connect/update creates a new permission lifetime independently of
        // the OS package lifetime. A fast Disconnect/Reconnect must never let
        // old MCP or BiDi processes regain their previous browser authority.
        connection_generation: layout
            .package
            .as_ref()
            .map(|_| crate::random_hex(32))
            .transpose()?,
    };
    changes.push(generated(
        layout.receipt.clone(),
        serde_json::to_vec_pretty(&receipt)?,
        0o600,
    )?);
    transact(&layout, changes)?;
    let restart_required = extension && prior.is_some() && !reset_connection();
    Ok(
        json!({"restartRequired":restart_required,"installed":true,"version":receipt.version,"delivery":receipt.delivery,"packageManaged":layout.package.is_some(),"mode":receipt.mode,"profile":profile,"preservedModifiedFiles":preserved,"nextStep":if extension && layout.package.is_some() {"Connected to Codex. Add Codex Work Tabs to Firefox, then request a browser task in a new Codex chat using ordinary Firefox. Update or remove the helper with your system package manager. Disconnect in extension Settings to remove only this user's Codex connection. Firefox profiles and logins are preserved."} else if extension {"Connection installed. Add Codex Work Tabs to Firefox, then use ordinary Firefox and request a browser task in a new Codex chat. No separate Firefox launcher, code, token or profile path is needed. Updates revoke previous work-tab access and keep Firefox running. Remove the extension separately in Firefox Add-ons."} else {"Advanced debugging connection installed. No app-menu launcher was created. If ordinary Firefox is running, save your work and close it before using --launch-firefox PROFILE. Use --doctor or --uninstall for diagnosis or removal."}}),
    )
}
pub fn uninstall() -> Result<Value> {
    let layout = Layout::open()?;
    recover(&layout)?;
    let Some(receipt) = layout.receipt()? else {
        return Ok(
            json!({"removed":false,"nextStep":"No installation record was found. For an older development version, install a current package once before removing it."}),
        );
    };
    ensure!(
        receipt.codex_home == layout.root,
        "Use the CODEX_HOME from the original installation."
    );
    check_receipt_executable(&layout.home, &receipt)?;
    let mut changes = Vec::new();
    let mut preserved = Vec::new();
    if registration_owned(&layout, Some(&receipt))? {
        changes.push(prepare_config(
            &layout,
            &receipt.profile,
            receipt.mode == "extension",
            true,
        )?);
    } else {
        preserved.push("Modified or missing Firefox MCP registration".into());
    }
    for file in &receipt.files {
        if let Some(change) = restore_managed_file(file, &mut preserved)? {
            changes.push(change);
        }
    }
    if let Some(change) = retire_user_binary(&layout.home, &receipt, &mut preserved)? {
        changes.push(change);
    }
    changes.push(Change {
        path: layout.receipt.clone(),
        before: snapshot(&layout.receipt)?,
        after: None,
    });
    transact(&layout, changes)?;
    let restart_required = receipt.mode == "extension" && !reset_connection();
    Ok(
        json!({"removed":true,"restartRequired":restart_required,"packageManaged":layout.package.is_some(),"preservedModifiedFiles":preserved,"nextStep":if layout.package.is_some() {"Disconnected this user from Codex. The system helper remains installed; remove it with your package manager if desired. Firefox profiles, logins and tabs were preserved. Remove the extension separately in Firefox Add-ons."} else {"Connection removed. Firefox profiles, logins and result tabs were preserved. Close existing Codex chats to end their running tool connections. Firefox launch options and in-memory preferences change on its next normal restart."}}),
    )
}
pub fn repair() -> Result<Value> {
    let layout = Layout::open()?;
    recover(&layout)?;
    Ok(json!({"repaired":true}))
}
pub fn installed_version() -> Option<String> {
    let home = o::home().ok()?;
    let data = fs::read(home.join(".local/state/firefox-codex-mcp/install.json")).ok()?;
    let receipt = serde_json::from_slice::<Receipt>(&data).ok()?;
    if receipt.delivery == "package" {
        package_install::installed()
            .ok()
            .flatten()
            .map(|installation| installation.version().to_owned())
    } else {
        Some(receipt.version)
    }
}
pub fn installed_delivery() -> Option<String> {
    let home = o::home().ok()?;
    let data = fs::read(home.join(".local/state/firefox-codex-mcp/install.json")).ok()?;
    serde_json::from_slice::<Receipt>(&data)
        .ok()
        .map(|r| r.delivery)
}
/// A process binds its first chat to this generation and must require the same
/// value before each later dispatch. A receipt recreated by Reconnect has a new
/// identity even if the process missed the interval without a receipt.
pub fn connection_identity() -> Result<Option<String>> {
    connection_identity_at(&o::home()?)
}
fn connection_identity_at(home: &Path) -> Result<Option<String>> {
    let Some(data) = snapshot(&home.join(".local/state/firefox-codex-mcp/install.json"))? else {
        return Ok(None);
    };
    let receipt: Receipt = serde_json::from_slice(&data.bytes)?;
    check_receipt_executable(home, &receipt)?;
    if receipt.delivery != "package"
        || receipt.executable(home) != Path::new(package_install::EXECUTABLE)
        || !receipt.codex_home.is_absolute()
    {
        return Ok(None);
    }
    let Some(generation) = receipt.connection_generation.as_ref().filter(|generation| {
        generation.len() == 64 && generation.bytes().all(|byte| byte.is_ascii_hexdigit())
    }) else {
        return Ok(None);
    };
    let expected_argument = match receipt.mode.as_str() {
        "extension" => "--extension",
        "bidi" => "--bidi",
        _ => return Ok(None),
    };
    let document = config_document(&snapshot(&receipt.codex_home.join("config.toml"))?)?;
    let Some(server) = registration(&document) else {
        return Ok(None);
    };
    let registered = server.get("command").and_then(Item::as_str)
        == Some(package_install::EXECUTABLE)
        && server
            .get("enabled")
            .is_none_or(|enabled| enabled.as_bool() == Some(true))
        && server
            .get("args")
            .and_then(Item::as_array)
            .is_some_and(|args| {
                args.len() == 1
                    && args.get(0).and_then(toml_edit::Value::as_str) == Some(expected_argument)
            });
    Ok(registered.then(|| generation.clone()))
}
pub fn installed_mode() -> Option<String> {
    let data = fs::read(
        o::home()
            .ok()?
            .join(".local/state/firefox-codex-mcp/install.json"),
    )
    .ok()?;
    serde_json::from_slice::<Receipt>(&data)
        .ok()
        .map(|r| r.mode)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn contents(bytes: &str, mode: u32) -> Snapshot {
        Snapshot {
            bytes: bytes.as_bytes().to_vec(),
            mode,
        }
    }
    fn receipt(home: &Path, delivery: &str) -> Receipt {
        Receipt {
            version: "test".into(),
            mode: "extension".into(),
            profile: PathBuf::new(),
            codex_home: home.into(),
            files: Vec::new(),
            executable_hash: digest(b"payload"),
            delivery: delivery.into(),
            executable: Some(if delivery == "package" {
                PathBuf::from(package_install::EXECUTABLE)
            } else {
                user_executable(home)
            }),
            connection_generation: (delivery == "package").then(|| "a".repeat(64)),
        }
    }
    fn layout(root: &Path) -> Layout {
        Layout {
            home: root.into(),
            root: root.into(),
            executable: root.join("binary"),
            package: None,
            receipt: root.join("install.json"),
            pending: root.join("transaction.json"),
            _lock: File::create(root.join("lock")).unwrap(),
        }
    }
    #[test]
    fn targeted_config_edit_preserves_unrelated_empty_arrays_and_comments() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let layout = layout(temp.path());
        let original = "# Keep my choices\nmodel = \"example\"\n\n[mcp_servers.node_repl]\ncommand = \"/my/node/tool\"\nargs = [] # explicitly empty\n\n[profiles.personal]\nmodel_reasoning_effort = \"high\"\n";
        setup::write_atomic(&layout.root.join("config.toml"), original.as_bytes(), 0o640)?;
        let change = prepare_config(&layout, Path::new(""), true, false)?;
        let after = change.after.as_ref().context("prepared config")?;
        assert_eq!(after.mode, 0o640);
        let after_text = String::from_utf8_lossy(&after.bytes);
        assert!(after_text.starts_with("# Keep my choices\nmodel = \"example\"\n"));
        assert!(after_text.contains(
            "[mcp_servers.node_repl]\ncommand = \"/my/node/tool\"\nargs = [] # explicitly empty\n"
        ));
        assert!(after_text.contains("[profiles.personal]\nmodel_reasoning_effort = \"high\"\n"));
        put(&change.path, &change.after)?;
        assert!(registration_owned(&layout, None)?);
        let removed = prepare_config(&layout, Path::new(""), true, true)?;
        let removed_text = text(&removed.after)?;
        assert_eq!(removed_text, original);
        assert!(!removed_text.contains("mcp_servers.firefox"));
        Ok(())
    }
    #[test]
    fn registration_recognizes_receipt_owned_migration_and_preserves_foreign_entry() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let mut layout = layout(temp.path());
        let old = receipt(temp.path(), "portable");
        layout.executable = old.executable(temp.path());
        let change = prepare_config(&layout, Path::new(""), true, false)?;
        put(&change.path, &change.after)?;
        layout.executable = PathBuf::from(package_install::EXECUTABLE);
        assert!(!registration_owned(&layout, None)?);
        assert!(registration_owned(&layout, Some(&old))?);
        let foreign =
            "[mcp_servers.firefox]\ncommand = \"/other/tool\"\nargs = [\"--extension\"]\n";
        fs::write(layout.root.join("config.toml"), foreign)?;
        assert!(!registration_owned(&layout, Some(&old))?);
        assert_eq!(
            fs::read_to_string(layout.root.join("config.toml"))?,
            foreign
        );
        Ok(())
    }
    #[test]
    fn removal_never_touches_package_payload_or_modified_portable_binary() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let package = receipt(temp.path(), "package");
        let mut preserved = Vec::new();
        assert!(retire_user_binary(temp.path(), &package, &mut preserved)?.is_none());
        assert!(preserved.is_empty());
        let portable = receipt(temp.path(), "portable");
        let path = user_executable(temp.path());
        fs::create_dir_all(path.parent().context("binary directory")?)?;
        fs::write(&path, b"user modified payload")?;
        assert!(retire_user_binary(temp.path(), &portable, &mut preserved)?.is_none());
        assert_eq!(fs::read(&path)?, b"user modified payload");
        assert_eq!(preserved, [path.display().to_string()]);
        fs::write(&path, b"payload")?;
        let change = retire_user_binary(temp.path(), &portable, &mut preserved)?
            .context("retired binary")?;
        assert_eq!(change.path, path);
        assert!(change.after.is_none());
        Ok(())
    }
    #[test]
    fn legacy_receipt_defaults_to_user_managed_binary() -> Result<()> {
        let receipt: Receipt = serde_json::from_value(json!({
            "version":"0.4.2","mode":"extension","profile":"",
            "codex_home":"/example/.codex","files":[],"executable_hash":"hash"
        }))?;
        assert_eq!(receipt.delivery, "portable");
        assert!(receipt.connection_generation.is_none());
        assert_eq!(
            receipt.executable(Path::new("/example")),
            user_executable(Path::new("/example"))
        );
        Ok(())
    }
    #[test]
    fn firefox_settings_reuses_receipt_codex_home_without_overriding_explicit_environment(
    ) -> Result<()> {
        let temp = tempfile::tempdir()?;
        let custom = temp.path().join("custom-codex");
        fs::create_dir(&custom)?;
        let mut receipt = receipt(temp.path(), "package");
        receipt.codex_home = custom.clone();
        let path = temp
            .path()
            .join(".local/state/firefox-codex-mcp/install.json");
        fs::create_dir_all(path.parent().context("receipt directory")?)?;
        setup::write_atomic(&path, &serde_json::to_vec(&receipt)?, 0o600)?;
        assert_eq!(
            connection_codex_home_with_override(temp.path(), None)?,
            custom
        );
        let explicit = temp.path().join("explicit-codex");
        assert_eq!(
            connection_codex_home_with_override(
                temp.path(),
                Some(explicit.clone().into_os_string())
            )?,
            explicit
        );
        let link = temp.path().join("linked-codex");
        std::os::unix::fs::symlink(&custom, &link)?;
        assert!(
            connection_codex_home_with_override(temp.path(), Some(link.into_os_string())).is_err()
        );
        receipt.codex_home = PathBuf::from("relative-codex");
        setup::write_atomic(&path, &serde_json::to_vec(&receipt)?, 0o600)?;
        assert!(connection_codex_home_with_override(temp.path(), None).is_err());
        Ok(())
    }
    #[test]
    fn disconnect_and_disabled_registration_revoke_packaged_connections() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let mut layout = layout(temp.path());
        layout.executable = PathBuf::from(package_install::EXECUTABLE);
        let receipt = receipt(temp.path(), "package");
        let receipt_path = temp
            .path()
            .join(".local/state/firefox-codex-mcp/install.json");
        fs::create_dir_all(receipt_path.parent().context("receipt directory")?)?;
        setup::write_atomic(&receipt_path, &serde_json::to_vec(&receipt)?, 0o600)?;
        let change = prepare_config(&layout, Path::new(""), true, false)?;
        put(&change.path, &change.after)?;
        assert!(connection_identity_at(temp.path())?.is_some());
        let mut document = config_document(&snapshot(&change.path)?)?;
        document["mcp_servers"]["firefox"]["enabled"] = value(false);
        fs::write(&change.path, document.to_string())?;
        assert!(connection_identity_at(temp.path())?.is_none());
        put(&change.path, &change.after)?;
        fs::remove_file(&receipt_path)?;
        assert!(connection_identity_at(temp.path())?.is_none());
        let mut bidi = receipt;
        bidi.mode = "bidi".into();
        setup::write_atomic(&receipt_path, &serde_json::to_vec(&bidi)?, 0o600)?;
        assert!(connection_identity_at(temp.path())?.is_none());
        let change = prepare_config(&layout, Path::new("/example/profile"), false, false)?;
        put(&change.path, &change.after)?;
        assert!(connection_identity_at(temp.path())?.is_some());
        Ok(())
    }
    #[test]
    fn reconnect_changes_permission_identity_even_when_receipt_gap_is_not_observed() -> Result<()> {
        for extension in [true, false] {
            let temp = tempfile::tempdir()?;
            let mut layout = layout(temp.path());
            layout.executable = PathBuf::from(package_install::EXECUTABLE);
            let mut receipt = receipt(temp.path(), "package");
            receipt.mode = if extension { "extension" } else { "bidi" }.into();
            let path = temp
                .path()
                .join(".local/state/firefox-codex-mcp/install.json");
            fs::create_dir_all(path.parent().context("receipt directory")?)?;
            setup::write_atomic(&path, &serde_json::to_vec(&receipt)?, 0o600)?;
            let config = prepare_config(&layout, Path::new("/example/profile"), extension, false)?;
            put(&config.path, &config.after)?;
            let original = connection_identity_at(temp.path())?.context("first connection")?;

            // No intermediate read observes Disconnect. A presence-only check
            // would allow the old process to regain authority after Reconnect.
            fs::remove_file(&path)?;
            receipt.connection_generation = Some(crate::random_hex(32)?);
            setup::write_atomic(&path, &serde_json::to_vec(&receipt)?, 0o600)?;
            let reconnected = connection_identity_at(temp.path())?.context("reconnected")?;
            assert_ne!(original, reconnected);

            receipt.connection_generation = None;
            setup::write_atomic(&path, &serde_json::to_vec(&receipt)?, 0o600)?;
            assert!(connection_identity_at(temp.path())?.is_none());
            receipt.connection_generation = Some("invalid generation".into());
            setup::write_atomic(&path, &serde_json::to_vec(&receipt)?, 0o600)?;
            assert!(connection_identity_at(temp.path())?.is_none());
        }
        Ok(())
    }
    #[test]
    fn interrupted_install_recovers_and_preserves_concurrent_edits() {
        let temp = tempfile::tempdir().unwrap();
        let layout = layout(temp.path());
        let first = temp.path().join("config");
        let second = temp.path().join("skill");
        fs::write(&first, "original").unwrap();
        let changes = vec![
            generated(first.clone(), b"new".to_vec(), 0o600).unwrap(),
            generated(second.clone(), b"skill".to_vec(), 0o600).unwrap(),
        ];
        setup::write_atomic(
            &layout.pending,
            &serde_json::to_vec(&changes).unwrap(),
            0o600,
        )
        .unwrap();
        put(&first, &changes[0].after).unwrap();
        // Simulate power loss after the first file, before the second file.
        recover(&layout).unwrap();
        assert_eq!(fs::read_to_string(&first).unwrap(), "original");
        assert!(!second.exists());
        assert!(!layout.pending.exists());
        setup::write_atomic(
            &layout.pending,
            &serde_json::to_vec(&changes).unwrap(),
            0o600,
        )
        .unwrap();
        fs::write(&first, "user edit after crash").unwrap();
        assert!(recover(&layout).is_err());
        assert_eq!(fs::read_to_string(&first).unwrap(), "user edit after crash");
        assert!(layout.pending.exists());
    }
    #[test]
    fn rejects_symlink_targets() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("real");
        let link = temp.path().join("config");
        fs::write(&target, "untouched").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(generated(link, b"change".to_vec(), 0o600).is_err());
        assert_eq!(fs::read_to_string(target).unwrap(), "untouched");
    }
    #[test]
    fn shortcut_retirement_rolls_back_with_the_receipt() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let layout = layout(temp.path());
        let path = temp.path().join("firefox-codex.desktop");
        let installed = contents("Managed shortcut\n", 0o600);
        let original = contents("Original shortcut\n", 0o644);
        put(&path, &Some(installed.clone()))?;
        let managed = Managed {
            path: path.clone(),
            before: Some(original.clone()),
            installed: installed.clone(),
            block: None,
        };
        let mut preserved = Vec::new();
        let changes = vec![
            restore_managed_file(&managed, &mut preserved)?.context("retirement")?,
            generated(layout.receipt.clone(), b"new receipt".to_vec(), 0o600)?,
        ];
        setup::write_atomic(&layout.pending, &serde_json::to_vec(&changes)?, 0o600)?;
        put(&path, &changes[0].after)?;
        // A crash before committing the new receipt must restore the old pair.
        recover(&layout)?;
        assert!(snapshot(&path)? == Some(installed));
        assert!(!layout.receipt.exists());
        transact(&layout, changes)?;
        assert!(snapshot(&path)? == Some(original));
        assert!(preserved.is_empty());
        Ok(())
    }
    #[test]
    fn managed_removal_preserves_deleted_and_customized_files() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("firefox-codex-manager.desktop");
        let managed = Managed {
            path: path.clone(),
            before: Some(contents("Original shortcut\n", 0o600)),
            installed: contents("Managed shortcut\n", 0o600),
            block: None,
        };
        let mut preserved = Vec::new();
        // Deleting an installed shortcut must not resurrect an older original.
        assert!(restore_managed_file(&managed, &mut preserved)?.is_none());
        fs::write(&path, "My edited shortcut\n")?;
        assert!(restore_managed_file(&managed, &mut preserved)?.is_none());
        assert_eq!(fs::read_to_string(&path)?, "My edited shortcut\n");
        assert_eq!(preserved, [path.display().to_string()]);
        Ok(())
    }
    #[test]
    fn preference_retirement_preserves_edits_outside_the_known_block() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("user.js");
        let baseline = "user_pref(\"example.original\", true);\n";
        let installed = contents(&format!("{baseline}{PREF_BLOCK}"), 0o600);
        let managed = Managed {
            path: path.clone(),
            before: Some(contents(baseline, 0o600)),
            installed,
            block: Some(PREF_BLOCK.into()),
        };
        let edited = format!("{baseline}{PREF_BLOCK}user_pref(\"example.later\", true);\n");
        setup::write_atomic(&path, edited.as_bytes(), 0o640)?;
        let mut preserved = Vec::new();
        let change =
            restore_managed_file(&managed, &mut preserved)?.context("preference removal")?;
        put(&path, &change.after)?;
        assert_eq!(
            fs::read_to_string(&path)?,
            format!("{baseline}user_pref(\"example.later\", true);\n")
        );
        assert_eq!(snapshot(&path)?.context("preferences")?.mode, 0o640);
        let user_block = edited.replace("recommended\", false", "recommended\", true");
        fs::write(&path, &user_block)?;
        assert!(restore_managed_file(&managed, &mut preserved)?.is_none());
        assert_eq!(fs::read_to_string(&path)?, user_block);
        Ok(())
    }
}
