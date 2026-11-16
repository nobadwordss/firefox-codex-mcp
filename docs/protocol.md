# Bridge protocol v0.1

Transport: WebSocket `ws://127.0.0.1:8765/`. Single extension connection,
JSON text only. Server message/frame cap: 1 MiB; extension incoming cap: 256 KiB. Server verifies a `moz-extension://`
Origin and a shared random 256-bit token. Origin filtering alone is **not** identity
verification: a native process can forge Origin. Keep the token secret.

First client message: `{"type":"auth","token":"<64 hex characters>"}`.
On success: `{"type":"ready"}`. Five-second handshake/auth deadlines.
Token never appears in URL or logs. No command is sent before authentication.

Server command:
```json
{"id":"1","method":"snapshot","params":{}}
```
Other methods: `click`, `type`, `scroll`. See MCP tool schemas in `server/src/main.rs`
for the arguments. Response: `{"id":"1","result":{...}}` or
`{"id":"1","error":"message"}`. Commands run serially. Response deadline: 15 seconds.
A lost response or timeout is an **unknown action outcome**, not permission to retry.
The bridge closes that connection on failed/uncorrelated response.

The MCP side uses JSON-RPC 2.0, newline-delimited stdio and legacy protocol version
`2025-11-25`: initialize, ping, tools/list, tools/call. It is deliberately a small
subset, not a general MCP SDK; no resources, prompts, tasks, sampling or parallel
cancellation. stdout is protocol-only, stderr is diagnostics. The 2026 stateless
MCP revision is not implemented. Configure a client that supports the advertised
legacy version.

Page-derived content is untrusted. Neither text on a page nor a tool response
can authorize further actions. The integrating agent must enforce user approval
requirements; this prototype cannot determine whether a click is a purchase.
