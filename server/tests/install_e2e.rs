#![cfg(unix)]
use anyhow::{ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;
use std::{env, fs, path::Path, process::Command};

// Reproduce a receipt written by a release that owned app-menu shortcuts. The
// current installer must retire only those recorded files, not sweep the menu.
fn seed_legacy_launcher(
    home: &Path,
    name: &str,
    before: Option<&str>,
    current: Option<&str>,
) -> Result<()> {
    let path = home.join(".local/share/applications").join(name);
    fs::create_dir_all(path.parent().context("launcher parent")?)?;
    if let Some(current) = current {
        fs::write(&path, current)?;
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    } else if path.exists() {
        fs::remove_file(&path)?;
    }
    let receipt_path = home.join(".local/state/firefox-codex-mcp/install.json");
    let mut receipt: Value = serde_json::from_slice(&fs::read(&receipt_path)?)?;
    let files = receipt["files"].as_array_mut().context("receipt files")?;
    files.retain(|file| file["path"] != path.to_string_lossy().as_ref());
    let snapshot = |value: &str| serde_json::json!({"bytes":STANDARD.encode(value),"mode":0o600});
    files.push(serde_json::json!({
        "path":path,
        "before":before.map(snapshot),
        "installed":snapshot("Legacy managed shortcut.\n"),
        "block":null
    }));
    fs::write(receipt_path, serde_json::to_vec_pretty(&receipt)?)?;
    Ok(())
}

#[test]
#[ignore = "uses the installed Codex CLI with an isolated HOME to test global setup"]
fn global_install_preserves_config_and_repeats() -> Result<()> {
    let codex = env::var("FIREFOX_SETUP_CODEX_BINARY").context("Set FIREFOX_SETUP_CODEX_BINARY")?;
    let root = tempfile::tempdir()?;
    let home = root.path();
    let manager = home.join(".local/share/applications/firefox-codex-manager.desktop");
    let launcher = home.join(".local/share/applications/firefox-codex.desktop");
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
            .env("FIREFOX_MCP_RUNTIME_DIR", home.join("runtime"))
            .env("FIREFOX_MCP_CODEX_BINARY", &codex)
            .output()?)
    };
    let source = env::var_os("FIREFOX_SETUP_SERVER_BINARY")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_firefox-codex-mcp").into());
    let executable = Path::new(&source);
    // A published older installer can seed this same disposable account, so
    // the ordinary update checks also exercise a real release migration.
    let upgrade_from = env::var_os("FIREFOX_UPGRADE_FROM_BINARY");
    let initial = upgrade_from.as_deref().map(Path::new).unwrap_or(executable);
    // No Firefox profile must be needed for the default native connection.
    let saved_firefox = home.join("saved-firefox");
    fs::rename(&firefox, &saved_firefox)?;
    let first = run(initial, &["--install"])?;
    fs::rename(&saved_firefox, &firefox)?;
    ensure!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    if upgrade_from.is_none() {
        ensure!(!manager.exists() && !launcher.exists());
    }
    let installed = home.join(".local/lib/firefox-codex-mcp/firefox-codex-mcp");
    let initial_version = run(&installed, &["--version"])?;
    ensure!(initial_version.status.success());
    let config_once = fs::read_to_string(config.join("config.toml"))?;
    ensure!(config_once.contains(installed.to_str().context("Invalid install path")?));
    ensure!(!config_once.contains(executable.to_str().context("Invalid source path")?));
    ensure!(config_once.contains("--extension") && !config_once.contains("FIREFOX_MCP_PROFILE"));
    let manifest = home.join(".mozilla/native-messaging-hosts/firefox_codex_mcp.json");
    let native: Value = serde_json::from_slice(&fs::read(&manifest)?)?;
    ensure!(
        native["allowed_extensions"]
            == serde_json::json!(["{a171de16-96b6-4e50-94b7-e0b54d94436a}"])
    );
    let wrapper = Path::new(native["path"].as_str().context("native path")?);
    use std::os::unix::fs::PermissionsExt;
    ensure!(fs::metadata(wrapper)?.permissions().mode() & 0o777 == 0o700);
    let agents_once = fs::read_to_string(config.join("AGENTS.md"))?;
    let second = run(executable, &["--update"])?;
    ensure!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    ensure!(config_once == fs::read_to_string(config.join("config.toml"))?);
    ensure!(agents_once == fs::read_to_string(config.join("AGENTS.md"))?);
    let installed_version = run(&installed, &["--version"])?;
    let target_version = run(executable, &["--version"])?;
    ensure!(installed_version.status.success() && target_version.status.success());
    ensure!(installed_version.stdout == target_version.stdout);
    ensure!(fs::read(&installed)? == fs::read(executable)?);
    ensure!(!manager.exists() && !launcher.exists());
    if upgrade_from.is_some() {
        ensure!(
            fs::read_to_string(config.join("skills/firefox-browser/SKILL.md"))?
                == include_str!("../../skills/firefox-browser/SKILL.md")
        );
    }
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
    ensure!(result["firefoxRunning"].is_null());
    ensure!(result["debugPortReachable"].is_null());
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
    ensure!(!manifest.exists() && !wrapper.exists());
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
    // Explicit advanced mode still works, and default install migrates it safely.
    fs::remove_file(config.join("skills/firefox-browser/SKILL.md"))?;
    let advanced = run(
        executable,
        &[
            "--install-bidi",
            firefox.join("main").to_str().context("profile")?,
        ],
    )?;
    ensure!(
        advanced.status.success(),
        "advanced: {}",
        String::from_utf8_lossy(&advanced.stderr)
    );
    ensure!(fs::read_to_string(config.join("config.toml"))?.contains("--bidi"));
    ensure!(!manager.exists() && !launcher.exists());
    ensure!(run(executable, &["--update"])?.status.success());
    ensure!(fs::read_to_string(config.join("config.toml"))?.contains("--bidi"));

    // The extension migration removes old receipt-owned shortcuts, restores an
    // original shortcut baseline, and removes only our known preference block.
    seed_legacy_launcher(
        home,
        "firefox-codex-manager.desktop",
        None,
        Some("Legacy managed shortcut.\n"),
    )?;
    seed_legacy_launcher(
        home,
        "firefox-codex.desktop",
        Some("Original shortcut.\n"),
        Some("Legacy managed shortcut.\n"),
    )?;
    let pref_path = firefox.join("main/user.js");
    fs::write(
        &pref_path,
        format!(
            "{}user_pref(\"example.during_bidi\", true);\n",
            fs::read_to_string(&pref_path)?
        ),
    )?;
    fs::set_permissions(&pref_path, fs::Permissions::from_mode(0o640))?;
    let lock = firefox.join("main/lock");
    let lock_target = format!("localhost:+{}", std::process::id());
    std::os::unix::fs::symlink(&lock_target, &lock)?;
    ensure!(run(executable, &["--install"])?.status.success());
    let migrated = fs::read_to_string(config.join("config.toml"))?;
    ensure!(migrated.contains("--extension") && !migrated.contains("FIREFOX_MCP_PROFILE"));
    ensure!(!manager.exists());
    ensure!(fs::read_to_string(&launcher)? == "Original shortcut.\n");
    let prefs = fs::read_to_string(&pref_path)?;
    ensure!(prefs.contains("example.during_bidi") && !prefs.contains("remote.prefs.recommended"));
    ensure!(!prefs.contains("firefox-codex-mcp:start"));
    ensure!(fs::metadata(&pref_path)?.permissions().mode() & 0o777 == 0o640);
    ensure!(fs::read_link(&lock)? == Path::new(&lock_target));

    // Deleted and edited shortcuts remain user-owned after migration and later
    // removal. In particular, uninstall must not resurrect their old baselines.
    seed_legacy_launcher(
        home,
        "firefox-codex-manager.desktop",
        Some("Deleted original shortcut.\n"),
        None,
    )?;
    seed_legacy_launcher(
        home,
        "firefox-codex.desktop",
        Some("Original shortcut.\n"),
        Some("My edited shortcut.\n"),
    )?;
    ensure!(run(executable, &["--update"])?.status.success());
    ensure!(!manager.exists());
    ensure!(fs::read_to_string(&launcher)? == "My edited shortcut.\n");
    ensure!(run(executable, &["--uninstall"])?.status.success());
    ensure!(!manager.exists());
    ensure!(fs::read_to_string(&launcher)? == "My edited shortcut.\n");
    ensure!(fs::read_link(&lock)? == Path::new(&lock_target));

    // A same-named, unowned shortcut and an existing standalone preference are
    // outside the receipt. Neither a fresh install nor migration may adopt them.
    let user_pref = format!(
        "{}user_pref(\"remote.prefs.recommended\", false);\n",
        fs::read_to_string(&pref_path)?
    );
    fs::write(&pref_path, &user_pref)?;
    ensure!(run(
        executable,
        &[
            "--install-bidi",
            firefox.join("main").to_str().context("profile")?
        ]
    )?
    .status
    .success());
    ensure!(run(executable, &["--install"])?.status.success());
    ensure!(run(executable, &["--uninstall"])?.status.success());
    ensure!(fs::read_to_string(&pref_path)? == user_pref);
    ensure!(!manager.exists());
    ensure!(fs::read_to_string(&launcher)? == "My edited shortcut.\n");
    ensure!(fs::read_link(&lock)? == Path::new(&lock_target));
    if let Some(out) = env::var_os("FIREFOX_UPGRADE_ARTIFACTS") {
        fs::create_dir_all(&out)?;
        fs::write(
            Path::new(&out).join("upgrade-result.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "from":String::from_utf8(initial_version.stdout)?.trim(),
                "to":String::from_utf8(installed_version.stdout)?.trim(),
                "publishedSourceProvided":upgrade_from.is_some(),
                "installedBinaryMatchesTarget":true,
                "unrelatedConfigurationPreserved":true,
                "englishSkill":upgrade_from.is_some(),
                "noAppMenuLaunchers":true,
                "legacyLauncherMigrationPreservesUserChanges":true,
                "foreignLauncherAndPreferencePreserved":true,
                "repeatedInstallUpdateRemoval":true,
                "userEditsPreserved":true,
                "advancedModeMigration":true
            }))?,
        )?;
    }
    Ok(())
}
