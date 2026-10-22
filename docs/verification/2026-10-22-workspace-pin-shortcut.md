# Active workspace pin shortcut acceptance

2026-10-22. Linux x86_64 isolated X11 `:92`, GPUI Kit 0.7.3, debug build and
Mesa software renderer. Source base `4f9d6240be864284d52cec11fc0ccffc74a7a7b6`
plus this pin implementation and the canonical-path test repair. The accepted
desktop SHA-256 is `27895108df4f8e0ce6805edf931561d15ab905cffe654caa3c3bf3b7d9ee738a`.
Only additional capacity-test assertions were edited after native QA; production
code and the accepted executable remain unchanged.
[Receipt](assets/workspace-pin-shortcut/receipt.json),
[decision](../adr/0118-active-workspace-pin-shortcut.md).

## Native behavior

Synthetic Codex and ACP Hosts retain an original question and permission in the
same Unicode directory, plus two independent 43-byte multiline drafts. A third
saved workspace uses an offline Host. The fixture derives from
`scripts/workspace-names-smoke.py --fixture /tmp/fp-pin-shortcut-native`; the QA
store seeds three layout tabs and adds SQLite insert/delete audit triggers.

- With no catalog row mounted, Ctrl+Shift+P unpins only the selected Codex UUID.
  A 1.4-second physical hold adds exactly one audit row. Unit coverage also
  checks release and focus-reset behavior. Before the fix, the same hold added
  **eight** rows: Kit's X11 backend always emitted `is_held: false`, despite
  filtering synthetic key releases. P press/release tracking fixes that boundary.
  [Before](assets/workspace-pin-shortcut/before-repeat-fix-held-one-action.json),
  [after](assets/workspace-pin-shortcut/held-one-action.json).
- Ctrl+P, Ctrl+Alt+Shift+P and the pin chord while a Kit popup is focused leave
  the audit unchanged. Clicking **Tabs → Toggle workspace pin** executes one
  transition. The catalog itself does not treat its selected row as an active
  tab shortcut target. [Menu](assets/workspace-pin-shortcut/menu.png),
  [guard](assets/workspace-pin-shortcut/menu-guard.json).
- An injected insert/delete refusal retains the original pins and shows the
  actual error. A normal window-manager close is refused. Removing the trigger
  and pressing the chord again writes once and clears the error/close warning.
  A second failure followed by **Keep saved pins** adds no audit row.
  [Refused close](assets/workspace-pin-shortcut/failure-close.png),
  [900×560 failure, permission and composer](assets/workspace-pin-shortcut/small-failure.png),
  [explicit keep](assets/workspace-pin-shortcut/kept-saved.json).
- The same-path ACP entry toggles independently; the offline workspace pins
  successfully while its unavailable transport remains visibly reported.
  Reopening an already cached catalog after another shortcut shows **2 / 32**
  without pressing Newest. [Offline](assets/workspace-pin-shortcut/offline-pinned.png),
  [updated cache](assets/workspace-pin-shortcut/cached-pins.png).
- With real shell PID **1714124**, keyboard input enabled and the terminal screen
  focused, the chord toggles its saved workspace without entering the PTY.
  Terminal UUID, PID, dimensions and `next_input: 0` remain exact, as do both
  complete Host snapshots over that interval.
  [Terminal](assets/workspace-pin-shortcut/terminal-shortcut.png),
  [before](assets/workspace-pin-shortcut/terminal-list-before.json),
  [after](assets/workspace-pin-shortcut/terminal-list-after.json).
- Normal close and a new process restore all three pins and both exact saved
  draft records at revision 1. Native ACP composer Copy before/after replacement
  returns the identical **43 UTF-8 bytes**, including a real newline and two
  trailing spaces. Copy requires explicit Take control; restoration itself
  remains read-only. [Restored composer](assets/workspace-pin-shortcut/reopened.png),
  [copy](assets/workspace-pin-shortcut/reopened-draft-copy.png),
  [three restored pins](assets/workspace-pin-shortcut/reopened-pins.png).

Before the explicit control claims and shell creation, every Host snapshot field
except the transport sequence remains exact: provider/Host PIDs, owner, pending
requests/observations, active turn, attention and mutation journal are unchanged.
Initial history reads advance that sequence. The settled failure/recovery and
terminal shortcut intervals each preserve the entire snapshot byte-for-byte.
All three GUI processes exit normally with code 0. Owned workspace, Hosts,
providers and shell are stopped; the fixture's source file is unchanged.
[Cleanup](assets/workspace-pin-shortcut/cleanup.json).

## Checks and CI follow-up

`cargo test --workspace --locked`: **367 pass**. Desktop tests: **50 pass**.
`python3 -m unittest discover -s scripts/tests -p '*_test.py'`: **22 pass**.
Both strict all-target Clippy checks, both fmt checks, Host/desktop builds and
the Windows engine cross-check pass. The final four pin tests additionally cover
atomic concurrent toggles, full-capacity refusal/rollback and removal/readdition.
Names and existing pins process smokes pass through the public CLI, including
exact snapshots/drafts, explicit assignment idempotence and the new toggle.
[Command inventory](assets/workspace-pin-shortcut/checks.txt).

Previous exact-head [CI run 39795438762](https://github.com/arucil/fleetpane/actions/runs/39795438762)
for `4f9d624` passed Windows/Ubuntu engine and Linux desktop, but failed one macOS
name test. Its fixture looked up an alias temp path while registration stores a
canonical path. A Linux symlink `TMPDIR=/tmp/fp-names-ci-fix/alias` reproduces the
same `Option::unwrap()` failure before repair; using the saved registration's
path passes all four name tests. The process fixture also resolves its root.
This changes test identity handling, not production path lookup semantics.
[Before](assets/workspace-pin-shortcut/canonical-name-before.log),
[after](assets/workspace-pin-shortcut/canonical-name-after.log).

This document records local acceptance. New exact-head remote CI is reported
separately. No macOS/Windows native GUI, installed OpenCodex/DeepSeek model run,
IME runtime, configurable shortcuts, pin reordering, automatic cross-window
refresh or release performance claim is made for this slice.
