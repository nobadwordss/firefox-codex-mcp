# Firefox extension signing and release

The `*-unsigned.xpi` in this package is a review/development artifact. It is NOT
an installable permanent add-on for standard Firefox. Do not present temporary
`about:debugging` loading as normal installation or as a persistent restart test.

## 0.3.0 signed release

Mozilla approved Codex Work Tabs 0.3.0 for unlisted/direct distribution.
Extension ID: `{a171de16-96b6-4e50-94b7-e0b54d94436a}`.
AMO version: 6719847; file: 5263930. The original source ZIP was provided for review.
Signed artifact: `codex-work-tabs-0.3.0.xpi`.
SHA256: `c5b33190fbacb1b05776156c0f8064624e4a53390089674b2ce3a853bad8a188`.

The package builder compares every runtime file with the signed archive (manifest
JSON semantically, JavaScript/HTML/CSS byte-for-byte). Firefox 160 accepted permanent
installation and retained the add-on across restart without reinstalling it.
The helper and installer embed this same signed artifact.

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
- Popup pause/revoke and disconnect release capabilities; result tabs remain open.
- Supported release target: native Linux x86_64 Firefox 140+ and local Codex.
  Snap, Flatpak, Windows and macOS are not validated by this package.
