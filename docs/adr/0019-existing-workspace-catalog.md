# ADR 0019: Register existing local workspaces independently of Git branches

Status: accepted for the local Unix prototype, 2026-10-20.

## Problem

Typing a cwd for every new session does not provide the PRD's project/workspace
identity or expose which checkout an agent will use. The reference Paseo
registry distinguishes ordinary directories, local checkouts and worktrees;
its branch name is separate from the workspace identity. Fleetpane needs that
distinction before adding worktree creation and setup.

## Decision

`fleetpane-session::workspaces` owns registration, inspection, paging and
creation. CLI and GPUI Kit call the same engine. Users explicitly choose Git or
ordinary-folder registration: a Git failure is visible, never converted to an
apparently valid ordinary folder.

The controller's private SQLite store persists project/workspace UUIDs. An
ordinary folder's project identity is its canonical directory, device and inode.
A Git project's identity is the canonical common Git directory, device and
inode; its main checkout and linked worktrees share that project. A workspace
also records its own canonical directory identity and kind. Both identities are
scoped to the exact configured Host socket. Branch names, HEAD and remotes do
not participate. Aliases resolving to the same path deduplicate; separate Hosts
and checkouts remain distinct. Replaced directories or common Git directories
produce new identities; existing records refuse inspection/creation for them.
Moving a directory, deleting/recreating a repository, or switching Git/folder
registration is not an identity-preserving operation in this version.

Git observation runs off the render thread using bounded child processes. Each
invocation allows 10 seconds and at most 256 KiB each for stdout/stderr. The
status parser accepts at most 500 changes, NUL-separated porcelain v2, staged
rename source/destination, conflicts, untracked paths, detached and unborn HEAD.
Bounds, invalid bytes and unsupported output fail visibly. Inherited `GIT_*`
variables cannot redirect the request; optional locks and fsmonitor are disabled.
There are no fetch, branch-switch, index-refresh or worktree mutations. No
continuous Git polling is added. The native change list is virtualized.

**Prepare on NAME** rechecks the saved identity and selects that Host's existing
view without creating a session or taking over an existing owner. The page
reports a missing configured Host instead of selecting a substitute. Switching
away while inspection is pending prevents a late completion from selecting a
different Host. **New session** checks the prepared cwd and re-inspects identity
before executing the existing creation command. Registration/inspection never
contacts the Host. The checks do not reserve a directory atomically or create a
security sandbox; concurrent filesystem replacement and inode reuse are not
protected against.

## Storage and compatibility

Extracting controller storage from `creations` preserves the existing path and
API re-export. Local-store schema 1 upgrades to 2 in one transaction by adding
project/workspace tables; original intent UUIDs, targets and attempt flags are
unchanged. Missing versioned tables are errors, not recreated state. The private
file, NOFOLLOW, rollback journal, synchronous EXTRA and 64 MiB bound remain.
Lists expose 32 rows per page. There is no retention/removal UI yet.

`--hosts CONFIG.json` retains `CONFIG.creations/creations.sqlite3`. A single-Host
window now uses `SOCKET.with_extension("controller")/creations.sqlite3`.
Old controllers reject schema 2; switching back requires a separate store or a
backup made before migration, not editing the schema number. This controller
schema is independent of unchanged Host protocol 3 and Host journal schema 2.

[ADR 0020](0020-planned-worktree-creation.md) subsequently adds planned worktree
creation in controller schema 3, preserving this catalog and creation intents.

## Limits

This slice registers existing directories/worktrees only. Worktree creation,
deletion, branch switching, setup scripts, terminal, file browsing/editing,
workspace tabs/pinning and session-to-workspace ownership routing remain open.
Preparing from this catalog uses direct per-Host operation receipts; it does
not create an ADR 0018 durable group intent or persist the prepared GUI draft.
Git observations require a local working tree and UTF-8 paths. A large or slow
repository reports its limit rather than a partial clean result. No provider
recovery, automatic replay, remote transport or performance claim is added.
