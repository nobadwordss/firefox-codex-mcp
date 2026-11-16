# Bridge protocol v0.2

This document describes the legacy extension transport. The default is now the
[direct WebDriver BiDi broker](easy-connection-design.md), which creates AI work
tabs without extension pairing or manual tab grants.

Transport: WebSocket `ws://127.0.0.1:<OS-assigned-port>/`. Each stdio MCP process
binds port 0 and generates a fresh 128-bit lowercase hex `clientId`. One authenticated
extension connection per process; the extension can hold up to 16 independent clients.
JSON text only. Server message/frame cap: 1 MiB; extension incoming cap: 256 KiB. Server verifies a `moz-extension://`
Origin and a shared random 256-bit token. Origin filtering alone is **not** identity
verification: a native process can forge Origin. Keep the token secret.

Authentication is mutually verified HMAC-SHA256:

1. Server: `{"type":"challenge","version":2,"clientId":"<32 hex>","challenge":"<64 hex>"}`.
2. Extension: `{"type":"auth","clientId":"<32 hex>","proof":"<64 hex>"}`.
3. Server: `{"type":"ready","clientId":"<32 hex>","proof":"<64 hex>"}`.

The HMAC key is the ASCII lowercase 64-hex-character token. Its input is exactly
`firefox-codex-mcp/v2/${role}\n${clientId}\n${challenge}`, with role `extension` or
`server`. A new cryptographic challenge is generated per connection. The extension
checks the clientId against the manually entered connection code before proving its
token, and waits for server-proof verification before handling application messages.
There are five-second handshake/auth deadlines. The token itself never appears in
the URL, logs, MCP responses or WebSocket frames. Old v0.1 peers are incompatible;
reload both server and extension when upgrading. Port reuse cannot authorize a
different clientId.

Server command:
```json
{"id":"1","method":"snapshot","params":{}}
```
Other methods: `status`, `click`, `type`, `scroll`. See MCP tool schemas in `server/src/main.rs`
for the arguments. Response: `{"id":"1","result":{...}}` or
`{"id":"1","error":"message"}`. Commands run serially. Response deadline: 15 seconds.
A lost response or timeout is an **unknown action outcome**, not permission to retry.
The bridge closes that connection on failed/uncorrelated response.
The server tags queued commands with its authenticated connection generation;
commands left over from a previous connection are rejected before dispatch.

`status` takes empty params and is handled by the authenticated background,
without querying any tab or content script. It returns `connected`, `tabEnabled`,
`tabId` (null without a grant) and `nextStep`. When disconnected, the MCP server
returns both booleans false locally. MCP status also includes this process's public
`clientId` and `connectionCode` (`port:clientId`). It never infers permission from connectivity.

## Extension grant lifecycle

The toolbar popup is the only grant/configuration entry point. Its selected client
only selects which connection to configure; it never routes incoming MCP commands.
Each connection closes over its own socket, token, session, generation and grant.
Enabling a tab revokes that client's previous grant synchronously, then checks ownership after every
asynchronous browser API call. Pending grants are also cancelled by tab closure,
navigation or disconnection. Late failures from an earlier enable attempt cannot
clear its replacement. A tab is synchronously reserved before script injection;
another client cannot enable an active or pending owned tab. Revoke the current
owner explicitly before assigning it to another client. A connection's revoke or
failure cannot clear another client's reservation. See [multi-chat design](multi-chat-design.md).

The background connects to the top-frame content script with a new
`runtime.Port` named `firefox-codex-mcp/grant-v1`. Before dispatching commands:

1. Content sends `hello` with its random `documentId`.
2. Background sends `grant` with `{clientId, sessionId, grantId, documentId}`. `sessionId`
   changes on every authenticated WebSocket; `grantId` changes on every enable.
3. Content clears its snapshot and replies `ready` with the same context.
4. Only then does background publish the active grant. Handshake deadline: 5s.

Port commands and replies carry `kind`, `id` and this context. Background checks
the active grant immediately before dispatch and after receiving a reply.
Content validates channel identity and all four IDs before execution. Snapshot
results include the context for diagnostics; MCP action arguments still use
`snapshotId` and `elementId`. Every action validates the stored snapshot's
context, ID, age, URL, viewport and relevant page/target state, then consumes it
before changing the page. A DOM revision change alone no longer invalidates it.
An old action cannot use a new grant, even within the same WebSocket session.

The popup's **Revoke tab access** closes active/pending ports while keeping the
authenticated WebSocket alive. Reading or acting then fails until a new explicit
enable. Old snapshots are invalid even after re-enabling the same document.
Disconnect also closes active and pending ports, clears ownership and increments the
generation before settling asynchronous work. A lost reply after dispatch is an
unknown outcome, even if the action really executed. The page response deadline
is 14s; timeout revokes the grant and closes that WebSocket. Commands are never
automatically replayed. Delayed old-port disconnect events cannot clear a new
grant. `pagehide` clears the content channel/snapshot; `pageshow` does not restore
permission. Current policy also revokes on URL-only changes such as SPA routing.

Revocation fences new dispatch. It cannot undo an action already dispatched to
the page. Inspect unknown outcomes before retrying.

## Observation and action freshness

`firefox_snapshot` returns structured JSON with these fields:

| Field | Meaning / bound |
| --- | --- |
| `snapshotId`, `context` | Single-use observation and grant identity |
| `title`, `url`, `readyState` | Document metadata (200 / 2048 characters for title / URL) |
| `viewport` | Width, height, scrollX, scrollY |
| `observation` | Informational DOM revision and `maxAgeMs: 30000` |
| `text` | Rendered body text, up to 12,000 UTF-16 code units |
| `headings` | Up to 30 visible headings, 500 characters each, with level |
| `statuses` | Up to 30 live/status/alert/dialog regions, 1,000 characters each, with role/live/busy/truncated |
| `elements` | Up to 200 controls, with id/tag/role/type/label and state |
| `truncation`, `truncated` | Per-section and overall collection limits |

Control state includes disabled/readOnly/busy and applicable checked/expanded/
selected/pressed/invalid/required values. `actions` lists supported operations;
disabled, busy and native select controls have none. Select options are not
exposed or changed; native picker clicks are rejected rather than reported as
successful. Input values are never returned. Internal target fingerprints retain
the target's current value only to refuse overwriting an intervening edit.

The text collector skips entire script/style, hidden/inert/aria-hidden and editing
subtrees (input, textarea, select, contenteditable, ARIA textbox/combobox). It
checks Range fragments against the viewport, overflow clipping and hit testing,
subdividing partial ranges so an offscreen tail of the same text node is excluded.
Each text extraction has limits of 10,000 nodes, 60,000 source characters and
2,048 Range checks. Candidate scans stop at 2,000. Labels and referenced
descriptions may use offscreen naming text, but still exclude hidden/edit buffers.
This is DOM-based, best-effort visibility, not a pixel-perfect accessibility tree.
CSS clip/clip-path content is conservatively omitted, and unusual fixed-position
descendants inside overflow containers may also be omitted. Iframes, shadow DOM,
canvas, pseudo-element text and arbitrary site-specific secret detection are absent.

Immediately before each action, without an asynchronous yield, content checks:

1. The latest snapshot's grant, ID, URL, age (at most 30 seconds) and unchanged
   viewport. Scroll/resize events invalidate it, including nested scrolling.
2. Observed live/status/dialog content and modal node identity. New, changed or
   truncated status/dialog observations require re-observation.
3. For click/type, the same connected target node, enabled/non-busy state,
   visibility and an unobstructed visible rectangle fragment. Wrapped inline
   links use individual fragments rather than the union rectangle's center.
4. Target label/state/link destination/form destination/description and target
   input buffer, unchanged surrounding semantic container and its rendered text
   (up to 4,000 characters), and geometry within 2 CSS pixels.

Ordinary clock/animation mutations elsewhere do not block an unchanged target.
Changes inside its observed context, continuous live-status updates, very large
contexts or ongoing layout motion can still reject actions conservatively. These
checks do not prove unchanged JavaScript event-handler behavior. After success,
scroll, expiration or a pre-dispatch rejection, observe again. An unknown outcome
must be inspected before retrying; there is no automatic action replay.

## MCP boundary

The MCP side uses JSON-RPC 2.0, newline-delimited stdio and legacy protocol version
`2025-11-25`: initialize, ping, tools/list, tools/call. It is deliberately a small
subset, not a general MCP SDK; no resources, prompts, tasks, sampling or parallel
cancellation. stdout is protocol-only, stderr is diagnostics. The 2026 stateless
MCP revision is not implemented. Configure a client that supports the advertised
legacy version.

Page-derived content is untrusted. Neither text on a page nor a tool response
can authorize further actions. The integrating agent must enforce user approval
requirements; this prototype cannot determine whether a click is a purchase.
