# Changelog

## 0.5.0-beta.2 — Windows preview (unreleased)

- Wait up to ten seconds for Windows setup/cleanup locks, including exclusive sharing handles from older cleanup workers, before reporting a busy operation.
- Exit obsolete cleanup workers when repair has already removed their cleanup journal.
- Add deterministic repair-process tests for cleanup sharing locks and installer byte-range locks, plus timeout and permanent-error coverage.
- Prepare the English extension and guidance for the next Windows preview.
- Keep Windows desktop and SmartScreen validation pending. Linux English defaults ship separately as stable 0.4.1.

## 0.4.1 — English Linux release — 2026-11-17

- Use English for Linux setup, CLI help, diagnostics, launchers and connection errors.
- Use English for the Codex Work Tabs 0.4.1 extension controls and status messages.
- Make installation guides English by default and retain separate Korean translations.
- Verify the installed defaults with an English model request in an isolated home.
- Publish Linux x86_64 as a stable release with English installation instructions.
- Keep earlier release files intact. Windows remains a separate preview; this release publishes no Windows installer.

## 0.5.0-beta.1 — Windows preview

- Add Windows local Named Pipes with logon-session ACLs, remote-client rejection,
  peer token checks and exclusive broker creation. Keep the common chat protocol.
- Add a double-click EXE and English manager for per-user install/update/removal.
- Register Native Messaging in HKCU and Codex globally without profile paths,
  tokens, a CLI dependency, elevation or browser debugging.
- Use immutable versioned EXEs, journaled file/registry changes, conflict-preserving
  recovery and bounded deferred deletion of unchanged program files.
- Add Windows native Firefox and official Codex app-server integration tests,
  extracted-package testing and Windows ZIP artifacts with license notices.
- Keep the Mozilla-signed extension at 0.4.0 and retain Linux-only advanced BiDi.
- Windows 10/11 desktop, live model/desktop UI and Authenticode/SmartScreen validation
  are pending. Windows Server CI is not a substitute for those checks.

## 0.4.0 — 2026-11-17

- Preserve work tabs across turns and MCP process replacement using Codex host
  thread metadata; clients without that metadata retain connection-only scope.
- Fence old connections, queued work, late EOF and stale snapshots. Explicit
  revocation never restores earlier tabs, including revocation while detached.
- Expire inactive chat ownership after 24 hours; keep user result tabs open.
- Add actual Firefox process-replacement, form-state, isolation and restart tests.
- Add the rounded, tail-less cursor icon with a white outline.
- Make English the default README and retain a linked Korean edition.

## 0.3.1 — 2026-11-17

- Fix Firefox/Codex desktop connection splitting when only Firefox inherits
  XDG_RUNTIME_DIR. Extension clients now share a private HOME-based endpoint.
- Reconnect both old and new router locations during migration; keep Firefox running.
- Reproduce the actual environment mismatch in signed Firefox E2E instead of
  masking it with a shared test-only runtime override.
- Native-mode diagnostics report browser/debugger state as unknown rather than
  incorrectly claiming that Firefox is stopped.
- Add a browser-only desktop fixture with independent local DOM event evidence.
- Actual personal-profile Codex desktop E2E passed in three new chats, including
  automatic tool selection, independent input readback and simultaneous isolation.
- Document ownership reset when the desktop recreates MCP connections across turns.
- Document Windows 10/11 transport, installer and actual desktop validation requirements.
- The Mozilla-signed extension remains unchanged at 0.3.0.

## 0.3.0 — 2026-11-17

- Native Messaging extension is the default; no pairing codes, tokens or profile paths.
- Mozilla-signed Codex Work Tabs installs permanently from the local management screen.
- Per-chat work tabs and explicit profile selection isolate concurrent chats and profiles.
- Disconnect, restart, pause and profile switches revoke old tab access.
- Helper updates reconnect without restarting Firefox; BiDi remains an advanced option.
- Actual Firefox tests cover signed installation, restart, two profiles, two chats,
  revocation, live helper update and independently verified Codex CLI actions.
- Public Linux packages include signed/unsigned XPI, MIT license, SBOM and checksums.

Limits: native Linux x86_64 only; unlisted signed extensions need manual updates;
no automatic adoption of existing tabs; a selected non-default profile may need to
be opened manually. This version’s new connection was tested with Codex CLI, not
a fresh Codex desktop UI session.

## 0.2.0 — 2026-11-16

- Firefox WebDriver BiDi work tabs replace manual extension pairing as the default.
- A shared Rust broker gives each MCP connection its own work tabs and revocation scope.
- One local management screen installs, diagnoses, updates, repairs and removes the integration.
- User-level Codex registration includes browser discovery instructions and a Firefox skill.
- Installation uses a lock, journal, atomic replacement and recovery; removal preserves user edits.
- Broker protocol negotiation rejects incompatible updates without stopping Firefox.
- Linux x86_64 packages include MIT license, dependency notices, CycloneDX SBOM and checksums.
- Tests cover clean user setup, update/removal, interrupted installation, actual Firefox actions,
  concurrent chat isolation and revocation. Native Codex Computer Use list integration remains deferred.

Known limits: ordinary Linux Firefox only; saved-work restart if it was started without debugging;
local Codex chats only; iframe/native input/file transfer unsupported; forced broker crashes may
require Firefox restart. Existing chats keep their process until closed. Public Release publication
is separate from the CI artifact build.
