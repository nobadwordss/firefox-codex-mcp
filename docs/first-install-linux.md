# First installation on Linux

> **Release status:** Linux **0.4.2** remains the published stable release.
> The **0.4.3 RPM/DEB candidate is paused and unreleased**; see the
> [pause handoff](linux-packaging-handoff-2026-11-17.md) for remaining validation.

**English** · [Korean translation](first-install-linux.ko.md)

Requirements: Linux x86_64, ordinary desktop Firefox 140+ and the local Codex app
or CLI. You must be able to sign in to Codex. Rust, Node and the project source
are not needed. Snap/Flatpak Firefox and other operating systems are outside
this guide's verification scope.

**Linux 0.4.2 is the English stable release.** The installer and signed extension
use English; this page describes their actual button labels. Use ordinary Firefox
and its existing profile, then manage the connection through the extension's
**Settings**. No extra application-menu launcher or Firefox profile is installed.
Windows remains a separate preview. Older release archives remain unchanged.
See the [management guide](firefox-only-management.md).

## Extract and run setup

1. [Download the Linux 0.4.2 archive](https://github.com/nobadwordss/firefox-codex-mcp/releases/download/v0.4.2/firefox-codex-mcp-0.4.2-linux-x86_64.tar.gz).
2. Use your file manager's **Extract** action to unpack the whole archive.
   Do not run setup from inside the archive.
3. Open a terminal in the extracted folder and run:

   ```sh
   ./install.sh
   ```

Some file managers open shell scripts in an editor or ask how to run them.
If an editor opens, use the terminal command above. No administrator rights
are needed. The Linux package does not guarantee the same double-click behavior
across file managers.

## Install the connection and extension

In the Firefox setup page, choose **Install connection → Continue**. Setup
installs the local helper, registers MCP and installs the browser skill and
guidance automatically. You do not need a separate skill download or configuration
file edits.

In the same page, choose **Add Codex Work Tabs to Firefox**, then confirm Firefox's
**Continue Installation/Allow → Add** prompts. Both the helper and extension are
required. If several Firefox profiles connect, open the extension in the intended
profile and choose **Use this Firefox**.

## Try your first web task

Open a **new chat** in the Codex app or interactive CLI and try:

> Open https://example.com in the browser, click Learn more, and tell me the destination page title.

A new Firefox work tab and a successful link click confirm the connection.
Review any tool approval request for the task you asked for. **Registered** in
the setup page confirms configuration; the web task verifies actual tab control.

## Everyday use and removal

Use your normal Firefox. Open **Codex Work Tabs → Settings** for **Check setup**,
**Repair** or **Remove connection**. Pause and revoke controls stay in the extension.
Install the helper once per OS user; the extension cannot install it by itself.
Remove the helper before removing the extension. If the helper cannot connect or
the extension is already removed, rerun `./install.sh` from the extracted package
for repair or removal. Firefox profiles, website logins and result tabs are preserved.

To update, open setup from the newer package, choose **Update connection**, then
install that package's signed extension too. Direct-distribution XPIs have no
automatic update channel. Start a new Codex chat after updating.

## Troubleshooting

| What you see | What to check |
| --- | --- |
| A script editor opens | Run `./install.sh` in a terminal in the extracted folder |
| Codex is not found | Check the Codex app/CLI installation and reopen setup. The Linux installer needs to locate the Codex executable |
| Tools are missing | Reopen Codex and start a new chat |
| Extension is paused | Choose **Resume** in the intended Firefox extension |
| A profile choice is required | Choose **Use this Firefox** in the intended profile |
| Tool approval is denied | Use the host's permitted approval process. Setup does not change user or administrator approval policy |

Non-interactive `codex exec` has different approval behavior from ordinary chats.
The initial test's default refusal policy blocked work-tab creation. For your
first task, use the Codex app or interactive CLI. Developers running automation
must select an approval review mode supported by their Codex version; there is
no need to disable approvals or the sandbox to use this integration.

## Verification boundary

The [0.4.2 release page](https://github.com/nobadwordss/firefox-codex-mcp/releases/tag/v0.4.2)
provides checksums, GitHub build provenance and `release-verification.json` for
the exact distribution. The [Firefox management source-preparation record](firefox-only-management-e2e-2026-11-17.json)
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
