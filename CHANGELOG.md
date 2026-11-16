# Changelog

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
