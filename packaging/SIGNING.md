# Firefox extension signing and release

The `*-unsigned.xpi` in this package is a review/development artifact. It is NOT
an installable permanent add-on for standard Firefox. Do not present temporary
`about:debugging` loading as normal installation or as a persistent restart test.

## 0.4.2 signed Firefox management build

Mozilla approved Codex Work Tabs 0.4.2 for unlisted/direct distribution on
2026-11-17. It adds Settings in the trusted extension controls and simplifies
single-profile use. The extension ID and permissions are unchanged. The optional
native Settings control requires the matching new helper; old helpers show an
upgrade message. It is separate from MCP browser tools.
AMO version: 6720969; file: 5265052. Readable source, helper source and reproduction
instructions were provided for review.
Signed artifact: `codex-work-tabs-0.4.2.xpi`.
SHA256: `14e42f17f608cc975a47ba57b3cfde0f58998ce6a5e832eee362fce53352090f`.

Current source builds embed this artifact and compare all runtime files before
packaging. Helper 0.5.0-beta.3 remains unreleased; published Linux 0.4.1 and
Windows 0.5.0-beta.2 assets remain unchanged. Source/test approval is separate
from a public release or Windows 10/11 desktop acceptance.

## 0.4.1 signed English build

Mozilla approved Codex Work Tabs 0.4.1 for unlisted/direct distribution on
2026-11-17. It translates the extension controls, status and help to English.
The ID, permissions and native protocol are unchanged.
AMO version: 6720447; file: 5264530. An archive of the readable extension source,
privacy policy and reproduction instructions was provided for review.
Signed artifact: `codex-work-tabs-0.4.1.xpi`.
SHA256: `4a1685b820ef279e580b0a790540b165d529852b91e8e8631cbc8f61956d8ddc`.

Published Linux helper 0.4.1 and Windows preview beta.2 embed this artifact.
Linux helper 0.4.1 is the English stable release. Windows
remains a separate preview; Linux release status does not establish Windows desktop support.
Permanent installation through Firefox's actual confirmation controls, restart,
two profiles, chat continuity and irreversible revocation passed on Linux with
Firefox 160. The English clean-home model task also passed. See
[the verification record](https://github.com/nobadwordss/firefox-codex-mcp/blob/main/docs/english-default-e2e-2026-11-17.json).

## Historical 0.4.0 signed release

Mozilla approved Codex Work Tabs 0.4.0 for unlisted/direct distribution.
Extension ID: `{a171de16-96b6-4e50-94b7-e0b54d94436a}`.
AMO version: 6720041; file: 5264124. The original source ZIP was provided for review.
Signed artifact: `codex-work-tabs-0.4.0.xpi`.
SHA256: `a96a8c21d5007f5131ae3e0cb9f18547f8938ce36b234cec6abdfeb8eb72c6e2`.

The package builder compares every runtime file with the signed archive (manifest
JSON semantically, JavaScript/HTML/CSS and PNG icons byte-for-byte). Firefox 160 accepted permanent
installation and retained the add-on across restart without reinstalling it.
The published 0.4.0 and Windows beta.1 helper and installer embed this artifact.

This does not create a searchable AMO listing. There is no automatic extension
update URL: install a new signed XPI for future extension releases.
Source and releases: https://github.com/nobadwordss/firefox-codex-mcp

## Future extension updates

For each new extension version, the maintainer must:

1. Use their Mozilla Add-ons developer account and review/accept applicable terms.
2. Upload the included XPI at https://addons.mozilla.org/developers/addon/submit/.
   Choose listed distribution for marketplace discovery and automatic updates.
   Unlisted signing can produce a permanent install for a limited pilot, but does
   not by itself establish a marketplace listing or automatic update channel.
3. Provide PRIVACY.md, this repository's source, and reviewer notes below.
4. Download Mozilla's signed XPI. Retain the extension ID in manifest.json;
   changing it requires updating the native-host allowed_extensions as well.
5. Test the signed XPI on standard Firefox in a disposable profile: install via
   Firefox's visible add-on flow, accept the expected permissions, run MCP work,
   quit/reopen Firefox WITHOUT reloading/reinstalling the extension, and verify
   automatic reconnect with no old tab ownership restored. Repeat with two profiles.
6. Publish the verified signed artifact/listing link with the native helper package.
   Do not mark the new version ready before this gate passes.

Reviewer notes:
- No bundler, minifier, remote scripts, eval, telemetry, or external extension server.
- JavaScript in the XPI is the source. Build: `zip -X` with an explicit runtime file list.
- Native host: Rust executable, mode --native-host, permitted ID only, stdin/stdout
  framing. Local router uses a same-UID private Unix socket; no network listener.
- Native helper is needed once per OS user; the extension cannot install it.
- The AI can create HTTP(S) work tabs using that Firefox profile's existing logins.
  Content scripts are injected only into these tabs; user tabs are not adopted.
- Page text/title/URL/status/control descriptions pass to local Codex. Form input
  values are excluded; password/payment/OTP/file fields cannot be typed into.
- Popup pause/revoke and native disconnect release capabilities; result tabs remain open.
- Ordinary MCP EOF retains host-identified chats for up to 24 hours idle; anonymous
  clients remain connection-scoped. Reattachment fences old commands and snapshots.
  Revoked tabs are never reclaimed. See docs/chat-continuity.md.
- Supported release target: native Linux x86_64 Firefox 140+ and local Codex.
  Snap, Flatpak, Windows and macOS are not validated by this package.
