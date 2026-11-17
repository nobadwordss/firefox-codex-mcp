# Firefox Codex MCP

**English** · [Korean](README.ko.md)

Let Codex open and use its own work tabs in your everyday Firefox profile, with
your existing website logins. A Firefox extension and a small local Rust helper
connect automatically: no pairing codes, tokens, profile paths, or per-tab setup.

![Codex Work Tabs cursor](extension/icons/cursor-96.png)

```text
Codex chat A ─ MCP stdio ─┐                       ┌─ A's work tabs
                         ├─ Local Rust router ─ Extension
Codex chat B ─ MCP stdio ─┘   Unix socket / Windows pipe  └─ B's work tabs
                              Native Messaging
```

## Install

Supported: **Linux x86_64**, ordinary Firefox 140+, and local Codex desktop/CLI.
The package runs without Rust or Node installed. It targets Ubuntu 22.04
(glibc 2.35) or newer. Windows has a separate preview below. macOS,
Snap/Flatpak Firefox and Android are not validated.

Current source builds use English setup, extension controls and diagnostics.
The published stable 0.4.0 archive retains its earlier Korean interface; it has
not been replaced. The English labels below describe the next preview build.
For an older download, follow its bundled `INSTALL.txt` for the matching labels.

1. Open Codex and check that you can sign in. Install ordinary desktop Firefox.
2. Download and fully extract the [0.4.0 Linux package](https://github.com/nobadwordss/firefox-codex-mcp/releases/tag/v0.4.0). Open a terminal in the extracted folder and run `./install.sh`. A file manager that supports running scripts can also launch it directly.
3. In the local manager, choose **Install connection**, then **Continue**. The installer registers MCP and installs the browser skill and guidance automatically.
4. Choose **Add Codex Work Tabs to Firefox** and confirm Firefox's installation and permission prompts.
5. Start a new Codex chat and ask: `Open https://example.com in the browser, click Learn more, and tell me the destination page title.`

A new Firefox work tab and a successful link click confirm the first connection.
Review any tool approval request for the task you asked for. If the script opens in
an editor, use the terminal command above. See the [first-install guide](docs/first-install-linux.md)
for troubleshooting and the exact fresh-environment verification boundary.

Install the helper once per OS user and the extension in
the Firefox profile you want to use. A single connected profile is selected
automatically. If several profiles connect, click **Use this Firefox**
in the intended profile. An offline selected profile never silently falls back to another.

The package includes a Mozilla-signed extension for permanent installation.
This is an **unlisted, direct distribution**, not an AMO search listing.
Current builds include `codex-work-tabs-0.4.1.xpi`. Use the signed XPI bundled with
your package; `*-unsigned.xpi` is only for development/review.
See [signing and release](packaging/SIGNING.md).

Global MCP registration, a Firefox skill and short browser guidance help Codex
consider Firefox when a real browser is useful. They do not force every model or
every web search to use it. Existing chats may need a new chat or an app restart;
project configuration and administrator policy take precedence. Cloud chats are
outside this local integration. Codex's built-in **Computer Use** list is not patched.

## Windows preview

The [0.5.0-beta.1 preview](https://github.com/nobadwordss/firefox-codex-mcp/releases/tag/v0.5.0-beta.1)
targets **Windows 10 22H2 / Windows 11 x64** with desktop Firefox. It is not yet a
fully validated Windows desktop release: automated verification uses Windows
Server 2022. See the exact [Windows verification scope](docs/windows-preview.md).

1. Install desktop Firefox and Codex from their official sources.
2. Download and extract the Windows ZIP, then double-click `firefox-codex-mcp.exe`. Do not run as administrator.
3. Choose **Install connection**, then **Continue** in the local English setup page.
4. Choose **Add Codex Work Tabs to Firefox** and confirm Firefox's prompt.
5. Start a new Codex chat and ask for a browser task.

No terminal, Rust, Node, pairing code, token or profile path is needed. The ZIP includes the EXE, dependency licenses and an SBOM. The helper is **not
Authenticode-signed**. The published beta.1 includes Mozilla-signed extension
0.4.0; current source builds prepare the English extension 0.4.1. We have not
validated SmartScreen's downloaded-file flow. Do not disable Windows security to
install the preview; if blocked, wait for signed delivery or review/build the source.

Update by extracting the newer ZIP and opening its EXE. Remove via **Windows Settings →
Apps → Firefox Codex MCP → Uninstall**, then **Remove connection** in the manager.
The manager exits after removal. Close old Codex chats to release locked program
files. Modified files and unrelated Codex settings are preserved; remove the
extension separately in Firefox. See [installation details](packaging/INSTALL.windows.txt).
BiDi remains Linux-only; WSL, Store Firefox, ARM64 and elevated/mixed-user operation
are outside this preview's validation scope.

## Continue the same tab across turns

In extension mode, work tabs belong to the host-supplied Codex chat identity, so
the next message can continue the same tab, including its form state. Replacing
the MCP process also preserves that chat's tabs while Firefox and the router remain
connected. A replacement connection invalidates old commands and snapshots; take
a fresh snapshot before continuing.

- Different chats cannot select or read each other's tabs.
- Existing user tabs, page-created popups and released tabs cannot be adopted.
- `firefox_revoke`, toolbar pause/revoke, Firefox/native connection loss, helper
  restart/update and profile changes permanently release existing access.
- After **24 hours without a request**, retained access expires. Up to 128 chat
  sessions and 8 work tabs per chat are retained in memory.
- Revocation leaves result tabs open. Reconnecting never restores their ownership.
- Clients without host `threadId` metadata keep connection-only ownership;
  `firefox_status` reports `continuity: "chat"` or `"connection"`.

Identity comes from MCP request metadata, not a tool argument, title, working
directory or model-provided resume token. This is logical isolation between MCP
chats, **not a sandbox against malicious programs running as the same OS user**.
Stopping generation or closing/archiving a Codex chat is not a reliable immediate
revocation signal;
use the explicit revoke or pause control when you want access to end.

## Tools and limits

| Tool | Action |
|---|---|
| `firefox_status` | Connection, continuity mode and this chat's work-tab count |
| `firefox_open_tab` | Create and select a new background HTTP(S) work tab |
| `firefox_tabs`, `firefox_select_tab` | List/select only this chat's retained tabs |
| `firefox_navigate` | Navigate an owned tab |
| `firefox_snapshot` | Read page text, title, status and controls |
| `firefox_click`, `firefox_type`, `firefox_scroll` | Act using a fresh snapshot |
| `firefox_close_tab` | Close the selected owned tab |
| `firefox_revoke` | Release this chat's access without closing its result tabs |

Snapshots are single-use for actions and expire after 30 seconds. Changes to the
target, context, state, modal or viewport invalidate unsafe actions; unrelated DOM
updates are allowed. Navigation keeps tab ownership but requires a fresh snapshot.
Already-dispatched actions cannot be undone by revocation. If an action times out,
inspect the outcome before retrying to avoid duplicate submissions.

Work tabs use the profile's existing logins. Every local Codex chat with this MCP
enabled can create a work tab, subject to the user's task scope and normal approvals.
Page text can contain private information; see [privacy](PRIVACY.md). Pages are
untrusted data, not instructions. Form input buffers are excluded from snapshots;
a successful type result alone is not independent input readback. Iframe editors,
file upload and trusted native input are not implemented.

If Firefox is closed, the helper tries its ordinary launcher. When its default
profile differs from the selected profile, open the intended profile yourself.

## Update and remove

Run the new package's `install.sh` and choose **Update connection**. Firefox stays
running, but helper replacement releases old work-tab access. Install the matching
signed XPI to update the extension, then use a fresh Codex chat. Direct-distribution
XPIs currently have no automatic update channel.

The app-menu entry **Firefox Codex Manager** provides diagnosis, recovery and removal.
Removal targets registered MCP settings, installed files and managed blocks while
preserving user edits, Firefox profiles, logins and result tabs. Remove the Firefox
extension separately in Add-ons Manager. Local receipts and recovery state live
under `~/.local/state/firefox-codex-mcp/`.

```bash
./install.sh --install          # default extension connection
./install.sh --update           # preserve the installed connection mode
./install.sh --doctor
./install.sh --uninstall
./install.sh --repair
./install.sh --install-bidi     # advanced: debug connection/profile discovery
./install.sh --install-bidi /absolute/profile
```

WebDriver BiDi remains an advanced option with connection-scoped ownership. Only
that mode requires the **Firefox (Codex connection)** debug launcher. To migrate a BiDi
installation, choose the manager's default extension installation.

## Development and verification

Our focus is predictable Codex chat ownership, continuity and revocation with
reproducible checks. See the [comparison plan](docs/comparison-plan.md) for related
projects and fair test criteria; no comparative performance or security advantage
has been measured.

Requires Rust 1.89+, Node 24+, and `zip`, `unzip`, `tar` for packaging.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
node --test --test-isolation=none extension/tests/*.test.cjs
cargo build --release --locked
FIREFOX_SETUP_CODEX_BINARY=/path/to/codex cargo test --release --test install_e2e -- --ignored
FIREFOX_SIGNED_XPI="$PWD/packaging/signed/codex-work-tabs-0.4.1.xpi" FIREFOX_SIGNED_UI=1 FIREFOX_GECKODRIVER=/path/to/geckodriver cargo test --release --test firefox_e2e native_e2e:: -- --ignored --nocapture
cargo run --release --locked --bin package -- target/release/firefox-codex-mcp dist/release
```

Automated Firefox E2E uses isolated HOME directories and disposable profiles.
`--allow-system-access` / `MOZ_REMOTE_ALLOW_SYSTEM_ACCESS=1` are limited to those
Firefox UI tests. Without `FIREFOX_SIGNED_XPI`, tests explicitly reload a temporary
development add-on after restart and do not claim permanent installation.

See [connection architecture](docs/extension-mode.md), [continuity design](docs/chat-continuity.md),
[0.4.0 desktop and Firefox results](docs/chat-continuity-e2e-2026-11-17.json)
and [validation](docs/validation.md). Legacy pairing UI stays only for regression
tests and is excluded from release XPIs.

Release packages include the helper, installer, signed XPI, MIT license, dependency
licenses, CycloneDX SBOM and SHA256SUMS. Public GitHub CI also produces build
provenance. Checksums alone are not signatures. [MIT license](LICENSE).
