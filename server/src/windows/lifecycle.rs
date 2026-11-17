//! Per-user Windows installer. Files and registry values commit together with
//! compare-before-write and a durable recovery journal. No Firefox profile edits.
use crate::{onboarding as o, setup, windows as w};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use winreg::{enums::*, RegKey};

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(tag = "kind")]
enum Target {
    File { path: PathBuf },
    Registry { key: String, name: String },
}
#[derive(Clone, Serialize, Deserialize)]
struct Change {
    target: Target,
    before: Option<String>,
    after: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Managed {
    target: Target,
    original: Option<String>,
    installed: String,
    block: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct Receipt {
    version: String,
    codex_home: PathBuf,
    executable: PathBuf,
    files: Vec<Managed>,
    binaries: Vec<(PathBuf, String)>,
}
#[derive(Serialize, Deserialize)]
struct Cleanup {
    files: Vec<CleanupFile>,
}
#[derive(Serialize, Deserialize)]
struct CleanupFile {
    path: PathBuf,
    sha256: String,
}
impl Cleanup {
    fn from_binaries(binaries: &[(PathBuf, String)]) -> Self {
        Self {
            files: binaries
                .iter()
                .map(|(path, hash)| CleanupFile {
                    path: path.clone(),
                    sha256: hash.clone(),
                })
                .collect(),
        }
    }
    fn binaries(self) -> Vec<(PathBuf, String)> {
        self.files.into_iter().map(|v| (v.path, v.sha256)).collect()
    }
}
struct Layout {
    root: PathBuf,
    codex: PathBuf,
    receipt: PathBuf,
    journal: PathBuf,
    _lock: File,
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn reg_base() -> String {
    std::env::var("FIREFOX_MCP_REGISTRY_ROOT").unwrap_or_else(|_| "Software".into())
}
fn native_key() -> String {
    format!(
        r"{}\Mozilla\NativeMessagingHosts\firefox_codex_mcp",
        reg_base()
    )
}
fn receipt_path() -> Result<PathBuf> {
    Ok(w::local_root()?.join("install.json"))
}
fn receipt() -> Result<Option<Receipt>> {
    crate::transport::read_private(&receipt_path()?, 4 * 1024 * 1024)?
        .map(|b| serde_json::from_slice(&b).map_err(Into::into))
        .transpose()
}
impl Layout {
    fn open() -> Result<Self> {
        let root = w::local_root()?;
        setup::private_dir(&root)?;
        let lock_path = root.join("install.lock");
        w::no_reparse(&lock_path)?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        lock.try_lock()
            .context("Another install/update/removal is running. Try again when it finishes.")?;
        let codex = o::codex_home(&o::home()?);
        ensure!(codex.is_absolute(), "CODEX_HOME must be absolute");
        w::no_reparse(&codex)?;
        Ok(Self {
            receipt: root.join("install.json"),
            journal: root.join("transaction.json"),
            root,
            codex,
            _lock: lock,
        })
    }
}
fn read(target: &Target) -> Result<Option<String>> {
    match target {
        Target::File { path } => crate::transport::read_private(path, 4 * 1024 * 1024)?
            .map(|b| String::from_utf8(b).map_err(Into::into))
            .transpose(),
        Target::Registry { key, name } => {
            let hive = RegKey::predef(HKEY_CURRENT_USER);
            let key = match hive.open_subkey_with_flags(key, KEY_READ | KEY_WOW64_64KEY) {
                Ok(k) => k,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e.into()),
            };
            match key.get_value::<String, _>(name) {
                Ok(v) => Ok(Some(v)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e).context("Preserving an incompatible registry value"),
            }
        }
    }
}
fn write(target: &Target, value: &Option<String>) -> Result<()> {
    match target {
        Target::File { path } => match value {
            Some(value) => setup::write_atomic(path, value.as_bytes(), 0o600),
            None => {
                w::no_reparse(path)?;
                if path.exists() {
                    fs::remove_file(path)?;
                }
                Ok(())
            }
        },
        Target::Registry { key, name } => {
            let hive = RegKey::predef(HKEY_CURRENT_USER);
            if let Some(value) = value {
                let (k, _) = hive.create_subkey_with_flags(key, KEY_WRITE | KEY_WOW64_64KEY)?;
                k.set_value(name, value)?;
            } else if let Ok(k) =
                hive.open_subkey_with_flags(key, KEY_READ | KEY_WRITE | KEY_WOW64_64KEY)
            {
                if let Err(e) = k.delete_value(name) {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        return Err(e.into());
                    }
                }
                let empty = k.enum_values().next().is_none() && k.enum_keys().next().is_none();
                drop(k);
                if empty {
                    hive.delete_subkey_with_flags(key, KEY_WOW64_64KEY)?;
                }
            }
            Ok(())
        }
    }
}
fn restore(changes: &[Change]) -> Result<()> {
    for c in changes.iter().rev() {
        let now = read(&c.target)?;
        if now == c.before {
            continue;
        }
        ensure!(now==c.after,"A setting changed outside this installer. Preserving it and the recovery journal: {:?}",c.target);
        write(&c.target, &c.before)?;
    }
    Ok(())
}
fn recover(layout: &Layout) -> Result<()> {
    if let Some(bytes) = crate::transport::read_private(&layout.journal, 16 * 1024 * 1024)? {
        let changes: Vec<Change> = serde_json::from_slice(&bytes)?;
        if !changes
            .iter()
            .all(|c| read(&c.target).is_ok_and(|v| v == c.after))
        {
            restore(&changes)?;
        }
        fs::remove_file(&layout.journal)?;
    }
    Ok(())
}
fn transact(layout: &Layout, changes: Vec<Change>) -> Result<()> {
    for c in &changes {
        ensure!(
            read(&c.target)? == c.before,
            "Settings changed; please retry"
        );
    }
    setup::write_atomic(&layout.journal, &serde_json::to_vec(&changes)?, 0o600)?;
    for c in &changes {
        let result = (|| {
            ensure!(
                read(&c.target)? == c.before,
                "Settings changed during installation"
            );
            write(&c.target, &c.after)
        })();
        if let Err(e) = result {
            restore(&changes).context("Run --repair to finish recovery")?;
            fs::remove_file(&layout.journal)?;
            return Err(e);
        }
    }
    fs::remove_file(&layout.journal)?;
    Ok(())
}
fn file(path: PathBuf) -> Target {
    Target::File { path }
}
fn registry(key: String, name: &str) -> Target {
    Target::Registry {
        key,
        name: name.into(),
    }
}
fn change(target: Target, after: Option<String>) -> Result<Change> {
    Ok(Change {
        before: read(&target)?,
        target,
        after,
    })
}
fn config(layout: &Layout, old: Option<&Receipt>, exe: Option<&Path>) -> Result<Change> {
    let target = file(layout.codex.join("config.toml"));
    let before = read(&target)?;
    let mut document = before
        .as_deref()
        .unwrap_or("")
        .parse::<toml_edit::DocumentMut>()?;
    if let Some(existing) = document.get("mcp_servers").and_then(|v| v.get("firefox")) {
        let ours = old.is_some_and(|r| {
            existing.get("command").and_then(|v| v.as_str()) == r.executable.to_str()
                && existing
                    .get("args")
                    .and_then(|v| v.as_array())
                    .is_some_and(|a| {
                        a.len() == 1 && a.get(0).and_then(|v| v.as_str()) == Some("--extension")
                    })
        });
        ensure!(ours,"Preserving another or modified firefox MCP entry. Review Codex settings before installing/removing.");
    }
    if let Some(exe) = exe {
        document["mcp_servers"]["firefox"]["command"] =
            toml_edit::value(exe.to_str().context("Executable path is not Unicode")?);
        let mut args = toml_edit::Array::new();
        args.push("--extension");
        document["mcp_servers"]["firefox"]["args"] = toml_edit::value(args);
        document["mcp_servers"]["firefox"]["startup_timeout_sec"] = toml_edit::value(20);
        document["mcp_servers"]["firefox"]["tool_timeout_sec"] = toml_edit::value(70);
    } else if let Some(servers) = document
        .get_mut("mcp_servers")
        .and_then(|v| v.as_table_like_mut())
    {
        servers.remove("firefox");
    }
    Ok(Change {
        target,
        before,
        after: Some(document.to_string()),
    })
}
fn manage(
    target: Target,
    value: String,
    block: Option<String>,
    prior: Option<&Receipt>,
    files: &mut Vec<Managed>,
    changes: &mut Vec<Change>,
) -> Result<()> {
    let before = read(&target)?;
    let previous = prior.and_then(|r| r.files.iter().find(|f| f.target == target));
    let mut original = previous
        .map(|p| p.original.clone())
        .unwrap_or_else(|| before.clone());
    if let Some(old) = previous {
        if before.as_ref() != Some(&old.installed) {
            if let Some(b) = &old.block {
                let text = before.as_deref().unwrap_or("");
                ensure!(
                    text.matches(b).count() == 1,
                    "Managed instructions changed; preserving them"
                );
                original = Some(text.replacen(b, "", 1));
            } else {
                bail!("Managed file or registry entry changed; preserving it: {target:?}");
            }
        }
    } else if before.is_some() && block.is_none() {
        bail!("Preserving an existing registration or file: {target:?}");
    }
    changes.push(Change {
        target: target.clone(),
        before,
        after: Some(value.clone()),
    });
    files.push(Managed {
        target,
        original,
        installed: value,
        block,
    });
    Ok(())
}
fn install_binary(path: &Path, bytes: &[u8]) -> Result<()> {
    w::no_reparse(path)?;
    if path.exists() {
        ensure!(
            digest(&fs::read(path)?) == digest(bytes),
            "Version directory contains a different binary"
        );
        return Ok(());
    }
    setup::write_atomic(path, bytes, 0o700)
}
fn reset(executable: &Path) -> bool {
    Command::new(executable)
        .arg("--extension-reset")
        .stdin(Stdio::null())
        .output()
        .is_ok_and(|v| v.status.success())
}
pub fn install(explicit: Option<&str>, update_only: bool) -> Result<Value> {
    ensure!(
        explicit.is_none(),
        "BiDi mode is Linux-only. Use the default Windows extension connection."
    );
    let layout = Layout::open()?;
    recover(&layout)?;
    let prior = receipt()?;
    ensure!(
        !update_only || prior.is_some(),
        "Install the connection before updating it"
    );
    if let Some(old) = &prior {
        ensure!(
            old.codex_home == layout.codex,
            "Use the CODEX_HOME from the original installation"
        );
    }
    let bytes = fs::read(std::env::current_exe()?)?;
    let hash = digest(&bytes);
    let version_dir =
        layout
            .root
            .join("versions")
            .join(format!("{}-{}", env!("CARGO_PKG_VERSION"), &hash[..12]));
    let exe = version_dir.join("firefox-codex-mcp.exe");
    let native = version_dir.join("firefox-codex-native-host.exe");
    // Immutable paths solve Windows' running-executable lock. Switching config is
    // the commit point; failure leaves only an inert, unregistered payload.
    setup::private_dir(&version_dir)?;
    install_binary(&exe, &bytes)?;
    install_binary(&native, &bytes)?;
    let mut changes = vec![config(&layout, prior.as_ref(), Some(&exe))?];
    let mut files = Vec::new();
    let agents = file(layout.codex.join("AGENTS.md"));
    let existing = read(&agents)?.unwrap_or_default();
    manage(
        agents,
        o::guidance(&existing)?,
        Some(format!("{}\n{}{}", o::START, o::GUIDANCE, o::END)),
        prior.as_ref(),
        &mut files,
        &mut changes,
    )?;
    manage(
        file(layout.codex.join("skills/firefox-browser/SKILL.md")),
        o::SKILL.into(),
        None,
        prior.as_ref(),
        &mut files,
        &mut changes,
    )?;
    let manifest = layout.root.join("firefox_codex_mcp.json");
    manage(
        file(manifest.clone()),
        serde_json::to_string_pretty(
            &json!({"name":crate::native::HOST,"description":"Firefox Codex local connection","path":native,"type":"stdio","allowed_extensions":[crate::native::EXTENSION_ID]}),
        )?,
        None,
        prior.as_ref(),
        &mut files,
        &mut changes,
    )?;
    // Firefox may inspect the 32-bit view before the 64-bit registration.
    if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(native_key(), KEY_READ | KEY_WOW64_32KEY)
    {
        if let Ok(value) = key.get_value::<String, _>("") {
            ensure!(
                value == manifest.to_string_lossy(),
                "A different 32-bit native host is registered; preserving it"
            );
        }
    }
    manage(
        registry(native_key(), ""),
        manifest.to_string_lossy().into_owned(),
        None,
        prior.as_ref(),
        &mut files,
        &mut changes,
    )?;
    let uninstall = format!(
        r"{}\Microsoft\Windows\CurrentVersion\Uninstall\FirefoxCodexMCP",
        reg_base()
    );
    for (name, value) in [
        ("DisplayName", "Firefox Codex MCP".to_owned()),
        ("DisplayVersion", env!("CARGO_PKG_VERSION").into()),
        ("Publisher", "nobadwordss".into()),
        (
            "InstallLocation",
            layout.root.to_string_lossy().into_owned(),
        ),
        ("UninstallString", format!("\"{}\" --manage", exe.display())),
    ] {
        manage(
            registry(uninstall.clone(), name),
            value,
            None,
            prior.as_ref(),
            &mut files,
            &mut changes,
        )?;
    }
    let mut binaries = prior
        .as_ref()
        .map(|r| r.binaries.clone())
        .unwrap_or_default();
    for path in [exe.clone(), native] {
        if !binaries.iter().any(|(p, _)| p == &path) {
            binaries.push((path, hash.clone()));
        }
    }
    let next = Receipt {
        version: env!("CARGO_PKG_VERSION").into(),
        codex_home: layout.codex.clone(),
        executable: exe.clone(),
        files,
        binaries,
    };
    changes.push(change(
        file(layout.receipt.clone()),
        Some(serde_json::to_string_pretty(&next)?),
    )?);
    transact(&layout, changes)?;
    let restart_required = prior.is_some() && !reset(&exe);
    Ok(
        json!({"installed":true,"version":next.version,"mode":"extension","restartRequired":restart_required,"nextStep":"Connection installed. Add Codex Work Tabs using step 2, then start a new Codex chat. No code, token, profile path or Firefox restart is needed. An update releases previous work-tab access. Manage or remove this connection in Windows Settings → Apps → Firefox Codex MCP."}),
    )
}
pub fn uninstall() -> Result<Value> {
    let layout = Layout::open()?;
    recover(&layout)?;
    let Some(prior) = receipt()? else {
        return Ok(json!({"removed":false,"nextStep":"The connection is not installed."}));
    };
    ensure!(
        prior.codex_home == layout.codex,
        "Use the CODEX_HOME from installation"
    );
    let mut changes = Vec::new();
    let mut preserved = Vec::<String>::new();
    match config(&layout, Some(&prior), None) {
        Ok(c) => changes.push(c),
        Err(_) => preserved.push("Modified firefox MCP entry".into()),
    };
    for managed in &prior.files {
        let before = read(&managed.target)?;
        let after = if before.as_ref() == Some(&managed.installed) {
            managed.original.clone()
        } else if let Some(block) = &managed.block {
            let text = before.as_deref().unwrap_or("");
            if text.matches(block).count() == 1 {
                Some(text.replacen(block, "", 1))
            } else {
                preserved.push(format!("{:?}", managed.target));
                continue;
            }
        } else {
            if before.is_some() {
                preserved.push(format!("{:?}", managed.target));
            }
            continue;
        };
        changes.push(Change {
            target: managed.target.clone(),
            before,
            after,
        });
    }
    // Record payloads for bounded deferred deletion before removing the receipt.
    changes.push(change(
        file(layout.root.join("cleanup.json")),
        Some(serde_json::to_string(&Cleanup::from_binaries(
            &prior.binaries,
        ))?),
    )?);
    changes.push(change(file(layout.receipt.clone()), None)?);
    transact(&layout, changes)?;
    let reset_ok = reset(&prior.executable);
    let deferred = cleanup(&layout.root, &prior.binaries)?;
    Ok(
        json!({"removed":true,"preservedModifiedFiles":preserved,"deferredCleanup":deferred,"restartRequired":!reset_ok,"nextStep":"Connection removed. Close this manager and existing Codex chats to release running files. Firefox, your profiles, logins and result tabs are preserved. Remove Codex Work Tabs separately in Firefox Add-ons. Unmodified program files are cleaned up after the manager exits; modified files are preserved."}),
    )
}
fn cleanup(root: &Path, binaries: &[(PathBuf, String)]) -> Result<bool> {
    if root.join("install.json").exists() {
        return Ok(false);
    }
    let mut remaining = Vec::new();
    for (path, hash) in binaries {
        w::no_reparse(path)?;
        ensure!(
            path.starts_with(root.join("versions"))
                && !path.components().any(|c| matches!(
                    c,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                ))
                && path
                    .file_name()
                    .is_some_and(|name| name == "firefox-codex-mcp.exe"
                        || name == "firefox-codex-native-host.exe"),
            "Unsafe cleanup path"
        );
        if path.exists() && digest(&fs::read(path)?) == *hash && fs::remove_file(path).is_err() {
            remaining.push((path.clone(), hash.clone()));
        }
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }
    if remaining.is_empty() {
        let _ = fs::remove_file(root.join("cleanup.json"));
        return Ok(false);
    }
    // Windows locks an executing image. The built-in PowerShell runs a narrowly
    // scoped, encoded deletion after exit; no external script, elevation, shell
    // interpolation, recursive directory deletion or execution-policy bypass.
    use base64::Engine;
    use std::os::windows::process::CommandExt;
    let quote = |v: &str| format!("'{}'", v.replace('\'', "''"));
    let script = format!(
        r#"
$root={root}; $base=[IO.Path]::GetDirectoryName($root);
function SafePath($p) {{
  if(!$p.StartsWith($root+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){{return $false}}
  $at=$p;
  while($at) {{if(Test-Path -LiteralPath $at){{$node=Get-Item -LiteralPath $at -Force;if($node.Attributes -band [IO.FileAttributes]::ReparsePoint){{return $false}}}};$at=[IO.Path]::GetDirectoryName($at)}}
  return $true
}}
for($i=0;$i -lt 120;$i++){{
 Start-Sleep -Seconds 2; $left=0; $guard=$null;
 try {{$guard=[IO.File]::Open((Join-Path $base 'install.lock'),[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)}}catch{{continue}}
 try {{
 if(Test-Path -LiteralPath (Join-Path $base 'install.json')){{break}}
 $items=(Get-Content -LiteralPath (Join-Path $base 'cleanup.json') -Raw | ConvertFrom-Json).files;
 foreach($item in $items){{$p=$item.path; if(@('firefox-codex-mcp.exe','firefox-codex-native-host.exe') -notcontains [IO.Path]::GetFileName($p)){{continue}};
  if((Test-Path -LiteralPath $p) -and (SafePath $p)){{try{{
   if((Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash -eq $item.sha256){{Remove-Item -LiteralPath $p -ErrorAction Stop}}
  }}catch{{$left++}}}}
  $parent=[IO.Path]::GetDirectoryName($p);
  if((SafePath $parent) -and (Test-Path -LiteralPath $parent)){{try{{[IO.Directory]::Delete($parent,$false)}}catch{{}}}}
 }}
 if($left -eq 0){{break}}
 }} finally {{$guard.Dispose()}}
}}
"#,
        root = quote(&root.join("versions").to_string_lossy())
    );
    let encoded = base64::engine::general_purpose::STANDARD.encode(
        script
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    let system = std::env::var_os("SystemRoot").context("SystemRoot missing")?;
    Command::new(PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe"))
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
        .creation_flags(0x08000000)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(true)
}
pub fn repair() -> Result<Value> {
    let layout = Layout::open()?;
    recover(&layout)?;
    if let Some(bytes) =
        crate::transport::read_private(&layout.root.join("cleanup.json"), 4 * 1024 * 1024)?
    {
        cleanup(
            &layout.root,
            &serde_json::from_slice::<Cleanup>(&bytes)?.binaries(),
        )?;
    }
    Ok(json!({"repaired":true,"nextStep":"Interrupted setup recovered."}))
}
pub fn installed_version() -> Option<String> {
    receipt().ok().flatten().map(|r| r.version)
}
pub fn installed_mode() -> Option<String> {
    installed_version().map(|_| "extension".into())
}
pub fn installed_profile() -> Option<PathBuf> {
    None
}
pub fn doctor() -> Result<Value> {
    let prior = receipt()?;
    let root = o::codex_home(&o::home()?);
    let manifest = w::local_root()?.join("firefox_codex_mcp.json");
    let registered =
        read(&registry(native_key(), ""))? == Some(manifest.to_string_lossy().into_owned());
    let config_text = read(&file(root.join("config.toml")))?.unwrap_or_default();
    let document = config_text.parse::<toml_edit::DocumentMut>()?;
    let command = document
        .get("mcp_servers")
        .and_then(|v| v.get("firefox"))
        .and_then(|v| v.get("command"))
        .and_then(|v| v.as_str());
    Ok(
        json!({"globalMcpRegistered":prior.as_ref().is_some_and(|r|command==r.executable.to_str()),"nativeHostRegistered":registered&&manifest.is_file(),"binaryInstalled":prior.as_ref().is_some_and(|r|r.executable.is_file()),"mode":"extension","platform":"windows","firefoxAvailable":crate::transport::firefox().is_ok(),"skillInstalled":read(&file(root.join("skills/firefox-browser/SKILL.md")))?==Some(o::SKILL.into()),"automaticBrowserGuidance":read(&file(root.join("AGENTS.md")))?.is_some_and(|s|s.contains(o::GUIDANCE)),"existingChatsMayNeedRestart":true}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn layout(root: &Path) -> Layout {
        Layout {
            root: root.into(),
            codex: root.join("codex"),
            receipt: root.join("install.json"),
            journal: root.join("transaction.json"),
            _lock: File::create(root.join("lock")).unwrap(),
        }
    }
    #[test]
    fn crash_recovery_restores_files_and_registry_but_preserves_new_edits() {
        let temp = tempfile::tempdir().unwrap();
        let layout = layout(temp.path());
        let a = file(temp.path().join("config.toml"));
        write(&a, &Some("# before\n".into())).unwrap();
        let key = format!(
            r"Software\FirefoxCodexMCPTests\{}",
            crate::random_hex(12).unwrap()
        );
        let b = registry(key.clone(), "test");
        let changes = vec![
            change(a.clone(), Some("after".into())).unwrap(),
            change(b.clone(), Some("new registry".into())).unwrap(),
            change(file(temp.path().join("last")), Some("last".into())).unwrap(),
        ];
        setup::write_atomic(
            &layout.journal,
            &serde_json::to_vec(&changes).unwrap(),
            0o600,
        )
        .unwrap();
        write(&a, &changes[0].after).unwrap();
        write(&b, &changes[1].after).unwrap();
        recover(&layout).unwrap();
        assert_eq!(read(&a).unwrap(), changes[0].before);
        assert_eq!(read(&b).unwrap(), None);
        setup::write_atomic(
            &layout.journal,
            &serde_json::to_vec(&changes).unwrap(),
            0o600,
        )
        .unwrap();
        write(&a, &Some("user edit after crash".into())).unwrap();
        assert!(recover(&layout).is_err());
        assert!(layout.journal.exists());
        assert_eq!(read(&a).unwrap().unwrap(), "user edit after crash");
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(key);
    }
    #[test]
    fn locked_file_failure_rolls_back_config_and_registry() {
        use std::os::windows::fs::OpenOptionsExt;
        let temp = tempfile::tempdir().unwrap();
        let layout = layout(temp.path());
        let a = file(temp.path().join("config.toml"));
        write(&a, &Some("keep".into())).unwrap();
        let key = format!(
            r"Software\FirefoxCodexMCPTests\{}",
            crate::random_hex(12).unwrap()
        );
        let b = registry(key.clone(), "value");
        let blocked = temp.path().join("locked");
        fs::write(&blocked, "original").unwrap();
        let changes = vec![
            change(a.clone(), Some("changed".into())).unwrap(),
            change(b.clone(), Some("new".into())).unwrap(),
            change(file(blocked.clone()), Some("replace".into())).unwrap(),
        ];
        let _locked = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&blocked)
            .unwrap();
        assert!(transact(&layout, changes).is_err());
        assert_eq!(read(&a).unwrap().unwrap(), "keep");
        assert_eq!(read(&b).unwrap(), None);
        assert_eq!(fs::read_to_string(blocked).unwrap(), "original");
        assert!(!layout.journal.exists());
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(key);
    }
    #[test]
    fn configuration_keeps_unrelated_comments_and_foreign_registration() {
        let temp = tempfile::tempdir().unwrap();
        let layout = layout(temp.path());
        setup::private_dir(&layout.codex).unwrap();
        let original = "# keep\n[mcp_servers.other]\ncommand = 'other.exe'\n";
        fs::write(layout.codex.join("config.toml"), original).unwrap();
        let prepared = config(
            &layout,
            None,
            Some(Path::new(r"C:\Users\한글 user\mcp.exe")),
        )
        .unwrap();
        assert!(prepared.after.as_ref().unwrap().starts_with(original));
        assert_eq!(
            fs::read_to_string(layout.codex.join("config.toml")).unwrap(),
            original
        );
        fs::write(
            layout.codex.join("config.toml"),
            "[mcp_servers.firefox]\ncommand='foreign.exe'\n",
        )
        .unwrap();
        assert!(config(&layout, None, Some(Path::new(r"C:\mcp.exe"))).is_err());
    }
}
