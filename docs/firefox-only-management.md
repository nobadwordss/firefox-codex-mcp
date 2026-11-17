# Manage the connection in Firefox

Instructions for **Linux 0.4.2 stable** and **Windows 0.5.0-beta.3 preview**.
Use the [Linux installation guide](first-install-linux.md) or
[Windows preview guide](windows-preview.md) for the correct download. Older
Linux 0.4.1 and Windows beta.2 archives remain available unchanged.

Both packages include **Codex Work Tabs 0.4.2**, approved and signed by Mozilla
for unlisted/direct distribution on 2026-11-17. Source preparation, final archive
verification and platform scope are recorded separately. Linux clean-home
installation and actual model work passed, as did Windows Server source-build
and extracted-ZIP installation, update, removal and Firefox E2E. Windows 10/11
desktop acceptance remains pending.

## Install once, then use ordinary Firefox

1. Install ordinary desktop Firefox and the local Codex app or CLI.
2. Extract the package matching the version you intend to install. On Linux,
   run `./install.sh` from its extracted folder. On Windows, double-click
   `firefox-codex-mcp.exe` as your normal user.
3. Choose **Install connection → Continue** in the local setup page. This installs
   the helper and registers the Codex connection, skill and browser guidance.
4. Choose **Add Codex Work Tabs to Firefox** and accept Firefox's installation
   prompts. Use the Mozilla-signed extension bundled with the package.
5. Start a new Codex chat and ask:

   > Open https://example.com in the browser, click Learn more, and tell me the destination page title.

Install the helper once per OS user and the extension in your everyday Firefox
profile. The extension cannot install a native program or register Codex by itself.
No extra Firefox profile, app-menu launcher, pairing code or manually installed
skill is needed for the ordinary extension connection.

## Manage from the extension

Open ordinary Firefox, click **Codex Work Tabs → Settings**. The connection setup
page opens in a tab in that Firefox. Use **Check setup**, **Repair** or **Remove
connection** there. Firefox's Add-ons Manager can also open the extension controls,
which provide the same **Settings** button. Pause, resume and revoke controls
remain in the extension. Removing the helper and removing the extension are
separate actions.

If you use several Firefox profiles, choose **Use this Firefox** in the intended
one. A single connected profile is selected automatically. A disconnected selected
profile never silently falls back to another. You do not need another profile to
use this integration.

To update the helper, open setup from a newer package. Update the extension with
that package's signed XPI as well; direct-distribution XPIs have no automatic
update channel. Updates release existing work-tab access and leave result tabs
open. Start a new Codex chat afterwards.

If the helper is absent or cannot connect, reopen the extracted package's setup
file. The extension cannot repair a helper it cannot reach. On Linux, the package
also provides `./install.sh --doctor`, `--repair` and `--uninstall` for recovery.
Windows Settings → Apps → Firefox Codex MCP → Uninstall remains available.
If you already removed or disabled the extension, open the extracted package's
setup file to remove or repair the helper. Removing the Firefox add-on alone
does not uninstall the helper or remove Codex's connection settings.

## Verified Firefox controls

These screenshots show the signed 0.4.2 extension and beta.3 candidate helper in
a disposable Linux Firefox 160 profile. They are an automated integration check,
not a first-time-user study or Windows desktop acceptance.

![Extension controls in ordinary Firefox](assets/firefox-only-popup-2026-11-17.png)

**Settings** opens in that same Firefox profile:

![Local Settings page](assets/firefox-only-settings-2026-11-17.png)

## Existing installations

The installer removes its unchanged, previously managed **Firefox Codex
Manager** and **Firefox (Codex connection)** launchers. Modified launchers are
preserved and reported rather than silently overwritten or deleted. Ordinary
Firefox, its profile, logins and result tabs remain available.

Older BiDi debugging installations are an advanced compatibility path. Install
the default extension connection to use this Firefox-based management flow.
The installer does not create a debug launcher. Advanced BiDi users can use
the helper's `--launch-firefox PROFILE` command after saving their work and closing
Firefox; it is not part of normal installation.

These instructions do not establish Windows 10/11 desktop, SmartScreen or
first-time-user acceptance. Source-preparation screenshots and tests retain their
own candidate scope; each release supplies verification for its exact archive.
Earlier release records retain their original scope.

Verification and boundaries: [2026-11-17 record](firefox-only-management-e2e-2026-11-17.json).
