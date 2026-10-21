# ADR 0045: Create worktrees for existing local branches

Status: accepted for the local Unix prototype, 2026-10-21.

## Problem and reference

ADR 0020 only permits a new branch. Paseo root-history review 125–126 exposed the
missing existing-branch workflow. Its broad error fallback to creating a branch
is unsuitable: an occupied, changed or missing branch must remain an explicit
failure. No upstream implementation was copied.

Git's [worktree documentation](https://git-scm.com/docs/git-worktree) distinguishes
new-branch creation from checking out a local branch. In particular, `--no-track`
is only valid when creating a branch. `--no-guess-remote` prevents the existing
mode from implicitly creating a branch from a remote. Git 2.56.0 real repository
probes and process tests verify these choices, including a same-named tag at a
different commit. The implementation uses
[`for-each-ref`'s worktreepath](https://git-scm.com/docs/git-for-each-ref) to inspect
local branch occupancy without interpreting human-readable porcelain output.

## Decision

Add typed `BranchMode::Existing` to the saved plan. New-branch plans omit the
default mode, preserving the old wire shape; old plans still decode as new.
Existing-mode records include the field, so older strict readers reject them.
There is no controller schema or Host protocol change. UUID binding includes
mode, source, branch and baseline.

An existing plan accepts a local branch name, resolves exactly `refs/heads/NAME`
and records its full commit, project/directory identities and relative cwd. The
branch must exist and be unoccupied. No network, remote guess, force, reset,
fallback branch creation or source-checkout switch is performed. Before recording
an attempt the engine rechecks the exact tip and occupancy. A moved/deleted or
newly occupied branch leaves the plan prepared and target absent; prepare a new
plan to accept a different tip.

Existing checkout uses `git worktree add --quiet --no-guess-remote -- PATH BRANCH`.
New mode retains `--no-track -b BRANCH PATH COMMIT` and adds the explicit no-guess
setting. Both retain the original hooks/fsmonitor/submodule restrictions, bounded
output and 120-second process-group deadline. Existing filters remain executable.
The created project, root, branch and HEAD must match the preview before a
completed result is recorded. An external Git writer does not honor Fleetpane's
guard; a late tip change can still create a residual checkout/catalog entry. Its
post-validation result is failed, never a claimed success or automatic rollback.

## Concurrent creation and cancelled callers

Every Fleetpane apply, in either mode, obtains a nonblocking exclusive flock on
`COMMON_GIT_DIR/fleetpane-worktree-create.lock`. A private regular file is required;
symlinks, FIFOs and public permissions are refused. Linked source worktrees and
independent controller stores share the same lock. A busy repository fails before
the attempted transition. The empty lock file remains; unlinking an active flock
file would let another process lock a different inode. This is cooperative
serialization, not a sandbox against adversarial local filesystem replacement or
external Git commands.

Apply now runs in an owned blocking task with its own Tokio runtime. Dropping its
async awaiter does not cancel checkout or release its guard while the controller
process lives: the worker
finishes, verifies and records the result, or kills/reaps at the existing deadline.
This supersedes ADR 0020's cancellation behavior for a dropped async caller.
No detached process supervisor is introduced. Closing the last native window
exits the controller process; this is not a supervised checkout handover. An
exited or hard-killed controller can still
leave Git running and an attempted receipt; process termination releases flock.
Automatic reconciliation, worktree removal, setup and safe Host retirement remain
separate work. Check still reads only the saved outcome and never resends it.

## Presentation and acceptance

CLI `plan-existing SOURCE_UUID LOCAL_BRANCH UUID` and native **Existing local
branch** use the shared engine. Existing mode hides the unrelated base input;
preview labels the existing branch and expected tip. Reopened saved plans retain
their original mode. Creating a worktree and preparing/creating an agent session
remain separate actions.

[Evidence](../verification/2026-10-21-existing-worktrees.md) covers real Git,
cross-store/linked-root locking, dropped awaiters, legacy new-branch regressions
and owned Linux native interaction. No external model or non-Linux runtime
acceptance is claimed.
