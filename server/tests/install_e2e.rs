use anyhow::{ensure, Context, Result};
use serde_json::Value;
use std::{env, fs, path::Path, process::Command};

#[test]
#[ignore = "uses the installed Codex CLI with an isolated HOME to test global setup"]
fn global_install_preserves_config_and_repeats() -> Result<()> {
    let codex = env::var("FIREFOX_SETUP_CODEX_BINARY").context("Set FIREFOX_SETUP_CODEX_BINARY")?;
    let root = tempfile::tempdir()?;
    let home = root.path();
    let config = home.join(".codex");
    fs::create_dir_all(&config)?;
    fs::write(
        config.join("config.toml"),
        "# keep my comment\nmodel = \"test-model\"\n[mcp_servers.other]\ncommand = \"/bin/true\"\n",
    )?;
    fs::write(
        config.join("AGENTS.md"),
        "Keep my existing Rust preference.\n",
    )?;
    let firefox = home.join(".config/mozilla/firefox");
    fs::create_dir_all(firefox.join("main"))?;
    fs::write(firefox.join("main/prefs.js"), "")?;
    let original_pref = "// user preference\nuser_pref(\"example.keep\", true);\n";
    fs::write(firefox.join("main/user.js"), original_pref)?;
    fs::write(
        firefox.join("profiles.ini"),
        "[Profile0]\nIsRelative=1\nPath=main\nDefault=1\n",
    )?;
    let run = |binary: &Path, args: &[&str]| -> Result<std::process::Output> {
        Ok(Command::new(binary)
            .args(args)
            .current_dir(home)
            .env("HOME", home)
            .env("CODEX_HOME", &config)
            .env("FIREFOX_MCP_CODEX_BINARY", &codex)
            .output()?)
    };
    let source = env::var_os("FIREFOX_SETUP_SERVER_BINARY")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_firefox-codex-mcp").into());
    let executable = Path::new(&source);
    let first = run(executable, &["--install"])?;
    ensure!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let installed = home.join(".local/lib/firefox-codex-mcp/firefox-codex-mcp");
    let config_once = fs::read_to_string(config.join("config.toml"))?;
    ensure!(config_once.contains(installed.to_str().context("Invalid install path")?));
    ensure!(!config_once.contains(executable.to_str().context("Invalid source path")?));
    let agents_once = fs::read_to_string(config.join("AGENTS.md"))?;
    let second = run(&installed, &["--update"])?;
    ensure!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    ensure!(config_once == fs::read_to_string(config.join("config.toml"))?);
    ensure!(agents_once == fs::read_to_string(config.join("AGENTS.md"))?);
    ensure!(
        config_once.contains("test-model")
            && config_once.contains("mcp_servers.other")
            && config_once.contains("keep my comment")
    );
    ensure!(agents_once.starts_with("Keep my existing Rust preference."));
    let doctor = run(&installed, &["--doctor"])?;
    ensure!(
        doctor.status.success(),
        "{}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    let result: Value = serde_json::from_slice(&doctor.stdout)?;
    for key in [
        "globalMcpRegistered",
        "binaryInstalled",
        "skillInstalled",
        "automaticBrowserGuidance",
    ] {
        ensure!(result[key] == true, "{key}: {result}");
    }
    ensure!(result["firefoxRunning"] == false);
    // An unrelated MCP with the same name must not be replaced.
    let changed = config_once.replace(
        &installed.to_string_lossy().to_string(),
        "/bin/other-browser",
    );
    fs::write(config.join("config.toml"), &changed)?;
    ensure!(!run(&installed, &["--install"])?.status.success());
    ensure!(fs::read_to_string(config.join("config.toml"))? == changed);
    fs::write(config.join("config.toml"), &config_once)?;
    // Removal restores untouched originals, without removing unrelated MCPs.
    let removal = run(executable, &["--uninstall"])?;
    ensure!(
        removal.status.success(),
        "{}",
        String::from_utf8_lossy(&removal.stderr)
    );
    ensure!(!installed.exists());
    ensure!(fs::read_to_string(config.join("AGENTS.md"))? == "Keep my existing Rust preference.\n");
    ensure!(fs::read_to_string(firefox.join("main/user.js"))? == original_pref);
    let remaining = fs::read_to_string(config.join("config.toml"))?;
    ensure!(
        !remaining.contains("mcp_servers.firefox")
            && remaining.contains("mcp_servers.other")
            && remaining.contains("keep my comment")
    );
    ensure!(run(executable, &["--uninstall"])?.status.success());
    ensure!(run(executable, &["--install"])?.status.success());
    // User edits after installation survive both update and removal.
    let agents_path = config.join("AGENTS.md");
    fs::write(
        &agents_path,
        format!(
            "{}\nAdded after installation.\n",
            fs::read_to_string(&agents_path)?
        ),
    )?;
    let pref_path = firefox.join("main/user.js");
    fs::write(
        &pref_path,
        format!(
            "{}\nuser_pref(\"example.later\", 42);\n",
            fs::read_to_string(&pref_path)?
        ),
    )?;
    let skill = config.join("skills/firefox-browser/SKILL.md");
    fs::write(&skill, "My customized browser skill.\n")?;
    ensure!(run(executable, &["--update"])?.status.success());
    ensure!(run(executable, &["--uninstall"])?.status.success());
    let agents = fs::read_to_string(agents_path)?;
    ensure!(
        agents.contains("Added after installation") && !agents.contains("firefox-codex-mcp:start")
    );
    let prefs = fs::read_to_string(pref_path)?;
    ensure!(
        prefs.contains("example.keep")
            && prefs.contains("example.later")
            && !prefs.contains("remote.prefs.recommended")
    );
    ensure!(fs::read_to_string(skill)? == "My customized browser skill.\n");
    ensure!(firefox.join("main/prefs.js").exists());
    Ok(())
}
