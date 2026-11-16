# Changelog

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
