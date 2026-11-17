# Linux RPM and DEB candidate (unreleased)

> **Paused and unreleased:** No Linux 0.4.3 or RPM/DEB download is published.
> This page describes unfinished candidate behavior and is not a released setup guide.
> Use the [published Linux 0.4.2 instructions](first-install-linux.md). The
> [pause handoff](linux-packaging-handoff-2026-11-17.md) records the unresolved validation.

The Linux **0.4.3 candidate** proposes RPM and DEB as the primary downloads, with
tar.gz as the fallback for users without administrator rights. The candidate uses
the Rust helper and Mozilla-signed English extension **0.4.2**. Public Linux stable
remains **0.4.2** and Windows **0.5.0-beta.3** remains a separate preview; their
release files are unchanged.

## Candidate installation flow

The following commands describe the proposed flow after a verified release is
available. Candidate package filenames are examples, not available download links:

```sh
sudo dnf install ./firefox-codex-mcp-0.4.3-1.x86_64.rpm  # Fedora
sudo apt install ./firefox-codex-mcp_0.4.3-1_amd64.deb    # Ubuntu / Debian
```

Install `codex-work-tabs-0.4.2.xpi` in ordinary Firefox. Open **Codex Work Tabs →
Settings → Connect Codex → Continue** as your normal user. This registers that
account's Codex connection, browser skill and guidance. Repeat Connect for each
OS account that wants it. Never run Connect with `sudo`. No dedicated browser
profile or application-menu launcher is installed. Existing Firefox profiles,
website logins and other settings stay in place.

If you need the bundled extension link or the extension cannot open Settings,
run `firefox-codex-mcp --manage` as your normal user. Check setup with
`firefox-codex-mcp --doctor`. The Codex command-line executable does not need to
be on PATH, but a working local Codex app or CLI is required to use the connection.

## Update

Download and install the newer package with the same `dnf install ./…rpm` or
`apt install ./…deb` command. Each account uses the fixed system program directly;
there is no private copy left behind to keep running an older version. A package
transaction changes the installation generation. Old helper, router, native-host
and MCP processes stop, releasing previous work-tab access. Firefox and result
tabs remain open. Start a new Codex chat after updating.

These local packages do not add an APT/DNF repository. Ongoing automatic package
updates and repository signing are not provided. Check the release's SHA256SUMS
and GitHub provenance when verifying a download. The Firefox extension is signed
by Mozilla; the RPM and DEB currently have no separate package/repository signature.
Install newer signed XPIs in Firefox separately when the release calls for one.

## Disconnect and remove

First open **Settings → Disconnect Codex** for each connected OS account. It
revokes work-tab access and removes only that account's owned Codex configuration,
skill and guidance. Disconnect removes the entire owned Firefox MCP entry,
including custom options inside that entry. Unrelated MCP entries, edits outside
managed sections and modified managed files are preserved and reported. The system
program remains available for other accounts and for reconnecting later.

Then remove it with the software manager:

```sh
sudo dnf remove firefox-codex-mcp
# or
sudo apt remove firefox-codex-mcp
```

DEB purge is also supported. Both remove and purge remove package-owned files and
the activation marker; neither edits users' homes. Running packaged helpers stop
even for accounts that did not disconnect. Their remaining Codex registration
cannot launch a removed program. Firefox profiles, logins and result tabs remain.
Remove the Firefox extension separately in Add-ons Manager if desired.

### Removed the package before disconnecting

Reinstall the same or newer RPM/DEB, then run `firefox-codex-mcp --uninstall` as
the affected user, or use extension **Settings → Disconnect Codex**. Remove the
system package afterwards. Alternatively extract the current tar.gz and run
`./install.sh --uninstall`; it can clean a package connection receipt after system
removal without installing a new helper. Modified or foreign settings remain
untouched. Do not manually delete Firefox profiles or Codex's whole config folder.

## Existing tar.gz installations

Install the RPM/DEB, then open `firefox-codex-mcp --manage` and choose **Reconnect
Codex**, or open extension Settings and reconnect. This switches Codex to the
system program and retires unchanged, receipt-owned user binaries, native-host
files and old launchers. The old native registration must be retired because it
would mask Firefox's system registration. Modified files and foreign registrations
are preserved; a masking registration produces a clear conflict instead of
silently replacing user data.

If a custom native registration blocks migration, review the reported file and
move it aside yourself only if it is yours. Reconnect after resolving it. Keep
your original backup. Running old connections must be closed after migration;
the new installer resets this user's old router without terminating Firefox.

To return to a user-local tar.gz installation, disconnect and remove the system
package first, then run the fallback's `install.sh`. If a system package is still
installed, the fallback uses it rather than creating a competing private copy.

## Ownership and validation

The package owns `/usr/libexec/firefox-codex-mcp/`, the `/usr/bin` command and
system Native Messaging manifests. Its scripts access only fixed system paths;
they never enumerate homes, edit Firefox profiles, register root's Codex, open a
browser or create menu launchers. User setup owns a receipt under
`~/.local/state/firefox-codex-mcp/` and compares files before changing them.

The root-owned generation marker binds helpers to a payload digest. Helpers check
it at startup, before dispatch and every 500 ms while running. A package change
fails closed; already-dispatched browser actions cannot be undone. A failed
package transaction may leave the connection unavailable until the software
manager completes configuration or reinstalls the package.

No final `release-verification.json` or published candidate artifact exists.
The [pause handoff](linux-packaging-handoff-2026-11-17.md) separates completed
Ubuntu/Debian/Fedora lifecycle checks from the unresolved extension-first Settings
failure and browser phases that did not execute. Earlier green jobs with masked
test failures are not acceptance. Automated headless browser checks are separate
from novice testing, a fresh desktop OS install or Windows desktop acceptance.
