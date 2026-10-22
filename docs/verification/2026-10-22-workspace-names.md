# Workspace display-name acceptance

2026-10-22. Base `941045fcf65743e76b8f51a94e3db51894da77b4` plus ADR 0117's
changes. The base's [CI](https://github.com/arucil/fleetpane/actions/runs/39784854602)
passes; candidate CI is reported separately after push. This covers Linux X11
with task-owned synthetic Codex/ACP providers. No new installed-provider/model,
macOS GUI or Windows GUI acceptance is inferred.

## Store and public commands

Four core regressions cover exact Unicode names, unchanged UUID/project/
registration/position, independent pins, registration refresh, reopening and
reset with the original directory moved away. Concurrent Store connections have
one winner, stale revisions fail, a SQLite trigger refusal rolls back, and reset
retains the next revision. Invalid names, oversized corrupt persisted text and a
missing current-schema table return errors. Schema 17 upgrades to 18 while
preserving drafts, pins and pending-alert cursors.

`python3 scripts/workspace-names-smoke.py` exercises the public CLI with two
isolated providers, the same Unicode directory on both Hosts, two pending
requests, two exact drafts and a third unavailable Host. Concurrent subprocesses
have one winner; another Host's name is unchanged, refresh/pins retain names,
invalid/stale/failed writes are refused and reset works. Offline/missing-path
rename succeeds by UUID. Complete before/after Host snapshots, drafts and the
source file match exactly. Existing workspace/pin probes and affected earlier-
schema process fixtures pass. [Logs](assets/workspace-names/checks.txt).

## Native editor, identity and replacement

The isolated desktop loads the same fixture configuration and opens both
workspaces as separate tabs. Saving **研究 / release 🚀** updates only the Codex
catalog/pinned entry and tab; the ACP entry retains its default name. Original
paths remain visible. [Save](assets/workspace-names/final-save.png),
[catalog identity readback](assets/workspace-names/catalog-identity.json).
The editor also exercises Use default name and Reload name after a CLI writer
makes its observed revision stale. The old write is refused visibly, with no
automatic rebase. [Reset](assets/workspace-names/final-reset.png),
[conflict](assets/workspace-names/final-conflict.png).

The earlier desktop artifact's normal replacement restores the named tab,
selected ACP session and original Codex questions/ACP permission. Native Copy
matches the **43-byte UTF-8 draft**, including Chinese, one actual newline and
two trailing spaces, before and after replacement. Both draft keys/revisions/data
remain exact. [Replacement receipt](assets/workspace-names/replacement-receipt.json).
The accompanying [readback](assets/workspace-names/final-native-readback.json) and
[events](assets/workspace-names/replacement-query-events.json) distinguish six normal controller query replies
from provider-state changes; it does not claim that the entire sequence counter
is immutable across startup queries or explicit control changes.

## Final close-guard repair and acceptance

Review found an unrelated catalog refresh could emit a name event and clear the
window-close banner despite the editor retaining a failed save. The final
subscription clears that banner only after the editor has resolved its name
error. The final artifact has SHA-256
`15f4d68ccf935c05fd0314cdb03a12c87048b0d463a173fa9c808f2bcdfefe4b`;
the [manifest](assets/workspace-names/manifest.json) separates it from the earlier
artifact's save/reset/conflict and replacement captures.

A native save against an injected SQLite refusal keeps the name/revision intact,
shows the boundary error and refuses normal Alt+F4. Clicking Newest retains the
error and close guard. [Refresh after refusal](assets/workspace-names/guard-refresh-refused.png).
Reload name explicitly clears it. Removing only the task-owned trigger and
saving succeeds at revision 7. Complete Host snapshots match across this
observational interval. [Recovery receipt](assets/workspace-names/guard-receipt.json).

Normal Alt+F4 then exits with code 0. Starting the exact same final artifact
restores both tabs, the custom name, ACP permission and original draft. Take
control is explicit only to permit native Select all/Copy: the clipboard matches
all 43 bytes and both stored draft records remain revision 1.
[Final replacement](assets/workspace-names/guard-replacement-loaded.png),
[native draft copy](assets/workspace-names/guard-draft-exact.png),
[receipt](assets/workspace-names/guard-replacement-receipt.json).
At **900×560**, Rename's field and all four actions remain visible, with wrapped
helper text and independent scrolling.
[Final small editor](assets/workspace-names/guard-small-editor.png).

## Checks and limits

Workspace tests: **366 pass**; desktop tests: **49 pass**; Python script tests:
**22 pass**. Both strict all-target Clippy checks, fmt checks and Host/desktop
builds pass. CLI names, workspace/pins, appearance, Host choice, last session,
recents, Paseo import and final completion-alert process probes pass. See the
command and log inventory in [checks](assets/workspace-names/checks.txt).

Pre-push cross-compilation reproduced an unguarded module export importing Unix-
only storage on Windows. Guard the export with the same `cfg(unix)` as the
catalog. The Windows engine check fails before and passes after that repair;
this is not Windows GUI acceptance. Linux includes the same module as the native
artifact above. Targeted naming tests, strict workspace Clippy and the desktop
compiler check are rerun after the gate change.

This is local display-name acceptance, not filesystem/project renaming or a
provider workflow change. Cross-window labels require explicit reload/refresh.
The complete naming slice is not re-run against installed OpenCodex/DeepSeek or
other-platform GUIs. Release performance and the remaining parity gaps are
unmeasured. All owned GUI/Host/provider runtimes are stopped; normal desktop
exit codes and unchanged source bytes are in [cleanup](assets/workspace-names/cleanup.json).
