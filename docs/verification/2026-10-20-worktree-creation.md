# Planned worktree creation — local verification, 2026-10-20

This covers PRD WS-02's new-branch worktree creation. It does not establish full
Paseo parity, safe worktree deletion, or recovery of running Git after a crash.
See [ADR 0020](../adr/0020-planned-worktree-creation.md). Reference inspection used
Paseo `a486b0558168c08890890e787d55cfb88c0d7256` and the worktree service/utilities
listed there. Reference tests were not run; no source was copied.

## Automated verification

On Linux/Rust 1.98.1:

- `cargo test --workspace --locked`: 79 tests pass. New tests cover input bounds
  and v2→v3 migration preserving workspace/project IDs. Existing v1 migration
  preserves creation UUIDs/targets/attempts; missing versioned tables are refused.
- `cargo test --manifest-path apps/desktop/Cargo.toml --locked`: 4 tests pass.
- Both manifests pass strict Clippy (`--all-targets --locked -- -D warnings`),
  formatting and builds. `uvx --offline ruff check scripts`, formatting of the new
  script, and `git diff --check` pass.

`python3 scripts/worktrees-smoke.py` runs real Git and the production shared
engine/CLI with a private synthetic Codex Host. It verifies:

1. A selected subdirectory maps to the same relative path in the new worktree.
   Planning pins a full commit; moving `main` afterward does not change it.
   UUID reuse returns the original plan; different input with that UUID fails.
2. Two concurrent applies admit only one Git creation. The linked checkout shares
   the project UUID, has the requested new Unicode branch and original commit,
   and leaves the main index hash unchanged. Untracked source files are not
   copied. A real post-checkout hook marker is not created.
3. Workspace session creation sends the canonical new cwd to Codex. Planning,
   worktree creation and all error/status cases leave Host snapshots unchanged;
   the separately requested provider creation is the only new Host mutation.
4. Invalid/option-shaped branches, existing/conflicting refs, missing baselines,
   missing baseline subdirectories, replaced source repositories and replaced
   destination parents are refused. A pre-existing target directory/file is retained and
   its plan remains prepared, without dispatch.
5. A branch appearing after preview causes a saved failed result and no retry.
   A Git wrapper deliberately leaves a partial directory and exits with an error;
   the failed result reports it and its contents remain intact.
6. Another wrapper executes real Git, then holds its response. Killing the
   controller after that observed effect leaves attempted after process
   replacement; the real worktree exists, and subsequent apply is refused.
7. A wrapper and sleeping descendant hold the Git pipes for 300 seconds. The
   production 120-second deadline returns failed and stops the descendant;
   `ps` shows it absent or exited/zombie, never still running. The direct child
   is reaped. The fixture also pages through more than 32 saved creation records.

The wrappers inject deterministic I/O failures; they are not external model or
third-party repository tests. `scripts/workspaces-smoke.py` and
`scripts/creations-smoke.py` also pass after the schema/storage changes. Unix CI
now includes worktree creation smoke (about two minutes due to the real deadline).
Remote CI and macOS/Windows runs were not performed.

## Native verification

GPUI Kit 0.7.3 ran in isolated X11/software Vulkan workspace
`fleetpane-worktrees-qa` with three private fixture Hosts. No shared Paseo,
Codex/DSH process or external model request was used. Canonical task evidence is
`/tmp/fleetpane-worktrees-20261020/receipt.json`, with source/binary hashes,
process identities, saved plans, Host/history readbacks and cleanup checks.

- Registered a real repository subdirectory and entered a new Chinese branch and
  `main` baseline. Preview displayed Host, branch, resolved SHA, target and session
  cwd; the target was absent and Host mutation totals stayed unchanged.
- Closed the desktop, advanced `main`, and reopened the same configuration. The
  original prepared UUID/SHA remained. Create produced and registered a linked
  worktree at the pinned older commit. Main retained its newer file contents and
  clean status. A separate edit in the new checkout left main unchanged.
- Open workspace → Prepare → explicit Take control → New session created one
  ACP session in the new checkout's selected subdirectory. A Chinese prompt
  completed; authenticated Host queries confirm cwd, completed turn and items.
- A real branch collision was injected after a separate preview. Its failed
  record reloaded and showed Git's error and possible residual path, with no
  Create/retry button or previous successful result. Native Check saved status
  left all three complete Host snapshots identical before/after.
- Long paths are bounded, with explicit copy controls. A 335-character folder
  path showed an ellipsis; clipboard readback equals its complete original text.
  The final build also retains the completed/failed records and new workspace.
- The old drained Host's original approval and process IDs survived creation and
  the new session. Its Chinese draft survived visiting Workspaces and returning.
  Skip this tool completed that original turn. The unused Codex Host journal
  remained empty. Task-owned desktop/Hosts/providers/sockets were cleaned up and
  the isolated workspace stopped.

Selected synthetic screenshots:

- [Preview before creation](assets/worktree-creation-preview.png)
- [Completed registration](assets/worktree-creation-completed.png)
- [Independent Git changes and native controls](assets/worktree-creation-workspace.png)
- [Saved failure without retry](assets/worktree-creation-failed.png)
- [ACP session in the new worktree](assets/worktree-creation-session.png)
- [Long path with complete clipboard value](assets/worktree-creation-long-path.png)

## Limits

The native attempted/incomplete display and native >32-record history were not
separately exercised; process tests cover interruption and engine paging. The
native successful creation used the same engine before a later change adding
more explicit failure-step text; final engine tests/smoke and final desktop
build/render/copy checks passed afterward. No further successful GUI Git
mutation was repeated for that wording change.

An attempted record is not success or failure until an outcome was saved. A
hard-killed controller may leave Git running or a finished unregistered checkout.
No detached supervisor, automatic reconciliation, replay, rollback or cleanup is
provided. Check does not inspect Git/filesystem state. Replacement checks use
path/device/inode, not an atomic filesystem reservation or security sandbox.

Existing-branch worktrees, archive/removal, setup/teardown, terminal/file editing,
automatic session routing, live DeepSeek/OpenCodex model acceptance and
performance comparisons remain open. Worktree disk usage has no catalog quota;
the 64 MiB bound covers only the controller database.
