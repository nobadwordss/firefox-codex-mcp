//! Repeatable user-level setup. Delegate TOML edits to the official Codex CLI.
use crate::setup;
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) const SKILL: &str = include_str!("../../skills/firefox-browser/SKILL.md");
pub(crate) const START: &str = "<!-- firefox-codex-mcp:start -->";
pub(crate) const END: &str = "<!-- firefox-codex-mcp:end -->";
pub(crate) const GUIDANCE: &str = "For tasks needing browser interaction, rendered-page checks, or a website workflow on this computer, proactively consider the installed Firefox MCP and firefox-browser skill even when Firefox is not named. Respect an explicit browser choice; use specialized connectors or web search when they better fit. Create your own work tabs with firefox_open_tab. Follow the user's scope and normal approvals; tool availability does not grant blanket permission.\n";

pub(crate) fn home() -> Result<PathBuf> {
    Ok(PathBuf::from(
        env::var_os("HOME").context("HOME is unavailable")?,
    ))
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
        candidates.extend(env::split_paths(&path).map(|path| path.join("codex")));
    }
    candidates.push(PathBuf::from("/usr/lib/chatgpt/resources/codex"));
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .context("Codex를 먼저 설치하세요. Codex CLI 실행 파일을 찾지 못했습니다")
}
fn cli(binary: &Path, root: &Path, args: &[&str]) -> Result<std::process::Output> {
    Command::new(binary)
        .current_dir(root)
        .env("CODEX_HOME", root)
        .args(args)
        .output()
        .context("Cannot run Codex configuration command")
}
pub(crate) fn configured(binary: &Path, root: &Path) -> Result<Option<Value>> {
    let result = cli(binary, root, &["mcp", "list", "--json"])?;
    ensure!(
        result.status.success(),
        "Codex 설정을 읽지 못했습니다: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let values: Vec<Value> = serde_json::from_slice(&result.stdout)?;
    Ok(values.into_iter().find(|value| value["name"] == "firefox"))
}
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
    selected.cloned().context("Firefox 프로필을 하나로 결정할 수 없습니다. Firefox를 한 프로필로 실행한 뒤 다시 설치하거나 --install PROFILE 경로를 지정하세요")
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
pub fn doctor() -> Result<Value> {
    let home = home()?;
    let root = codex_home(&home);
    let configured = configured(&codex()?, &root)?;
    let profile = configured
        .as_ref()
        .and_then(|v| v["transport"]["env"]["FIREFOX_MCP_PROFILE"].as_str())
        .map(PathBuf::from);
    let executable = home.join(".local/lib/firefox-codex-mcp/firefox-codex-mcp");
    let registration = configured.as_ref().is_some_and(|v| {
        v["transport"]["command"].as_str() == executable.to_str()
            && v["transport"]["args"] == json!(["--bidi"])
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
        json!({"globalMcpRegistered":registration,"binaryInstalled":executable.is_file(),
        "skillInstalled":read_optional(&root.join("skills/firefox-browser/SKILL.md"))? == SKILL,
        "automaticBrowserGuidance":read_optional(&root.join("AGENTS.md"))?.contains(GUIDANCE),
        "profile":profile,"firefoxRunning":profile.as_deref().is_some_and(setup::running),
        "debugPortReachable":reachable,"existingChatsMayNeedRestart":true,
        "scope":"Local Codex on this computer; project overrides and administrator policy can restrict availability"}),
    )
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
