# First installation on Linux

**English** · [Korean translation](first-install-linux.ko.md)

Requirements: Linux x86_64, ordinary desktop Firefox 140+ and the local Codex app
or CLI. Sign in to Codex before the first web task. Rust, Node and the project
source are not needed. Snap/Flatpak Firefox and other operating systems are
outside this guide's verification scope.

**Linux 0.4.3 is the English stable release.** RPM and DEB are the primary
downloads; tar.gz is the fallback without administrator rights. The setup page
and Mozilla-signed extension **0.4.2** use English. Existing Firefox profiles,
website logins and tabs stay in place. No extra app-menu entry or Firefox profile
is created. Windows **0.5.0-beta.3** remains a separate, unchanged preview.

## Install the helper

Download the package for your distribution:

| Distribution | Download |
| --- | --- |
| Fedora | [firefox-codex-mcp-0.4.3-1.x86_64.rpm](https://github.com/nobadwordss/firefox-codex-mcp/releases/download/v0.4.3/firefox-codex-mcp-0.4.3-1.x86_64.rpm) |
| Ubuntu / Debian | [firefox-codex-mcp_0.4.3-1_amd64.deb](https://github.com/nobadwordss/firefox-codex-mcp/releases/download/v0.4.3/firefox-codex-mcp_0.4.3-1_amd64.deb) |

Open the downloaded file with your software manager if it supports local packages.
If no installer opens, open a terminal in the download folder and use the command
for your distribution:

```sh
# Fedora
sudo dnf install ./firefox-codex-mcp-0.4.3-1.x86_64.rpm
```

```sh
# Ubuntu / Debian
sudo apt install ./firefox-codex-mcp_0.4.3-1_amd64.deb
```

This installs the helper for the system. It does not open Firefox, register root's
Codex or modify users' home directories. Use your normal user account for the
remaining steps. Never run **Connect Codex** with `sudo`.

The local RPM/DEB does not add an automatic update repository. The packages have
no separate package/repository signature; the extension is Mozilla-signed.
Check the release's `SHA256SUMS` and GitHub build provenance when verifying files.

## Add the extension and connect your account

1. Open the [Mozilla-signed Codex Work Tabs 0.4.2 XPI](https://github.com/nobadwordss/firefox-codex-mcp/releases/download/v0.4.3/codex-work-tabs-0.4.2.xpi)
   in ordinary Firefox and confirm **Continue Installation/Allow → Add**.
   If Firefox saves the file, open Add-ons Manager, choose the gear menu's
   **Install Add-on From File**, and select the downloaded XPI.
2. Click **Codex Work Tabs → Settings → Connect Codex → Continue**.
   This registers your account's Codex connection, browser skill and guidance.
   You do not need a separate skill download or configuration file edits.
3. Repeat **Connect Codex** for each OS account that wants the connection.
   Install the extension in the ordinary Firefox profile each account uses.

If the extension is already installed, use its **Settings** button after installing
the RPM/DEB. If Settings cannot reach the helper, run `firefox-codex-mcp --manage`
as your normal user. Its page also includes **Add Codex Work Tabs to Firefox**.
If several profiles connect, choose **Use this Firefox** in the intended profile.

## Try your first web task

Open a **new chat** in the Codex app or interactive CLI and try:

> Open https://example.com in the browser, click Learn more, and tell me the destination page title.

A new Firefox work tab and a successful link click confirm the connection.
Review any tool approval request for the task you asked for. **Registered** in
the setup page confirms configuration; the web task verifies actual tab control.

## Update, disconnect and remove

Install a newer RPM/DEB with your software manager or the same command using its
new filename. Each account uses the system helper directly. Updating or removing
the package stops its old running helpers and releases previous work-tab access.
Firefox and result tabs remain open. Start a new Codex chat after updating.

To remove the connection completely:

1. Open **Codex Work Tabs → Settings → Disconnect Codex** for each connected
   OS account. This removes only that account's owned connection, skill and
   guidance; the system helper remains installed. User edits are preserved.
2. Remove the helper with your software manager, `sudo dnf remove firefox-codex-mcp`
   or `sudo apt remove firefox-codex-mcp`. DEB purge is also supported.
3. Remove Codex Work Tabs separately in Firefox Add-ons Manager if desired.

Removing the package first still stops running packaged helpers. It leaves user
settings untouched. Reinstall the RPM/DEB to disconnect those accounts, or extract
the current tar.gz and run `./install.sh --uninstall` to clean their receipts.
Do not delete Firefox profiles or Codex's whole settings folder.
See [migration and recovery](linux-packages.md).

New signed XPIs are installed separately when a release calls for one.
Direct-distribution XPIs currently have no automatic extension update channel.

## Fallback without administrator rights

[Download the 0.4.3 tar.gz](https://github.com/nobadwordss/firefox-codex-mcp/releases/download/v0.4.3/firefox-codex-mcp-0.4.3-linux-x86_64.tar.gz),
extract the whole archive, open a terminal in the extracted folder and run:

```sh
./install.sh
```

Choose **Install connection → Continue**, then **Add Codex Work Tabs to Firefox**
and accept Firefox's prompts. This fallback installs a private helper for your
OS account. Later, use **Settings → Remove connection** to remove it, or run
`./install.sh --uninstall`. To update it, extract the newer archive and choose
**Update connection**. Script double-click behavior varies by file manager; if an
editor opens, use the terminal command above.

If an RPM/DEB is already installed, this fallback connects to the system helper
rather than creating a competing private copy. To switch from an older tar.gz,
install the RPM/DEB and choose **Reconnect Codex**. Unchanged receipt-owned legacy
files are retired; a custom registration that masks the system helper is reported
and preserved. See the [management guide](firefox-only-management.md).

## Troubleshooting

| What you see | What to check |
| --- | --- |
| No package installer opens | Use the matching `dnf` or `apt` command above |
| Settings cannot reach the helper | Run `firefox-codex-mcp --manage`, or reinstall the RPM/DEB if the command is missing |
| A custom Native Messaging registration is reported | Review the reported file before moving it aside; setup preserves foreign or edited files |
| Tools are missing | Install/sign in to the local Codex app or CLI, reopen it and start a new chat. The CLI need not be on PATH for setup |
| Extension is paused | Choose **Resume** in the intended Firefox extension |
| A profile choice is required | Choose **Use this Firefox** in the intended profile |
| Tool approval is denied | Use the host's permitted approval process. Setup does not change user or administrator approval policy |

Non-interactive `codex exec` has different approval behavior from ordinary chats.
The initial test's default refusal policy blocked work-tab creation. For your
first task, use the Codex app or interactive CLI. Developers running automation
must select an approval review mode supported by their Codex version; there is
no need to disable approvals or the sandbox to use this integration.

## Verification boundary

The [0.4.3 release page](https://github.com/nobadwordss/firefox-codex-mcp/releases/tag/v0.4.3)
provides checksums, GitHub build provenance and `release-verification.json` for
the exact RPM, DEB and archive artifacts. Package-manager lifecycle checks run in
disposable Ubuntu and Fedora environments; see [package ownership and validation](linux-packages.md).
These automated checks do not establish manual novice or fresh-desktop acceptance.

The [Firefox management source-preparation record](firefox-only-management-e2e-2026-11-17.json)
records its own source builds, signed-extension checks and clean-home actual-model
run. Release verification identifies the final archive separately. Earlier
[English-default candidate evidence](english-default-e2e-2026-11-17.json) is historical.

The [2026-11-17 clean-home record](linux-first-install-e2e-2026-11-17.json) covers
the public 0.4.0 archive extracted outside the repository. It started with no MCP,
skill, guidance or native-host registration in a private home. Only an existing
Codex login was mounted read-only. Actual Firefox installation prompts and an
actual model's reads, clicks and typing were verified, including independent DOM
readback.

The test uses headless Firefox and Marionette automation. It retains the host's
installed Codex, Firefox and system libraries. It does not cover a fresh OS,
prerequisite installation, new-account sign-in, a separate Codex desktop GUI
session, manual file-manager double-click behavior or a novice user study.
Extension confirmation buttons were invoked by automation, and the CLI used
automatic approval review for that invocation. This does not establish equivalent
usability for manual approval prompts.

## Reproduce the regression test

This section is for developers, not package users. The test environment needs
Rust, `bwrap`, geckodriver and a logged-in Codex capable of reaching a real model.
Extract the distribution under `/tmp`, outside the checkout, and use a fresh
artifact directory for each run.

```sh
FIREFOX_FIRST_RUN_PACKAGE=/tmp/extracted/firefox-codex-mcp-VERSION-linux-x86_64 \
FIREFOX_FIRST_RUN_ARTIFACTS=/tmp/firefox-first-run-new-output \
FIREFOX_FIRST_RUN_CODEX=/absolute/path/to/native/codex \
FIREFOX_GECKODRIVER=/absolute/path/to/geckodriver \
cargo test --release --locked --test firefox_e2e \
  first_run_e2e::clean_distribution_first_web_task -- \
  --exact --ignored --nocapture --test-threads=1
```

The test creates private mount, PID and IPC namespaces. It hides the real home,
mounts the system read-only and exposes only `.codex/auth.json` as a read-only
login. It never copies credentials into artifacts. This test currently requires
file-based Codex credentials. It compares original settings before and after
the run, then removes the private account and its processes.

The unchanged package script runs with no arguments. WebDriver attaches to the
Firefox it launches, completes the actual extension installation prompts and
starts a real model task without naming Firefox, MCP or a skill. Independent DOM
readback checks the click count and input. MCP configuration, skills and model
selection are not overridden; only CLI approval review uses `--approve-for-me`.

The output directory contains `first-run.json`, `isolation.json`, model tool-call
logs and step PNGs. Elapsed time describes automation, not a novice's installation
time.
