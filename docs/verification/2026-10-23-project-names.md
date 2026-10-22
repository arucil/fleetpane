# Project display name acceptance

2026-10-23. Linux x86_64, GPUI Kit 0.7.3, isolated X11 `:92`, debug build,
Mesa software renderer. Source base
`87f439dac47c563327a30da17b5a9ea20cc5e2e5` plus this change.
[Source hashes, artifact identity and receipt](assets/project-names/receipt.json),
[decision](../adr/0120-local-project-display-names.md).

## Engine and CLI

- `cargo test --workspace --locked`: **375 passed** in the retained final run.
  Five project tests cover real main/linked Git identity, independent Hosts and
  folder registrations, pin/register/reopen preservation, sibling label merges,
  stale observations, reset revisions, invalid/corrupt/oversized values,
  exhausted revisions, offline missing paths and one concurrent save winner.
  Schema-18 migration compares every pre-existing table row before/after and
  refuses a missing current-schema project table.
- `python3 scripts/workspace-names-smoke.py`: public project/workspace commands,
  concurrent subprocess saves, stale refusal, injected write rollback, Unicode,
  reset, pins, re-registration and offline missing-directory edits. Exact drafts
  and complete Codex/ACP snapshots remain unchanged. The existing CI smoke gains
  this coverage; no CI job is added.
- `cargo test --manifest-path apps/desktop/Cargo.toml --locked`: **51 passed**.
  Both strict all-target Clippy checks, both format checks, Host/desktop builds,
  Windows engine cross-check, 22 Python script tests and `git diff --check` pass.
  Existing appearance, last-session, Host-choice, recent-directory, pin-order,
  completion/pending-reminder and offline-import process smokes also pass with
  schema 19 migration fixtures.

[Workspace tests](assets/project-names/workspace-tests.log),
[desktop tests](assets/project-names/desktop-tests.log),
[CLI result](assets/project-names/names-smoke.log),
[workspace Clippy](assets/project-names/workspace-clippy.log),
[desktop Clippy](assets/project-names/desktop-clippy.log),
[Windows check](assets/project-names/windows-check.log).

One preceding final regression failed in the unchanged scratch collection test,
after both writers dropped, at `scratch/tests.rs:45` with an active-lock refusal.
[Failed run](assets/project-names/workspace-tests-with-scratch-failure.log).
The [isolated test](assets/project-names/scratch-isolated.log) and next full run
pass. The intermittent cause remains unconfirmed; no scratch code, retry or
test suppression was added. This remains a regression reliability follow-up.

## Linux native fixture

`python3 scripts/workspace-names-smoke.py --fixture /tmp/fp-project-names-native`
starts private synthetic Codex/ACP Hosts with questions/permissions and two
independent 43-byte drafts. The retained
[extension](assets/project-names/extend.py) creates a real Git main checkout and
linked worktree, an independent other-Host registration, saved workspace labels
Main/Feature and three tabs. [Probe](assets/project-names/probe.py).

- **Rename project** saves exact `研究 Fleet 🚀`; both Main and Feature tabs
  and the project heading update while workspace labels and other Host groups
  retain their names. The saved group path remains visible.
  [Rendered result](assets/project-names/renamed.png),
  [saved receipt](assets/project-names/renamed.json).
- Injected SQLite update refusal preserves the old project value. Alt+F4 keeps
  the window open with the actual save error. **Reload name** after removing the
  trigger clears the close error; all edit/recovery controls remain available
  at **900×560**. [Refused close](assets/project-names/refused-close.png),
  [small failure](assets/project-names/small-refusal.png),
  [recovery](assets/project-names/reloaded-small.png).
- An independent CLI process changes the project to revision 2 while the native
  editor holds revision 1. Native reset is refused; saved revision 2 remains.
  Reload followed by explicit reset writes revision 3/None and returns both tabs
  to the derived label. Saving again at 900×560 writes revision 4 and restores
  the shared custom prefix.
  [Conflict](assets/project-names/stale-refused.png),
  [reset receipt](assets/project-names/reset.json),
  [small save](assets/project-names/small-saved.png).
- Two normal close/replacement cycles retain exact project/workspace labels,
  pins, registrations and draft rows. Native ACP composer Copy before and after
  replacement returns all **43 UTF-8 bytes**, including newline and two trailing
  spaces. Explicit Take control and Open last session are separate from name
  actions; neither pending request is answered.
  [First replacement](assets/project-names/replacement.png),
  [second](assets/project-names/replacement2.png),
  [Copy before](assets/project-names/draft-copy-before.png),
  [Copy after](assets/project-names/draft-copy-after.png).
- After the final `.git` folder-label edge correction, a fresh build renders
  the saved labels and pending ACP request, returns the same exact draft through
  native Copy, and saves an **Offline** Host project name with unchanged complete
  Host snapshots. [Final build](assets/project-names/final-build.png),
  [final draft](assets/project-names/final-draft.png),
  [offline save](assets/project-names/offline-saved.png).

All settled initial rename/failure/reload/conflict/reset intervals preserve both
entire Host snapshots exactly. Across every interval, drafts, workspace names,
pins, workspace rows and project identity rows remain exact. Reopening Codex
history changes only transport `sequence`; later explicit control changes owner
and read sequence. Those changes are not attributed to renaming.
Four Alt+F4 exits have code 0. The task workspace and owned Hosts were stopped,
their sockets removed, and the synthetic source file remains exact.
[Cleanup](assets/project-names/cleanup.json).

This is local fixture acceptance. No installed OpenCodex/DeepSeek model call,
macOS/Windows GUI, native Wayland, IME, project icon, automatic other-window label
refresh or release performance claim is made. Remote CI for this change remains
separate from local results.
