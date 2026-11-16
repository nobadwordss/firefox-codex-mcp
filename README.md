# Firefox Codex MCP

**English** · [한국어](README.ko.md)

Let Codex open and use its own work tabs in your everyday Firefox profile, with
your existing website logins. A Firefox extension and a small local Rust helper
connect automatically: no pairing codes, tokens, profile paths, or per-tab setup.

![Codex Work Tabs cursor](extension/icons/cursor-96.png)

```text
Codex chat A ─ MCP stdio ─┐                       ┌─ A's work tabs
                         ├─ Local Rust router ─ Extension
Codex chat B ─ MCP stdio ─┘   private Unix socket  └─ B's work tabs
                              Native Messaging
```

## Install

Supported: **Linux x86_64**, ordinary Firefox 140+, and local Codex desktop/CLI.
The package runs without Rust or Node installed. It targets Ubuntu 22.04
(glibc 2.35) or newer. Windows, macOS, Snap/Flatpak Firefox and Android are not yet
validated. [Windows 10/11 porting plan](docs/windows-support-plan.md).

1. Download the [0.4.0 Linux package](https://github.com/nobadwordss/firefox-codex-mcp/releases/tag/v0.4.0), extract it, and run `install.sh`.
2. In the local manager, choose **설치 (Install)**, then **진행 (Continue)**.
3. Choose **Firefox에 Codex Work Tabs 확장 추가 (Add Codex Work Tabs to Firefox)** and confirm Firefox's installation and permission prompts.
4. Start a new Codex chat and request an interactive web task. Codex can create its own work tab.

The manager and extension controls currently use Korean; the English labels above
describe those controls. Install the helper once per OS user and the extension in
the Firefox profile you want to use. A single connected profile is selected
automatically. If several profiles connect, click **이 Firefox 사용 (Use this Firefox)**
in the intended profile. An offline selected profile never silently falls back to another.

The package includes a Mozilla-signed extension for permanent installation.
This is an **unlisted, direct distribution**, not an AMO search listing.
Use `codex-work-tabs-0.4.0.xpi`; `*-unsigned.xpi` is only for development/review.
See [signing and release](packaging/SIGNING.md).

Global MCP registration, a Firefox skill and short browser guidance help Codex
consider Firefox when a real browser is useful. They do not force every model or
every web search to use it. Existing chats may need a new chat or an app restart;
project configuration and administrator policy take precedence. Cloud chats are
outside this local integration. Codex's built-in **Computer Use** list is not patched.

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

Run the new package's `install.sh` and choose **업데이트 (Update)**. Firefox stays
running, but helper replacement releases old work-tab access. Install the matching
signed XPI to update the extension, then use a fresh Codex chat. Direct-distribution
XPIs currently have no automatic update channel.

The app-menu entry **Firefox Codex 관리** provides diagnosis, recovery and removal.
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
that mode requires the **Firefox (Codex 연결)** debug launcher. To migrate a BiDi
installation, choose the manager's default extension installation.

## Development and verification

Requires Rust 1.89+, Node 24+, and `zip`, `unzip`, `tar` for packaging.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
node --test --test-isolation=none extension/tests/*.test.cjs
cargo build --release --locked
FIREFOX_SETUP_CODEX_BINARY=/path/to/codex cargo test --release --test install_e2e -- --ignored
FIREFOX_SIGNED_XPI="$PWD/packaging/signed/codex-work-tabs-0.4.0.xpi" FIREFOX_SIGNED_UI=1 FIREFOX_GECKODRIVER=/path/to/geckodriver cargo test --release --test firefox_e2e native_e2e:: -- --ignored --nocapture
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
