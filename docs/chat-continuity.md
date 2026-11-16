# Chat continuity and revocation

## Identity and scope

Codex supplies `params._meta.threadId` outside model-generated tool arguments.
The adapter accepts a UUID-shaped value, normalizes and hashes it, and pins that
identity on the first **tools/call**, not initialize or tools/list. Later changes
or malformed metadata fail closed. No model-facing tool accepts a chat ID or
resume token. Missing thread metadata uses a fresh connection-scoped owner.

The field is verified in the actual desktop app and in OpenAI's
[MCP call implementation](https://github.com/openai/codex/blob/main/codex-rs/core/src/mcp_tool_call.rs)
(`with_mcp_tool_call_ids_meta`). A read-only Rust probe in `server/examples` reports
only hashes, process ID and the presence of the thread environment variable.
Two chats in the same project produced distinct hashes; two turns of one chat
kept the same hash. The process was reused in that probe: recreation is possible,
not something we assume happens on every turn.

This metadata is not a cryptographic attestation. The local transport checks the
OS user, and a malicious program with the same user's privileges can impersonate
a client. It provides chat isolation within the normal Codex/MCP execution path.

## Owner versus attachment

The router keeps a memory-only `identity digest -> random owner` mapping. Each
MCP connection receives a new random attachment. Replacing an attachment cancels
the old dispatcher and immediately suspends its extension attachment, even if
the replacement has not sent its first tool request. EOF cleanup checks that it still owns the current attachment,
so a delayed EOF cannot suspend the next connection.

The extension receives no Codex thread IDs. It owns the authoritative tab map and
accepts commands only for a registered attachment. Reattaching keeps tab IDs and
page/form state, replaces the owner object, closes content ports and invalidates
snapshots. Queued operations retain the old object and fail its guard. A fresh
snapshot must bind to the current document before another click/type/scroll.

Normal MCP EOF and process crashes both suspend retained work; EOF does not reveal
the reason a process ended. The router expires ownership after 24 hours without a
request, checked once a minute, and caps retained chats at 128 and tabs at 8/chat.
Stopping generation, closing or archiving a chat is not a reliable immediate
revocation signal. Use explicit revoke/pause when access must end.

## Revocation is irreversible

`firefox_revoke` and toolbar revoke/pause delete ownership in the extension. The
router may retain an identity mapping, but the next attachment has no old tabs to
recover. No disk tab registry, URL search or existing-tab adoption is used.
Native disconnect, Firefox/helper restart or update, profile switch and permission
removal also discard ownership. Results remain open for the user.

Awaited tab creation checks the captured owner again before registering the result.
If revoked or replaced mid-creation, the new tab is left unowned. Already-dispatched
browser actions cannot be undone: an unknown result is reported and never blindly
replayed. The next turn must inspect the page. Content commands keep the existing
document/grant/snapshot validation and single-use snapshots.

## Verification contract

- Same chat, separate desktop turns: same tab ID and document state, fresh read and action.
- Same host identity, replaced MCP process: form state retained; old client fenced.
- Different chat: selecting the other tab denied before page access.
- Old snapshot and late EOF: neither can affect a replacement connection.
- Explicit revoke followed by another turn/process: old tab cannot be selected.
- Toolbar revoke during a detached interval: reconnect starts with zero owned tabs.
- One chat's revoke leaves the other chat operational.
- Firefox restart retains the signed add-on, but never restores old tab ownership.

BiDi remains connection-scoped. Windows 10/11 still requires the separate
[platform port and actual VM tests](windows-support-plan.md).
