# Local workspace updates across windows

2026-10-23. Linux x86_64, GPUI Kit 0.7.3, isolated X11 `:91`, debug build,
Mesa software renderer. Source base `125e0e0a61aabd0e916bddffedccceea95939413`
plus this change. [Source/artifact receipt](assets/workspace-sync/receipt.json),
[decision](../adr/0121-workspace-preference-observation.md).

## Engine and process checks

- `cargo test --workspace --locked`: **379 passed**. Four new snapshot tests
  cover independent handles changing labels/order/membership, reset/stale-save
  refusal, exact database bytes with a missing directory, bounded/duplicate/
  unknown/corrupt input rejection and consistent workspace/pin labels during
  concurrent writes. Existing project-label tests also exercise monotonic merging.
- `python3 scripts/workspace-names-smoke.py`: public snapshot command observes
  independent Host-scoped labels and pin order; duplicate/unknown UUIDs refuse;
  offline missing-directory reads preserve identity and exact drafts/complete
  synthetic Codex/ACP snapshots. `python3 scripts/workspace-pins-smoke.py` passes
  existing concurrent-order, rollback and pin/identity/draft preservation checks.
- `cargo test --manifest-path apps/desktop/Cargo.toml --locked`: **51 passed**.
  Both strict all-target Clippy checks, format checks, builds, Windows engine
  cross-check, 22 Python script tests and `git diff --check` pass. No schema,
  dependency or CI job is added.

[Workspace tests](assets/workspace-sync/workspace-tests.log),
[desktop tests](assets/workspace-sync/desktop-tests.log),
[CLI smoke](assets/workspace-sync/names-smoke.log),
[pin smoke](assets/workspace-sync/pins-smoke.log),
[workspace Clippy](assets/workspace-sync/workspace-clippy.log),
[desktop Clippy](assets/workspace-sync/desktop-clippy.log),
[Windows check](assets/workspace-sync/windows-check.log).

One initial private-fixture setup returned Pending for a `thread/start` operation.
The [Host log](assets/workspace-sync/initial-fixture-failure.log) records a client
deadline; after fixture shutdown the journal records that operation as unknown.
Exact cause is unconfirmed. The fresh fixture and later process smoke complete;
no retry, deadline increase or unrelated runtime change was added. Separately,
the first new pin-order smoke assertion expected the pre-toggle order; inspecting
the script's successful toggle showed the newest pin correctly first. The final
assertion now checks that actual order.

## Native acceptance

`python3 scripts/workspace-names-smoke.py --fixture /tmp/fp-workspace-sync-native-v2`
creates task-owned synthetic Codex/ACP Hosts with pending questions/permissions
and two independent 43-byte drafts. The retained [extension](assets/workspace-sync/extend.py)
adds a real Git checkout/worktree pair, other-Host registration, three tabs and
Main/Feature labels. [Probe](assets/workspace-sync/probe.py) records complete
Host snapshots and local table rows; [control](assets/workspace-sync/control.py)
injects local failures/other-process changes.

- Two desktop processes share the catalog Store and use separate appearance/
  notification stores. Window A types `本地未保存 🚀`; B saves workspace label
  `另一窗口 Main`. A's tab/catalog/pin label updates automatically while its
  editor text and original revision remain. A's Save refuses the stale revision;
  Alt+F4 retains the window and real error. Background reads do not release it.
  [Automatic update](assets/workspace-sync/auto-workspace-a2.png),
  [stale save/close](assets/workspace-sync/stale-editor-close.png).
- B's Down action reorders pins; A observes it while retaining its name error.
  B saves project label `同步项目 🚀`; A updates both Main/Feature tabs and the
  shared project heading, preserving workspace names, other-Host labels and
  A's editor/error. [Pin order](assets/workspace-sync/auto-pin-order.png),
  [project propagation](assets/workspace-sync/auto-project-a.png).
- After explicit name reload, an injected SQLite delete refusal makes A's pin
  move fail and blocks close. Removing the trigger and reversing pins through
  the CLI does not erase A's error or replace its old observed order. At
  **900×560**, Reload pins remains reachable and restores the current saved list.
  [Failed save](assets/workspace-sync/pin-save-failure.png),
  [retained failure/order](assets/workspace-sync/failed-pins-retained-small.png).
- An invalid project name makes the observer pause with an explicit error.
  Repairing the row and unpinning externally leaves that observer paused; Reload
  workspace updates applies the new list and resumes observation. The final
  error-layout adjustment was rebuilt and rechecked at 900×560: the full error
  wraps, and its reload button remains available.
  [Paused after repair](assets/workspace-sync/paused-after-repair.png),
  [explicit recovery](assets/workspace-sync/reloaded-sync-small.png),
  [final error rendering](assets/workspace-sync/final-error-small2.png).
- The final binary observes project revision 2 (`终版同步 🚀`) and a newly
  repinned workspace. Normal close/replacement preserves saved labels, pins,
  registrations and drafts. Native ACP composer Select all/Copy returns exactly
  **43 UTF-8 bytes** before and after replacement, including the newline and two
  trailing spaces. Explicit Take control is separate; neither pending request
  is answered. [Copy before](assets/workspace-sync/draft-copy-before.png),
  [replacement](assets/workspace-sync/replacement-small.png),
  [Copy after](assets/workspace-sync/draft-copy-after.png).

Settled rename/order/failure/recovery intervals retain both **complete** Host
snapshots exactly. All captures preserve draft, workspace-registration and
project-identity rows. Later explicit history reopening changes only transport
`sequence`; explicit ACP takeover/replacement changes `owner` and `sequence`.
These are recorded separately from preference observation. Four normal GUI
exits return 0. Owned Hosts and the isolated workspace stop, their sockets are
removed and the original source file stays exact. [Cleanup](assets/workspace-sync/cleanup.json).

This is Linux synthetic-fixture acceptance. No new installed OpenCodex/DeepSeek
model call, macOS/Windows GUI, native Wayland, IME, remote-device sync or release
performance claim is made. Tab layout, catalog membership and recent-directory
refresh remain separate work. Remote CI is separate from local results.
