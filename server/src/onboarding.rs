//! Repeatable user-level setup and diagnostics without requiring a CLI on PATH.
use crate::setup;
use anyhow::{bail, Context, Result};
#[cfg(unix)]
use serde_json::json;
use serde_json::Value;
#[cfg(unix)]
use std::collections::BTreeMap;
#[cfg(unix)]
use std::fs;
use std::{
    env,
    path::{Path, PathBuf},
};

pub(crate) const SKILL: &str = include_str!("../../skills/firefox-browser/SKILL.md");
pub(crate) const START: &str = "<!-- firefox-codex-mcp:start -->";
pub(crate) const END: &str = "<!-- firefox-codex-mcp:end -->";
pub(crate) const GUIDANCE: &str = "For tasks needing browser interaction, rendered-page checks, or a website workflow on this computer, proactively consider the installed Firefox MCP and firefox-browser skill even when Firefox is not named. Respect an explicit browser choice; use specialized connectors or web search when they better fit. Create your own work tabs with firefox_open_tab. Follow the user's scope and normal approvals; tool availability does not grant blanket permission.\n";

pub(crate) fn home() -> Result<PathBuf> {
    #[cfg(unix)]
    {
        Ok(PathBuf::from(
            env::var_os("HOME").context("HOME is unavailable")?,
        ))
    }
    #[cfg(windows)]
    {
        crate::windows::known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Profile)
    }
}
pub(crate) fn codex_home(home: &Path) -> PathBuf {
    env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"))
}
pub(crate) fn codex() -> Result<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("FIREFOX_MCP_CODEX_BINARY") {
        candidates.push(PathBuf::from(path));
    }
    if let Some(path) = env::var_os("PATH") {
        candidates.extend(
            env::split_paths(&path)
                .map(|path| path.join(if cfg!(windows) { "codex.exe" } else { "codex" })),
        );
    }
    candidates.push(PathBuf::from("/usr/lib/chatgpt/resources/codex"));
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .context("Codex executable not found. Install the Codex app or CLI first")
}
#[cfg(unix)]
pub(crate) fn configured(_binary: &Path, root: &Path) -> Result<Option<Value>> {
    let document = read_optional(&root.join("config.toml"))?
        .parse::<toml_edit::DocumentMut>()
        .context("Cannot read Codex config.toml")?;
    let Some(table) = document
        .get("mcp_servers")
        .and_then(|servers| servers.get("firefox"))
    else {
        return Ok(None);
    };
    let args = table
        .get("args")
        .and_then(toml_edit::Item::as_array)
        .map(|array| {
            array
                .iter()
                .map(|value| value.as_str().context("Firefox MCP args must be strings"))
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    let mut environment = serde_json::Map::new();
    if let Some(env) = table.get("env").and_then(toml_edit::Item::as_table_like) {
        for (name, value) in env.iter() {
            environment.insert(
                name.into(),
                json!(value
                    .as_str()
                    .context("Firefox MCP environment must contain strings")?),
            );
        }
    }
    Ok(Some(json!({"name":"firefox",
        "enabled":table.get("enabled").and_then(toml_edit::Item::as_bool).unwrap_or(true),
        "transport":{"command":table.get("command").and_then(toml_edit::Item::as_str),
        "args":args,"env":environment}})))
}
#[cfg(unix)]
fn ini(text: &str) -> Vec<(String, BTreeMap<String, String>)> {
    let mut sections = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            sections.push((name.to_owned(), BTreeMap::new()));
        } else if let (Some((key, value)), Some((_, fields))) =
            (line.split_once('='), sections.last_mut())
        {
            fields.insert(key.trim().to_owned(), value.trim().to_owned());
        }
    }
    sections
}
pub(crate) fn profiles(home: &Path) -> Result<Vec<(PathBuf, bool)>> {
    #[cfg(windows)]
    {
        let _ = home;
        Ok(Vec::new())
    }
    #[cfg(unix)]
    {
        let mut found = BTreeMap::<PathBuf, bool>::new();
        for location in [
            ".config/mozilla/firefox",
            ".mozilla/firefox",
            "snap/firefox/common/.mozilla/firefox",
            ".var/app/org.mozilla.firefox/.mozilla/firefox",
        ] {
            let root = home.join(location);
            let path = root.join("profiles.ini");
            if !path.is_file() {
                continue;
            }
            let sections = ini(&fs::read_to_string(path)?);
            let installation_defaults: Vec<_> = sections
                .iter()
                .filter(|(name, _)| name.starts_with("Install"))
                .filter_map(|(_, fields)| fields.get("Default"))
                .collect();
            for (name, fields) in &sections {
                if !name.starts_with("Profile") {
                    continue;
                }
                let Some(value) = fields.get("Path") else {
                    continue;
                };
                let path = if fields.get("IsRelative").is_some_and(|v| v == "1") {
                    root.join(value)
                } else {
                    PathBuf::from(value)
                };
                if let Ok(path) = setup::profile(&path.to_string_lossy()) {
                    let default = if installation_defaults.is_empty() {
                        fields.get("Default").is_some_and(|v| v == "1")
                    } else {
                        installation_defaults.contains(&value)
                    };
                    found
                        .entry(path)
                        .and_modify(|existing| *existing |= default)
                        .or_insert(default);
                }
            }
        }
        Ok(found.into_iter().collect())
    }
}
pub(crate) fn choose(candidates: &[(PathBuf, bool)]) -> Result<PathBuf> {
    let active: Vec<_> = candidates
        .iter()
        .filter(|(path, _)| setup::running(path))
        .collect();
    let defaults: Vec<_> = candidates.iter().filter(|(_, default)| *default).collect();
    let selected = if active.len() == 1 {
        Some(&active[0].0)
    } else if active.is_empty() && defaults.len() == 1 {
        Some(&defaults[0].0)
    } else if candidates.len() == 1 {
        Some(&candidates[0].0)
    } else {
        None
    };
    selected.cloned().context("Could not choose a single Firefox profile. Open one profile and try again, or specify its path with --install-bidi PROFILE")
}
pub(crate) fn guidance(old: &str) -> Result<String> {
    let block = format!("{START}\n{GUIDANCE}{END}");
    match (old.find(START), old.find(END)) {
        (None, None) => Ok(format!(
            "{}{block}\n",
            if old.is_empty() {
                String::new()
            } else {
                format!("{}\n\n", old.trim_end())
            }
        )),
        (Some(start), Some(end))
            if end > start && old.matches(START).count() == 1 && old.matches(END).count() == 1 =>
        {
            Ok(format!(
                "{}{block}{}",
                &old[..start],
                &old[end + END.len()..]
            ))
        }
        _ => bail!("Firefox instruction markers are incomplete; preserving AGENTS.md"),
    }
}
#[cfg(unix)]
pub(crate) fn read_optional(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(value) => Ok(value),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e.into()),
    }
}
pub fn install(explicit: Option<&str>) -> Result<Value> {
    crate::lifecycle::install(explicit, false)
}
#[cfg(unix)]
pub fn doctor() -> Result<Value> {
    let home = home()?;
    let root = crate::lifecycle::connection_codex_home(&home)?;
    let configured = configured(Path::new(""), &root)?;
    let profile = configured
        .as_ref()
        .and_then(|v| v["transport"]["env"]["FIREFOX_MCP_PROFILE"].as_str())
        .map(PathBuf::from);
    let package = crate::package_install::installed()?;
    let executable = package
        .as_ref()
        .map(|p| p.executable().to_path_buf())
        .unwrap_or_else(|| home.join(".local/lib/firefox-codex-mcp/firefox-codex-mcp"));
    let user_manifest = home.join(".mozilla/native-messaging-hosts/firefox_codex_mcp.json");
    let native_registered = if let Some(package) = &package {
        fs::symlink_metadata(&user_manifest).is_err() && package.native_host_registered()
    } else {
        user_manifest.is_file()
    };
    let registration = configured.as_ref().is_some_and(|v| {
        v["transport"]["command"].as_str() == executable.to_str()
            && (v["transport"]["args"] == json!(["--bidi"])
                || v["transport"]["args"] == json!(["--extension"]))
            && v["enabled"] == true
    });
    let mut reachable = false;
    if let Some(profile) = &profile {
        if let Ok(bytes) = fs::read(profile.join("WebDriverBiDiServer.json")) {
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                if value["ws_host"] == "127.0.0.1" {
                    if let Some(port) = value["ws_port"]
                        .as_u64()
                        .filter(|v| (1024..=65535).contains(v))
                    {
                        reachable = std::net::TcpStream::connect_timeout(
                            &std::net::SocketAddr::from(([127, 0, 0, 1], port as u16)),
                            std::time::Duration::from_millis(500),
                        )
                        .is_ok();
                    }
                }
            }
        }
    }
    Ok(
        json!({"globalMcpRegistered":registration,"mode":if profile.is_some(){"bidi"}else{"extension"},"nativeHostRegistered":native_registered,"binaryInstalled":executable.is_file(),
        "systemPackage":package.is_some(),"executable":executable,
        "packageGeneration":package.as_ref().map(|p|p.generation()),
        "skillInstalled":read_optional(&root.join("skills/firefox-browser/SKILL.md"))? == SKILL,
        "automaticBrowserGuidance":read_optional(&root.join("AGENTS.md"))?.contains(GUIDANCE),
        "profile":profile,"firefoxRunning":profile.as_deref().map(setup::running),
        "debugPortReachable":profile.as_ref().map(|_|reachable),"existingChatsMayNeedRestart":true,
        "scope":"Local Codex on this computer; project overrides and administrator policy can restrict availability"}),
    )
}

#[cfg(windows)]
pub fn doctor() -> Result<Value> {
    crate::lifecycle::doctor()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guidance_preserves_user_rules_and_is_repeatable() {
        let old = "Prefer Rust.\n";
        let once = guidance(old).unwrap();
        assert!(once.starts_with("Prefer Rust.\n"));
        assert_eq!(once, guidance(&once).unwrap());
        assert!(guidance(&format!("{START} incomplete")).is_err());
    }
    #[test]
    #[cfg(unix)]
    fn discovery_prefers_install_default_over_legacy_default() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join(".config/mozilla/firefox");
        for name in ["modern", "legacy"] {
            fs::create_dir_all(root.join(name)).unwrap();
            fs::write(root.join(name).join("prefs.js"), "").unwrap();
        }
        fs::write(root.join("profiles.ini"), "[Profile0]\nIsRelative=1\nPath=modern\n[Profile1]\nIsRelative=1\nPath=legacy\nDefault=1\n[Install123]\nDefault=modern\n").unwrap();
        assert_eq!(
            choose(&profiles(home.path()).unwrap()).unwrap(),
            root.join("modern")
        );
        assert!(choose(&[(root.join("modern"), true), (root.join("legacy"), true)]).is_err());
    }
}
