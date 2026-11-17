//! Transactional, user-level install/update/remove. Never stops user applications.
use crate::{onboarding as o, setup};
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
}

fn legacy_mode() -> String {
    "bidi".into()
}

struct Layout {
    home: PathBuf,
    root: PathBuf,
    directory: PathBuf,
    executable: PathBuf,
    receipt: PathBuf,
    pending: PathBuf,
    _lock: File,
}
impl Layout {
    fn open() -> Result<Self> {
        let home = o::home()?;
        ensure!(home.is_absolute(), "HOME must be absolute");
        let root = o::codex_home(&home);
        ensure!(root.is_absolute(), "CODEX_HOME must be absolute");
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
            executable: home.join(".local/lib/firefox-codex-mcp/firefox-codex-mcp"),
            receipt: directory.join("install.json"),
            pending: directory.join("transaction.json"),
            home,
            root,
            directory,
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
// Run the official CLI against a staging config. The live config changes only
// together with the rest of the transaction, after a compare-before-write check.
fn prepare_config(
    layout: &Layout,
    profile: &Path,
    extension: bool,
    remove: bool,
) -> Result<Change> {
    let path = layout.root.join("config.toml");
    let before = snapshot(&path)?;
    let stage = layout
        .directory
        .join(format!("stage-{}", crate::random_hex(8)?));
    setup::private_dir(&stage)?;
    let result = (|| {
        if let Some(value) = &before {
            setup::write_atomic(&stage.join("config.toml"), &value.bytes, 0o600)?;
        }
        let binary = o::codex()?;
        let mut command = Command::new(binary);
        command
            .current_dir(&stage)
            .env("CODEX_HOME", &stage)
            .args(["mcp"]);
        if remove {
            command.args(["remove", "firefox"]);
        } else if extension {
            command
                .args(["add", "firefox", "--"])
                .arg(&layout.executable)
                .arg("--extension");
        } else {
            command
                .args(["add", "firefox", "--env"])
                .arg(format!("FIREFOX_MCP_PROFILE={}", profile.display()))
                .arg("--")
                .arg(&layout.executable)
                .arg("--bidi");
        }
        let output = command.output()?;
        ensure!(
            output.status.success(),
            "Could not prepare Codex settings: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut after = snapshot(&stage.join("config.toml"))?;
        if let (Some(old), Some(new)) = (&before, &mut after) {
            new.mode = old.mode;
        }
        Ok(Change {
            path,
            before,
            after,
        })
    })();
    let _ = fs::remove_dir_all(stage);
    result
}
fn owned_registration(layout: &Layout) -> Result<bool> {
    if !layout.root.exists() {
        return Ok(false);
    }
    Ok(o::configured(&o::codex()?, &layout.root)?.is_some_and(|v| {
        v["transport"]["command"].as_str() == layout.executable.to_str()
            && (v["transport"]["args"] == json!(["--bidi"])
                || v["transport"]["args"] == json!(["--extension"]))
    }))
}
pub fn install(explicit: Option<&str>, update_only: bool) -> Result<Value> {
    let layout = Layout::open()?;
    recover(&layout)?;
    let prior = layout.receipt()?;
    ensure!(
        !update_only || prior.is_some() || layout.executable.exists(),
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
    if o::configured(&o::codex()?, &layout.root)?.is_some() {
        ensure!(
            owned_registration(&layout)?,
            "Another Firefox MCP configuration was preserved. Review the existing connection first."
        );
    }
    setup::private_dir(
        layout
            .executable
            .parent()
            .context("Missing install directory")?,
    )?;
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
    if extension {
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
        let manifest = layout
            .home
            .join(".mozilla/native-messaging-hosts")
            .join(format!("{}.json", crate::native::HOST));
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
        if prior.is_none() && owned_registration(&layout)? {
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
    let bytes = fs::read(std::env::current_exe()?)?;
    let receipt = Receipt {
        version: env!("CARGO_PKG_VERSION").into(),
        mode: if extension { "extension" } else { "bidi" }.into(),
        profile: profile.clone(),
        codex_home: layout.root.clone(),
        files,
        executable_hash: digest(&bytes),
    };
    changes.push(generated(layout.executable.clone(), bytes, 0o700)?);
    changes.push(generated(
        layout.receipt.clone(),
        serde_json::to_vec_pretty(&receipt)?,
        0o600,
    )?);
    transact(&layout, changes)?;
    let restart_required = extension
        && prior.is_some()
        && !Command::new(&layout.executable)
            .arg("--extension-reset")
            .output()
            .is_ok_and(|o| o.status.success());
    Ok(
        json!({"restartRequired":restart_required,"installed":true,"version":receipt.version,"mode":receipt.mode,"profile":profile,"preservedModifiedFiles":preserved,"nextStep":if extension {"Connection installed. Add Codex Work Tabs to Firefox, then use ordinary Firefox and request a browser task in a new Codex chat. No separate Firefox launcher, code, token or profile path is needed. Updates revoke previous work-tab access and keep Firefox running. Remove the extension separately in Firefox Add-ons."} else {"Advanced debugging connection installed. No app-menu launcher was created. If ordinary Firefox is running, save your work and close it before using --launch-firefox PROFILE. Use --doctor or --uninstall for diagnosis or removal."}}),
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
    let mut changes = Vec::new();
    let mut preserved = Vec::new();
    if owned_registration(&layout)? {
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
    let executable = snapshot(&layout.executable)?;
    if executable
        .as_ref()
        .is_some_and(|s| digest(&s.bytes) == receipt.executable_hash)
    {
        changes.push(Change {
            path: layout.executable.clone(),
            before: executable,
            after: None,
        });
    } else {
        preserved.push(layout.executable.display().to_string());
    }
    changes.push(Change {
        path: layout.receipt.clone(),
        before: snapshot(&layout.receipt)?,
        after: None,
    });
    transact(&layout, changes)?;
    Ok(
        json!({"removed":true,"preservedModifiedFiles":preserved,"nextStep":"Connection removed. Firefox profiles, logins and result tabs were preserved. Close existing Codex chats to end their running tool connections. Firefox launch options and in-memory preferences change on its next normal restart."}),
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
    serde_json::from_slice::<Receipt>(&data)
        .ok()
        .map(|r| r.version)
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
    fn layout(root: &Path) -> Layout {
        Layout {
            home: root.into(),
            root: root.into(),
            directory: root.into(),
            executable: root.join("binary"),
            receipt: root.join("install.json"),
            pending: root.join("transaction.json"),
            _lock: File::create(root.join("lock")).unwrap(),
        }
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
