# Linux packaging work paused — 2026-11-17

The user requested a pause before publication and further feature development.
No Linux 0.4.3 tag or release has been published. Do not publish or resume product
development until the user asks. Continue the separately requested, isolated KDE
comparison without changing the personal Firefox or Codex installation.

## Saved source

- Product `main`: `a707f1ad920b2803f1512ab3871bf732d111af98` (development version
  `0.5.0-beta.4`; no Windows beta.4 release).
- Linux release branch `codex/linux-0.4.3`:
  `a794a1cbfded05c4cf321fee9c78c566ba49ba01`, with Cargo version `0.4.3`.
- Both source commits were pushed before the pause. The release branch differs
  from main only in Cargo version and lockfile version.
- Existing public Linux Latest remains **0.4.2**; Windows remains the separate
  **0.5.0-beta.3** preview. Previous release files were not replaced.

## Implemented, not released

RPM and DEB builders retain tar.gz as the fallback. The system package owns a
fixed executable, Native Messaging manifests and a root-owned installation
generation. Per-account Connect/Disconnect manages Codex registration, skill,
guidance and a receipt. Package transactions do not enumerate or edit homes.
Running package helpers are fenced on update/removal; a per-connection generation
prevents old MCP access from returning after rapid Disconnect/Reconnect.

Migration retires only unchanged receipt-owned portable files. TOML edits preserve
unrelated entries, comments and explicit empty arrays. Custom CODEX_HOME is
recovered from the receipt. English guides distinguish user Disconnect from system
package removal. The unchanged Mozilla-signed extension is **0.4.2**.

Setup buttons now wait for initial state. CI uses explicit Bash pipefail and a Rust
evidence verifier so failed tests or missing completion records block publication
artifacts. A cold, extension-first Settings test was added alongside existing
Firefox/browser and actual Codex app-server tests.

## Actual verification state

Local checks passed: Rust formatting, Clippy (Linux and Windows GNU targets),
44 Rust unit tests, ordinary integration checks and 72 JavaScript tests. An isolated
Fedora DNF lifecycle test passed against an earlier beta.4 snapshot; it was not a
browser/model or final-release check.

The corrected main [Linux CI](https://github.com/nobadwordss/firefox-codex-mcp/actions/runs/44046877873)
**failed** after these successful phases:

- Actual Ubuntu 22.04 / Debian 12 DEB and Fedora 45 RPM install, package revision
  upgrade, reinstall, removal/purge, two-user preservation and legacy tar migration.
- Explicit lifecycle completion-record validation.
- Ubuntu installed-DEB continuity and multi-profile/Settings scenarios, including
  actual Codex app-server dispatch and independent DOM readback.

The new third browser scenario opened Settings before any user connection but
failed to observe an enabled **Connect Codex** and disabled **Disconnect Codex**
within 15 seconds. This is unresolved. The strict failure correctly prevented the
later RPM browser phase, attestation and distribution upload. Do not infer those
unexecuted phases passed.

The exact Linux 0.4.3 [release-branch CI](https://github.com/nobadwordss/firefox-codex-mcp/actions/runs/44046897839)
also failed at the installed-DEB browser step. Its failure must be inspected before
any release acceptance. No fix was started after the user's pause.

Earlier jobs reported green while `tee` masked failing subprocesses. Their logs
contain Ubuntu test-account execution permission errors, Ubuntu browser readback
on the wrong tab, and Fedora initial-confirmation readiness failure. These jobs
are **not** release acceptance. Later commits fixed runner input staging, handle
selection, UI readiness and failure propagation.

An exact earlier candidate tar.gz completed a real-model clean-home task in
40.42 seconds with independent click/input readback. This remained preliminary:
the helper subsequently changed, and that result does not validate the primary
RPM/DEB cold Settings flow, a fresh OS or novice usability.

## Evidence locations

- Corrected main CI evidence: `/tmp/firefox-main-a707f1a-evidence`.
- Earlier misleading-green artifacts and logs:
  `/tmp/firefox-linux-0.4.3-release-evidence/first-ci`.
- Preliminary actual-model tar evidence:
  `/tmp/firefox-linux-0.4.3-release-evidence/first-model/evidence-summary.json`.
- Preliminary local RPM evidence:
  `/tmp/firefox-package-local-2026-11-17/evidence/local-rpm-verification.json`.
- Linux release worktree: `/tmp/firefox-linux-0.4.3-release-2026-11-17`.

Personal Linux was not given a product installation. Tests used disposable homes,
containers or private mount/PID/IPC namespaces. The existing Codex login was only
mounted read-only for the model test; credentials were not copied. Its isolation
record reports all eleven original settings unchanged. Personal Firefox profiles
and logins were not modified.

## Remaining work, only if resumed

1. Inspect the cold Settings page URL/body/state and environment to explain its
   failure. Diagnose whether it is product behavior, test assumptions or both.
2. Require all three lifecycle records and all six browser records on both Ubuntu
   and Fedora, with successful subprocess exit statuses.
3. Retest the final exact tar.gz with the real model in the isolated home; verify
   DEB/RPM/tar payload equality, glibc floor, checksums and GitHub provenance.
4. Prepare final scoped verification, licenses, English release notes and download
   links. Publish only after the user resumes release work; verify anonymous
   downloads and unchanged older assets afterwards.

## Current priority: independent comparison

Compare public Firefox MCP **0.4.2** with public Computer Use MCP **2.2.0**, pinned
to `b373e68e6273217d549097efdfbbee0b9fbd47d4`, in separate KDE test homes with
the same OS, Firefox, actual Codex model, reasoning and approval conditions.
Keep the unfinished 0.4.3 packaging state separate. Record documentation claims,
actual model behavior and deterministic isolation/revocation probes distinctly.
Choose between independent development, a narrower scope, or upstream use and
contribution based on results; maintaining this project is not a premise.
