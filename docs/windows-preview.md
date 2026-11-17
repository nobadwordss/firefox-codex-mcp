# Windows preview: installation and verification boundary

The 0.5.0-beta.1 helper targets Windows 10 22H2 and Windows 11 x64 with ordinary
desktop Firefox 140+. The extension stays at Mozilla-signed version 0.4.0.
This is a preview, not a claim that Windows 10/11 desktop acceptance testing is complete.

## Install, update and remove

Download the Windows setup EXE from the project's release and double-click it as
your normal Windows user. Choose **Install connection → Continue**, then **Add
Codex Work Tabs to Firefox**. Confirm Firefox's normal extension installation.
Start a new Codex chat. No administrator rights, code, token, terminal, profile
path, Rust or Node installation is required. The installer can register desktop
Codex even when its command-line executable is not on PATH.

The manager falls back to your default browser if it cannot locate Firefox. Install
desktop Firefox from Mozilla, then reopen setup in Firefox to install the extension.
The Firefox extension controls still use Korean: **이 Firefox 사용** selects that
profile, **잠시 중지** pauses control, and **모든 작업 탭 접근 해제** revokes all work tabs.

Download and open a newer setup EXE to update. Executing an older downloaded EXE
installs that version, so choose the intended release. Windows Settings → Apps →
Firefox Codex MCP → Uninstall opens the manager; choose **Remove connection**.
The manager exits after a successful removal. Remove the Firefox extension
separately if desired. Profiles, logins and visible result tabs remain intact.

Running Codex chats can keep old EXEs locked. Removal retries unchanged payload
deletion for up to four minutes. Close those chats promptly; if a file is still
locked, run a downloaded setup EXE with `--repair` after closing them. Recovery
metadata and profile-selection state may remain under the user installation root.
Changed files/registry values are preserved. Reinstalling cancels an old cleanup
worker; it never recursively deletes the installation directory.

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

The release verification record links the exact successful workflow commit and
its artifacts. Read that result before treating any listed scenario as passed.

Still required before a fully supported Windows desktop release:

- Windows 10 22H2 and Windows 11 clean, non-administrator desktop installation.
- Actual Windows Codex desktop chats across turns, app restarts and job-object
  shutdown, with the real model selecting Firefox when appropriate.
- Downloaded-file SmartScreen and Authenticode signing verification. The current
  helper EXE is unsigned; never disable Windows security to install it.
- Cross-user/session negative tests under real interactive accounts.
- Store/MSIX Firefox, ARM64, WSL and elevated/mixed-user use are not validated.

Linux's advanced BiDi mode remains Linux-only. A Windows Server CI pass must never
be described as a Windows 10/11 desktop pass.
