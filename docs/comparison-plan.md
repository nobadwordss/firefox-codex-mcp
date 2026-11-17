# Product direction and reproducible comparison

Reviewed 2026-11-17. This is a comparison **plan**, not a benchmark result.
We have not installed or executed the competing tools in this comparison.

## What we should earn

The product focus is a straightforward Codex setup and predictable work tabs:
isolation by chat, continuation across turns/MCP replacement, and irreversible
revocation. Native Messaging, logged-in Firefox access, and tab ownership are not
unique ideas. Rust alone does not establish a speed or security advantage.
Publish a comparative claim only with measurements that readers can reproduce.

## Documented alternatives

| Project | What its own documentation describes | Evidence status |
| --- | --- | --- |
| [Foxhand](https://github.com/hamb1y/foxhand) | Native Messaging, Codex configuration, Windows/macOS/Linux helper installation, iframe-aware observation, uploads and rich-editor input | README review only |
| [firefox-bridge](https://github.com/raychao-oao/firefox-bridge) | Signed extension, Codex integration, tab leases, native Windows and WSL setup | README review only |
| [firefox-mcp](https://github.com/dh1011/firefox-mcp/blob/main/docs/how-it-works.md) | Per-app tab groups, connection approval/stop controls and selection of existing tabs | Design-document review only |
| [Computer Use MCP](https://github.com/mirsella/computer-use-mcp/tree/b373e68e6273217d549097efdfbbee0b9fbd47d4) | KDE Wayland desktop control through AT-SPI, portals, PipeWire and EIS; foreground/private-background workers; packaged Rust executable, Codex plugin manifests and skill | Documentation and selected packaging/integration source reviewed at `b373e68e6273217d549097efdfbbee0b9fbd47d4` (2.2.0); not installed or executed |

Do not infer that a competitor lacks cross-turn continuity merely because a
README does not describe it. Record absent documentation as **unknown**.
Our narrower refusal to adopt existing user tabs is a deliberate product policy;
broader tools address different workflows. It is not a blanket security ranking.

## Reference review: Computer Use MCP

Reviewed the pinned [README](https://github.com/mirsella/computer-use-mcp/blob/b373e68e6273217d549097efdfbbee0b9fbd47d4/README.md),
[MCP contract](https://github.com/mirsella/computer-use-mcp/blob/b373e68e6273217d549097efdfbbee0b9fbd47d4/MCP.md),
[security boundaries](https://github.com/mirsella/computer-use-mcp/blob/b373e68e6273217d549097efdfbbee0b9fbd47d4/SECURITY.md)
and [architecture](https://github.com/mirsella/computer-use-mcp/blob/b373e68e6273217d549097efdfbbee0b9fbd47d4/ARCHITECTURE.md)
on 2026-11-17. Its reported smoke-test results remain the author's evidence;
we did not reproduce them. Useful ideas for our installation work are:

- Bundle discovery and guidance. Its [package manifest](https://github.com/mirsella/computer-use-mcp/blob/b373e68e6273217d549097efdfbbee0b9fbd47d4/package.json)
  ships the native executable, MCP/plugin manifests and skill together. Consider
  an optional Codex plugin path after verifying host support. Our RPM/DEB still
  needs Firefox's native-host registration and per-account Connect; a plugin
  cannot replace those requirements.
- Diagnose before requesting access. Its documented `doctor` checks prerequisites
  and session binding without portal consent, while `init` requests approval.
  Keep our setup diagnostics read-only and distinguish configuration checks from
  a successful first web task, with a concrete recovery step for each failure.
- Recover a missing graphical environment narrowly. Its [host entrypoint](https://github.com/mirsella/computer-use-mcp/blob/b373e68e6273217d549097efdfbbee0b9fbd47d4/host/mcp-server)
  fills only missing allowlisted values from the user service manager, bounds
  the lookup to five seconds and replaces the shell with the server. This is a
  reference for testing Firefox launch from service-started hosts, not a reason
  to import the entire environment or alter the user's session.
- Keep authority and recovery explicit. The documents describe generation-bound
  IDs, retiring revoked workers and distinguishing blocked dispatch from uncertain
  outcomes. Compare those boundaries with our revoke/reconnect tests; never
  automatically replay an action whose effect is unknown.

Its desktop/session routing and portal approvals are different authorities from
our Firefox chat-owned work tabs. AT-SPI access can occur without a portal prompt;
portal consent covers a session rather than each call. Private desktop routing
is documented as cooperative same-user separation. Per-chat browser continuity,
setup usability, performance and comparative security remain unmeasured here.
This review does not expand our product into desktop-wide control.

## Equal test conditions

Pin every repository commit/release, extension package, OS, Firefox and Codex
version. Use separate clean OS accounts or disposable VMs/profiles, identical
local fixture pages, the same Codex host/model settings and no real personal data.
Run each documented installation path without helper scripts that hide manual
steps. Do not install competing integrations into the developer's daily profile.

| Scenario | Record and independently verify |
| --- | --- |
| First install | Time from download to first successful read; prompts, commands, copied values, restarts, failures and recovery steps. Separate download time from interaction time. |
| Two chats | A and B create separate tabs; B tries A's tab ID and stale references. Check whether reads/writes are denied. |
| Existing user tabs | Before connecting, open a synthetic personal tab. Check whether its URL/title appears in tool responses and whether read/write is possible without selection or permission. Report the product's intended policy separately from enforcement bugs. |
| Continuation | Preserve a typed value and counter across a new turn, MCP replacement, host restart and Firefox restart. Report each boundary separately; intentional access revocation is not a continuity bug. |
| Revocation | Revoke while attached, detached and during a queued action; reconnect the old chat and try the old tab/reference. Distinguish already-dispatched actions from new requests after revocation. |
| Multiple profiles | Select one profile, disconnect it, and check whether another profile is selected silently. |
| Dynamic content | Replace a target node, show a modal, change status text and mutate unrelated content. Check incorrect actions, stale-reference rejection and required rereads. |
| Iframes/editors | Same-origin and cross-origin frames, contenteditable and a pinned rich-editor fixture. Verify rendered value and input events independently. Mark unsupported features explicitly. |
| Update/removal | Update with active chats; revoke old access; remove with locked EXEs; preserve unrelated settings and user edits. Record leftovers and required manual steps. |

Measure action latency only after semantic correctness, under the same load and
page state, reporting sample count, median and p95. Measure setup usability with
first-time users, rather than treating an expert's automated run as a user study.
Use screenshots, fixture event logs and exact error responses; an MCP success
response alone is not proof of the browser effect. Publish failures as well as
passes, with unsupported/not tested/not applicable distinct from failed.

Current project evidence is linked from [validation](validation.md) and
[Windows preview](windows-preview.md). These are our own integration results,
not side-by-side comparative results. Windows Server CI is not Windows 10/11
desktop acceptance, and direct Codex app-server dispatch is not model-driven UI
verification.
