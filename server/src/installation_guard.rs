//! Fence every running system-package process to the installation it started in.
//! A package transaction never needs to enumerate users or terminate Firefox.
use anyhow::Result;

#[cfg(unix)]
static INSTALLATION: std::sync::OnceLock<crate::package_install::Installation> =
    std::sync::OnceLock::new();
#[cfg(unix)]
static CONNECTION: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub fn start() -> Result<()> {
    #[cfg(unix)]
    if let Some(installation) = crate::package_install::current()? {
        INSTALLATION
            .set(installation)
            .map_err(|_| anyhow::anyhow!("Installation guard already started"))?;
        std::thread::Builder::new()
            .name("package-lifetime".into())
            .spawn(|| loop {
                std::thread::sleep(std::time::Duration::from_millis(500));
                if let Err(error) = validate() {
                    eprintln!("Firefox Codex package changed or was removed: {error:#}. Start a new Codex chat after setup.");
                    // Tokio stdin uses an uncancellable blocking read. Exiting
                    // this helper closes all its transports immediately; waiting
                    // for the runtime to shut down could retain Firefox access.
                    std::process::exit(1);
                }
            })?;
    }
    Ok(())
}

pub fn validate() -> Result<()> {
    #[cfg(unix)]
    if let Some(installation) = INSTALLATION.get() {
        installation.validate()?;
    }
    Ok(())
}

/// Native hosts remain available for Settings before this user connects Codex.
/// Chat requests require an active, owned user connection as well.
pub fn chat() -> Result<()> {
    validate()?;
    #[cfg(unix)]
    if INSTALLATION.get().is_some() {
        let identity = crate::lifecycle::connection_identity()?.ok_or_else(|| anyhow::anyhow!(
            "Codex is disconnected for this account. Open Codex Work Tabs → Settings and choose Connect Codex."
        ))?;
        anyhow::ensure!(
            CONNECTION.get_or_init(|| identity.clone()) == &identity,
            "This Codex connection was replaced. Start a new chat; previous tab access cannot be restored."
        );
    }
    Ok(())
}
