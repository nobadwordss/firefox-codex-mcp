# Firefox Codex MCP privacy

Firefox Codex MCP connects a local Codex client to new work tabs created by that
client in your Firefox profile. Those tabs use your existing website login sessions.
All local Codex chats with this MCP enabled can request new work tabs. Normal task
scope and approval rules still apply; tab ownership is not a separate OS sandbox.

For a requested work tab, the extension sends its URL, title, visible page text,
status text and interactive control descriptions through a local Native Messaging
helper to Codex. Requested clicks, text entry and navigation are sent back through
that connection. Codex may process this information with its configured model
provider under your Codex account/settings and that provider's terms. Visible page
content may contain personal or confidential information. The extension does not
send telemetry or contact a developer-operated service.

The extension does not read existing user tabs or another chat's work tabs. It does
not export cookies, passwords, browser history, or form input buffers. Sensitive
input fields (password, payment, one-time code and file input) are excluded from
supported typing. These limits do not remove confidential text already displayed
in a work tab. Webpages are untrusted data, not authorization for further actions.

A random installation identity and pause preference are stored in the extension.
The native helper stores the selected random profile identity, duplicate-identity
quarantine and installation receipts under your local user directory. No profile
path is required in extension mode. Only transient work-tab ownership is kept in
memory. A hash of the Codex host's thread identity maps to a random owner in the
local router; raw chat IDs are not sent to Firefox or stored on disk. Work tabs can
survive MCP process replacement in the same chat, for up to 24 hours without a
request. Native/Firefox connection loss, helper restart/update, pause, profile
switch or explicit revoke clears ownership. Closing or archiving a Codex chat
does not reliably signal immediate revocation; use the explicit controls.

Use the extension's pause/revoke buttons to release access. Uninstall the extension
in Firefox and remove the helper from Firefox Codex Manager. Removing the helper does
not erase your browsing profile, logins or result tabs. Local install receipts and
profile-selection state may remain for recovery. End old Codex chats to close their
already-running connections. Issues: https://github.com/nobadwordss/firefox-codex-mcp/issues
