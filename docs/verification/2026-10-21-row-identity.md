# Conversation identity collisions — 2026-10-21

Native conversation keys now distinguish status rows from items and preserve
thread/turn/item boundaries. Previously an item named `status` shared the status
row's key; `a:b` / `c` and `a` / `b:c` shared a command entity. This is a concrete
cache-key defect, not a provider deduplication heuristic.

The [receipt](assets/row-identity/receipt.json) binds source and binaries to the
synthetic input, test logs, screenshots and clipboard evidence. Private evidence
is at `/tmp/fleetpane-replay-157-20261021`.

## Checks

`cargo test --manifest-path apps/desktop/Cargo.toml --locked
host_view::row_changes::tests -- --nocapture` initially failed both new
regressions: retained entity counts were 1 instead of 2 and 3 instead of 4.
The tests use the same row builder as the Host view. After the fix, all 18 desktop
tests pass, including existing 1,200-card streaming, metadata, paging and removal
checks. Strict desktop Clippy, desktop build, formatter and `git diff --check`
pass. Core crates and provider fixtures did not change; the prior 165 root tests
were not rerun or presented as a new core acceptance result.

## Native before/after

Hidden workspace `fleetpane-identity-20261021`, 1300 × 900 window on a 1400 × 1000
display. A private Codex-shaped fixture supplies two completed turns containing
five items. No model is called.

The previous resource-card binary, SHA-256
`85915f0505d1e153464d07dbd862095ee75e76686f6475d72f783f79204228c4`,
loads the most recent turn correctly. Loading earlier history then shows the
status content twice, and the second turn's command becomes `FIRST_TOOL` rather
than `SECOND_TOOL`. Before-fix screenshots retain both observations. An attempted
before-fix Copy returned no readable STRING clipboard target; that attempt is
excluded from copy acceptance and its cause was not separately established.

The fixed binary uses the same Host PID 3031693 and provider PID 3031695. Loading
the same earlier page now shows one turn status and one distinct status-named
message, followed by the correct first command. The second turn retains its
second command and distinct `Case`/`case` messages. Opening the first command
leaves the second collapsed; both show their own output when opened.

Five successful clipboard reads match the intended source: first command JSON
(216 bytes), second command JSON (218 bytes), turn metadata, the exact status-named
message, and second command again after Reload conversation and earlier-page
loading. The two command JSON objects retain their respective output, cwd, status,
exit code and duration. The repeated second-command clipboard payload is identical.

Both native apps exited normally with code zero. The owned Host/provider stopped,
their task socket disappeared and the hidden workspace stopped. Existing user
sessions and desktop were untouched.

## Limits

This accepts Linux native cache identity with a synthetic protocol history. It
does not add provider selection, missing-ID reconciliation, fuzzy matching,
conflicting-history repair, permission key changes, a new external model check,
remote CI, cross-platform or performance acceptance. General history eviction and
the user's deferred Git Diff expansion remain open.
