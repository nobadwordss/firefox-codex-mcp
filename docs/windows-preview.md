# Windows preview: installation and verification boundary

> **Release status:** Linux **0.4.2** remains the published stable release.
> The **0.4.3 RPM/DEB candidate is paused and unreleased**; see the
> [pause handoff](linux-packaging-handoff-2026-11-17.md) for remaining validation.

The [0.5.0-beta.3 English preview](https://github.com/nobadwordss/firefox-codex-mcp/releases/tag/v0.5.0-beta.3)
targets Windows 10 22H2 and Windows 11 x64 with ordinary desktop Firefox 140+.
It includes Mozilla-signed English extension 0.4.2 and management through
**Codex Work Tabs → Settings** in your normal Firefox. No extra launcher or
Firefox profile is needed. This is a preview, not a claim that Windows 10/11
desktop acceptance testing is complete.

Linux 0.4.2 is the separate English stable release with the same signed extension.
It publishes no Windows installer. GitHub Latest points to Linux; use the
Windows-specific download below. Earlier Linux 0.4.1 and Windows beta.1/beta.2
archives remain unchanged. See the [management guide](firefox-only-management.md).

## Install, update and remove

[Download the Windows beta.3 ZIP](https://github.com/nobadwordss/firefox-codex-mcp/releases/download/v0.5.0-beta.3/firefox-codex-mcp-0.5.0-beta.3-windows-x86_64.zip)
and fully extract it. Use this Windows-specific link; GitHub Latest remains the
Linux stable release. Double-click
`firefox-codex-mcp.exe` as your normal Windows user. Keep the bundled license notices
with the application when redistributing it. Choose **Install connection → Continue**, then **Add
Codex Work Tabs to Firefox**. Confirm Firefox's normal extension installation.
Start a new Codex chat and try: `Open https://example.com in the browser, click Learn more, and tell me the destination page title.`
A new work tab and successful link click verify the connection. Review tool
approval requests for the task you asked for. No administrator rights, code, token, terminal, profile
path, Rust or Node installation is required. The installer can register desktop
Codex even when its command-line executable is not on PATH.

The manager falls back to your default browser if it cannot locate Firefox. Install
desktop Firefox from Mozilla, then reopen setup in Firefox to install the extension.
Install the helper once per Windows user; the extension cannot install it alone.
After setup, use ordinary Firefox. Open **Codex Work Tabs → Settings** for checks,
repair and removal. In the English extension, **Pause** pauses control and
**Revoke all tab access** revokes all work tabs. A single connected profile is
selected automatically; **Use this Firefox** appears when a profile choice is needed.

The helper EXE is not Authenticode-signed, and its downloaded-file SmartScreen
flow is unverified. Do not disable Windows security to install the preview. If
blocked, wait for signed delivery or review/build the source.

Download and extract a newer ZIP, then open its EXE to update. Executing an older downloaded EXE
installs that version, so choose the intended release. Choose **Update connection**,
then **Add Codex Work Tabs to Firefox** to install the bundled signed extension
0.4.2 too. Direct-distribution XPIs have no automatic update channel. Updates
release previous work-tab access while leaving result tabs open; start a new chat afterwards.
To remove the helper, open the extension's **Settings → Remove connection**.
Windows Settings → Apps → Firefox Codex MCP → Uninstall is also available.
The setup service exits after successful removal. Then remove the Firefox
extension separately if desired. Profiles, logins and visible result tabs remain intact.
If the extension was already removed or the helper cannot connect, reopen the
extracted package's EXE for repair or removal. Removing the add-on alone leaves
the helper and Codex registration.

Running Codex chats can keep old EXEs locked. Removal retries unchanged payload
deletion for up to four minutes. Close those chats promptly; if a file is still
locked, run a downloaded setup EXE with `--repair` after closing them. Recovery
metadata and profile-selection state may remain under the user installation root.
Changed files/registry values are preserved. Reinstalling cancels an old cleanup
worker; it never recursively deletes the installation directory.
If organizational policy blocks PowerShell, connection removal can still finish
but deferred file cleanup may not run. Close the affected chats and use `--repair`
from the extracted package; do not weaken that policy to remove the helper.

## Windows implementation

- Known folders resolve the Windows profile and LocalAppData. The helper lives in
  `LocalAppData\FirefoxCodexMCP\versions\<version>-<content hash>`.
- A named copy, `firefox-codex-native-host.exe`, supplies Firefox's no-argument
  native-host entry point. Arguments for broker processes still route normally.
- HKCU `Software\Mozilla\NativeMessagingHosts\firefox_codex_mcp` points to a UTF-8
  native manifest. The Firefox ID and frame protocol are unchanged.
- Codex's global TOML entry, browser skill, managed AGENTS block, native manifest,
  uninstall entry and receipt use compare-before-write plus a recovery journal.
  The immutable executable payload is staged first; an interrupted pre-commit
  stage can leave an inert, unregistered version directory.
- IPC uses a local named pipe scoped to the user's logon identity and installation.
  Its explicit DACL admits that logon SID. Remote clients are rejected. Both sides
  compare the peer process's user SID, logon SID and session ID. The first broker
  requires a first pipe instance and retains a replacement instance across accepts.
  This does not isolate malicious processes running as the same logged-in user.
- Brokers inherit null standard handles. Native host EOF is separate from MCP
  stdio. Updates reset only this router; Firefox itself is not terminated.
- All chat identity, stale-attachment fencing, 24-hour expiry, tab ownership and
  revocation logic remains shared with Linux. No Windows-only adoption capability.

The native registration follows [Mozilla's Native Messaging rules](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_messaging#windows_setup).
Pipe isolation follows [Microsoft's named-pipe security guidance](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights),
using [Tokio's Windows named-pipe APIs](https://docs.rs/tokio/latest/tokio/net/windows/named_pipe/index.html).

## Verification

The `Windows preview` GitHub workflow uses **Windows Server 2022 x64**, official
desktop Firefox, Mozilla geckodriver and the official native Windows Codex CLI.
It compiles with MSVC, runs Clippy and unit tests, exercises first-user setup in
paths containing Korean, spaces, an apostrophe and `&`, then repeats the E2E from
an extracted distribution outside the checkout.

The integration harness covers the manager and Firefox installation prompts,
permanent signed extension loading, actual native messaging/pipe transport, text
and status reads, clicks, typing with independent DOM readback, chat isolation,
MCP replacement, explicit revocation, running-EXE update, Firefox restart and
removal with unrelated configuration preserved. Recovery tests cover partially
applied registry/file journals, locked-file failure and external edits.

The Codex test launches the **real Codex app-server**, creates ephemeral test
chats, and uses its `mcpServer/tool/call` API to exercise Firefox. It does not use a
mock MCP server. It also does not prove model-driven tool selection or the Windows
Codex desktop UI: no live model/API account is sent to CI.

The [beta.3 release page](https://github.com/nobadwordss/firefox-codex-mcp/releases/tag/v0.5.0-beta.3)
provides the exact ZIP, `SHA256SUMS`, provenance and `release-verification.json`.
The [Firefox management preparation record](firefox-only-management-e2e-2026-11-17.json)
retains the earlier source/ZIP checks and failures that led to the Settings lifetime
fix. Final release verification identifies its exact archive separately.
The historical [repair regression record](windows-repair-e2e-2026-11-17.json)
retains beta.2's original lock-fix evidence.

The historical [beta.1 verification record](windows-e2e-2026-11-17.json) links its
successful workflow commit and artifacts. Source-build and extracted-ZIP E2E both passed
on 2026-11-17, including actual deferred EXE deletion before repair. Windows and
Linux archive checksums and GitHub build attestations were independently verified.

Still required before a fully supported Windows desktop release:

- Windows 10 22H2 and Windows 11 clean, non-administrator desktop installation.
- Actual Windows Codex desktop chats across turns, app restarts and job-object
  shutdown, with the real model selecting Firefox when appropriate.
- Downloaded-file SmartScreen and Authenticode signing verification. The current
  helper EXE is unsigned; never disable Windows security to install it.
- Cross-user/session negative tests under real interactive accounts.
- Store/MSIX Firefox, ARM64, WSL and elevated/mixed-user use are not validated.

## Settings during update and removal

Beta.3 opens Settings in the Firefox profile that requested it. The page is
outside chat-owned work tabs. Its update/removal response completes even when
the native connection resets. Source-build and extracted-ZIP tests exercise that
boundary and management from a nonselected profile.

Firefox closes a native host's Windows Job when its native connection ends.
Only the Settings subprocess uses `CREATE_BREAKAWAY_FROM_JOB` so the bounded
local setup service can finish its response. Broker/chat lifetime and revocation
behavior are unchanged. This follows [Mozilla's native-app lifetime documentation](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_messaging#closing_the_native_app).
It does not establish the Windows Codex desktop app's Job lifetime behavior.

## Setup lock recovery

The [Linux 0.4.1 release-branch Windows workflow](https://github.com/nobadwordss/firefox-codex-mcp/actions/runs/44010285979)
failed at `--repair` after uninstall with a sharing violation (`os error 32`).
The cleanup worker could still hold `install.lock` with `FileShare.None` after
deleting an EXE. Opening that lock failed before Rust's `try_lock` ran.

Windows beta.2 waits up to ten seconds for both open-time sharing
violations and ordinary setup lock contention. It preserves exclusive ownership
throughout setup and reports a busy operation if the deadline expires. Permanent
open errors are returned immediately. The lock file is not deleted or replaced.
A sleeping cleanup worker exits if repair has already removed its cleanup journal.

The [repair verification record](windows-repair-e2e-2026-11-17.json) identifies the
fix commit, Windows Server CI and exact ZIP. Actual repair subprocesses are held
behind both types of Windows lock until contention is observed, then must finish
after release without changing unrelated settings. The full lifecycle and Firefox
E2E are repeated from the extracted ZIP. Earlier failure records remain historical.
The fix is included in the published beta.2 ZIP. Windows 10/11 desktop and
SmartScreen acceptance remain pending; the beta.1 package is unchanged.

Linux's advanced BiDi mode remains Linux-only. A Windows Server CI pass must never
be described as a Windows 10/11 desktop pass.
