//! Linux setup/launcher for the user's existing profile. Never terminates Firefox.
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn uid() -> u32 {
    unsafe { libc::geteuid() }
}
pub fn profile(value: &str) -> Result<PathBuf> {
    let path = PathBuf::from(value).canonicalize()?;
    ensure!(
        path.is_dir() && path.join("prefs.js").is_file(),
        "Choose an existing Firefox profile directory"
    );
    ensure!(
        path.metadata()?.uid() == uid(),
        "Firefox profile must belong to this user"
    );
    Ok(path)
}
pub fn running(profile: &Path) -> bool {
    // Firefox's Unix profile lock names its owning PID. PID reuse errs toward
    // refusing to start; Firefox's own profile lock remains the final authority.
    fs::read_link(profile.join("lock"))
        .ok()
        .and_then(|path| {
            path.to_str()
                .and_then(|value| value.rsplit_once(':'))
                .map(|(_, pid)| pid.trim_start_matches('+').to_owned())
        })
        .and_then(|pid| pid.parse::<i32>().ok())
        .is_some_and(|pid| {
            pid > 1
                && (unsafe { libc::kill(pid, 0) } == 0
                    || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
        })
}
pub(crate) fn write_atomic(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let temp = path.with_extension(format!("{}.tmp", crate::random_hex(8)?));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        // Persist the rename as well as its contents for interrupted installations.
        if let Some(parent) = path.parent() {
            fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
pub(crate) fn private_dir(path: &Path) -> Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_dir() && meta.uid() == uid() && meta.mode() & 0o077 == 0,
        "Setup directory must be private and owned by this user"
    );
    Ok(())
}
pub(crate) fn quote_desktop(value: &str) -> Result<String> {
    ensure!(
        !value.chars().any(char::is_control),
        "Invalid launcher path"
    );
    Ok(format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('`', "\\`")
            .replace('$', "\\$")
            .replace('%', "%%")
    ))
}
pub fn install(profile: &Path) -> Result<Value> {
    let home = PathBuf::from(env::var_os("HOME").context("HOME unavailable")?);
    let folder = home.join(".local/lib/firefox-codex-mcp");
    private_dir(&folder)?;
    let executable = folder.join("firefox-codex-mcp");
    let bytes = fs::read(env::current_exe()?)?;
    write_atomic(&executable, &bytes, 0o700)?;
    // Preserve regular browsing preferences instead of Firefox's test defaults.
    let user_js = profile.join("user.js");
    let old = match fs::read_to_string(&user_js) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let line = "user_pref(\"remote.prefs.recommended\", false);";
    let mut backup = None;
    if !old.lines().any(|existing| existing.trim() == line) {
        if user_js.exists() {
            let path = folder.join(format!("user-js-backup-{}.js", crate::random_hex(8)?));
            write_atomic(&path, old.as_bytes(), 0o600)?;
            backup = Some(path);
        }
        write_atomic(&user_js,format!("{old}\n// Firefox Codex: keep normal browsing preferences while debugging.\n{line}\n").as_bytes(),0o600)?;
    }
    let applications = home.join(".local/share/applications");
    fs::create_dir_all(&applications)?;
    let launcher = applications.join("firefox-codex.desktop");
    let desktop=format!("[Desktop Entry]\nType=Application\nName=Firefox (Codex connection)\nComment=Use AI work tabs in this Firefox profile\nExec={} --launch-firefox {}\nIcon=firefox\nTerminal=false\nCategories=Network;WebBrowser;\nStartupNotify=true\n",quote_desktop(&executable.to_string_lossy())?,quote_desktop(&profile.to_string_lossy())?);
    write_atomic(&launcher, desktop.as_bytes(), 0o644)?;
    Ok(
        json!({"installed":true,"executable":executable,"launcher":launcher,"profile":profile,"preferencesBackup":backup,"browserRestarted":false}),
    )
}
pub fn launch(profile: &Path) -> Result<Value> {
    if running(profile) {
        let descriptor = profile.join("WebDriverBiDiServer.json");
        if let Ok(file) = fs::File::open(descriptor) {
            if let Ok(value) = serde_json::from_reader::<_, Value>(file) {
                if value["ws_host"] == "127.0.0.1" {
                    if let Some(port) = value["ws_port"]
                        .as_u64()
                        .filter(|port| (1024..=65535).contains(port))
                    {
                        if std::net::TcpStream::connect_timeout(
                            &std::net::SocketAddr::from(([127, 0, 0, 1], port as u16)),
                            Duration::from_millis(500),
                        )
                        .is_ok()
                        {
                            return Ok(
                                json!({"alreadyRunning":true,"profile":profile,"debugging":true}),
                            );
                        }
                    }
                }
            }
        }
        bail!("Firefox is already running. Save your work, close Firefox, then open Firefox (Codex connection). Your profile and logins are preserved.");
    }
    let descriptor = profile.join("WebDriverBiDiServer.json");
    if descriptor.exists() {
        fs::remove_file(&descriptor)?;
    }
    let mut child =
        Command::new(env::var("FIREFOX_BINARY").unwrap_or_else(|_| "/usr/bin/firefox".into()))
            .args(["--new-instance", "--profile"])
            .arg(profile)
            .args(["--remote-debugging-port", "0"])
            // A desktop browser must outlive the short-lived MCP/terminal launcher.
            .process_group(0)
            .env_remove("MOZ_REMOTE_ALLOW_SYSTEM_ACCESS")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(25);
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait()? {
            bail!("Firefox exited before debugging started: {status}");
        }
        if descriptor.is_file() {
            let value: Value = serde_json::from_reader(fs::File::open(&descriptor)?)?;
            if value["ws_host"] == "127.0.0.1"
                && value["ws_port"]
                    .as_u64()
                    .is_some_and(|port| (1024..=65535).contains(&port))
            {
                return Ok(
                    json!({"started":true,"pid":child.id(),"profile":profile,"debugging":true,"systemAccess":false}),
                );
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    // Do not kill a browser that might now contain user work.
    bail!("Firefox started, but its debugging connection did not become ready");
}
pub fn notify_launch_error(message: &str) {
    let _ = Command::new("notify-send")
        .args(["Firefox Codex connection", message])
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_arguments_do_not_expand_control_characters_or_placeholders() {
        assert!(quote_desktop("a\nb").is_err());
        assert_eq!(
            quote_desktop("/tmp/a $x %f \"b\"").unwrap(),
            "\"/tmp/a \\$x %%f \\\"b\\\"\""
        );
    }
    #[test]
    fn live_profile_is_detected_without_terminating_it() {
        let root = tempfile::tempdir().unwrap();
        assert!(!running(root.path()));
        std::os::unix::fs::symlink(
            format!("localhost:+{}", std::process::id()),
            root.path().join("lock"),
        )
        .unwrap();
        assert!(running(root.path()));
        assert!(launch(root.path())
            .unwrap_err()
            .to_string()
            .contains("already running"));
    }
}
