# Existing project/workspace catalog — local verification, 2026-10-20

This verifies existing-directory registration and read-only Git details, not
complete Paseo workspace parity. Reference inspection used the local
`paseo-arucil` checkout at `a486b0558168c08890890e787d55cfb88c0d7256`, especially
`workspace-registry.ts` and `new-workspace-screen.tsx`. Its tests were not run.
The implementation decision is [ADR 0019](../adr/0019-existing-workspace-catalog.md).

## Shared engine and process verification

On Linux/Rust 1.98.1, `cargo test --workspace --locked` passes 77 tests and
`cargo test --manifest-path apps/desktop/Cargo.toml --locked` passes 4. New tests
cover porcelain-v2 records, rename source/destination, conflict, detached/unborn
HEAD, invalid/oversized input, v1→v2 local-store migration preserving creation
bindings/attempts, and refusal to recreate missing versioned tables.

`python3 scripts/workspaces-smoke.py` uses real local Git repositories and private
synthetic Codex/ACP Hosts through the production CLI/shared engine. It verifies:

- Main checkout and linked worktree share a project UUID and have distinct
  workspace UUIDs. Symlink aliases deduplicate; changing branch/remote retains
  identity; the same directory on another Host remains separate.
- Real modified, staged-rename and untracked records retain spaces, newline and
  Chinese names. Detached and unborn HEAD are represented. Repeated inspection
  leaves the Git index byte-for-byte unchanged; linked edits leave the main
  checkout clean. Inherited `GIT_DIR`/`GIT_WORK_TREE` cannot redirect observation.
- Registration/inspection leave Host ownership, journal totals, admission and
  process identities unchanged. Saved-cwd creation works for Codex and ACP.
  Missing/replaced directories and replaced common Git directories refuse old
  records before claiming or growing the Host journal. New registration gets
  new identities. Git-on-ordinary-folder is an error; folder mode is explicit.
- Paging across more than 32 records is bounded and non-overlapping. A private
  fake Git executable exceeding 256 KiB is rejected. Another sleeps for 30
  seconds: the production 10-second subprocess deadline terminates and reaps it.

`python3 scripts/creations-smoke.py` still passes after extracting shared storage:
immutable targets/UUIDs, killed-controller pending receipts, restarted-Host
unknown outcomes, stale admission refusal, and no second dispatch.

Both manifests pass strict Clippy (`--all-targets --locked -- -D warnings`),
format checks and builds. `uvx --offline ruff check scripts`, formatting of the
new smoke script, and `git diff --check` pass. Unix CI now includes workspace
smoke; remote CI was not run. No macOS/Windows acceptance is claimed.

The production CLI also registered and inspected the actual Fleetpane source
checkout on `feat/session-host-foundation`, correctly reporting the root and
dirty status. Its Git index hash was identical before/after. This read did not
create a session in that checkout.

## Native verification

GPUI Kit 0.7.3 ran in isolated X11/software Vulkan workspace
`fleetpane-workspaces-qa`, with three private provider fixtures and an offline
configured endpoint. No external model request or shared Paseo/Codex/DSH process
was used. Task-owned evidence is
`/tmp/fleetpane-workspaces-20261020/receipt.json`, with source/binary SHA-256s,
Host/provider identities, history readback, database records and cleanup checks;
logs and intermediate screenshots are in that directory.

1. Registered the real main checkout and linked worktree from the window.
   Details displayed the shared project ID, distinct workspace IDs, main/linked
   membership and all three changes. Newline filenames appeared as escaped text
   rather than breaking rows; Chinese filenames rendered correctly.
2. A private v1 controller database containing an existing attempted creation
   upgraded to v2 when opening Workspaces. Every original creation column stayed
   equal. Closing/reopening the desktop preserved both workspace IDs; an external
   branch rename appeared after Refresh with those IDs unchanged.
3. Preparation selected the saved New DeepSeek endpoint/cwd. Editing the cwd and
   clicking New session showed a mismatch error with no new Host journal entry.
   In the final build, removing the saved endpoint from configuration produced
   an explicit missing-Host error and cleared the previous details; no substitute
   Host or session was selected. Restoring configuration preserved the catalog.
4. The final build prepared the saved linked worktree, required explicit takeover
   of the existing owner and created exactly one ACP session. A Chinese prompt
   completed with fixture reasoning/tool/text activity. Authenticated Host queries
   confirmed the canonical linked cwd, one completed turn, prompt and reply.
5. Git registration of an ordinary folder showed Git's error with no stale Git
   success details. Explicit Save folder then showed ordinary-folder details.
6. The old drained ACP Host kept its original running permission and process
   identities while the new session completed. Its Chinese composer draft
   survived visiting Workspaces and returning. Skip this tool completed the
   original turn; the unused Codex Host's journal stayed empty throughout.

All task-owned desktops, Hosts/providers and sockets were cleaned up, and the
isolated workspace was stopped. Selected synthetic screenshots:

- [Persisted linked worktree and Git changes](assets/workspaces-catalog.png)
- [Missing configured Host](assets/workspaces-missing-host.png)
- [Session created from the saved workspace](assets/workspaces-session.png)
- [Old approval and retained Chinese draft](assets/workspaces-old-draft.png)

## Remaining acceptance

Native 32-record pagination, deadline/oversize error presentation, single-Host
store-path behavior and the late-Prepare navigation guard were not separately
exercised; engine paging and I/O limits were. This is fixture provider acceptance,
not a live DeepSeek/OpenCodex model test or a performance benchmark. Directory
checks use path/device/inode, do not protect against inode reuse, and do not
atomically reserve a directory against concurrent replacement.

Worktree creation/removal, branch switching, setup scripts, terminal/files,
workspace tabs/pinning, durable session ownership routing and full provider
restart recovery remain open. Workspace preparation is memory-only and direct
creation uses existing Host receipts, not the durable group-intent workflow.
