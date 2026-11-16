# Changelog

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
