# ADR 0020: Preview a pinned worktree creation and retain its outcome

Status: accepted for the local Unix prototype, 2026-10-20.

## Problem and reference

An existing-directory catalog does not satisfy PRD WS-02's independent worktree
workflow. Creating a checkout also introduces filesystem and Git side effects
which cannot be rolled back safely after an interrupted controller.

Reference inspection used Paseo `a486b0558168c08890890e787d55cfb88c0d7256`:
`paseo-worktree-service.ts` preserves the selected project's relative cwd, while
`utils/worktree.ts` separates baseline/branch intent, derives a managed root and
uses a 120-second Git creation deadline. No implementation was copied. Fleetpane
does not copy its automatic collision suffixes or setup/bootstrap behavior.

## Decision

The shared session engine exposes plan, apply, check and paged list operations.
CLI and GPUI Kit invoke those same operations. A plan records UUID, source
workspace/Host/project identity, canonical destination-parent identity, new
branch, user baseline ref, resolved full commit, worktree path and session cwd.
The user sees these before explicitly applying the plan.

Git validates the branch; existing/conflicting branch refs are rejected while
planning. `rev-parse --verify --end-of-options REF^{commit}` pins a local commit,
never a moving branch at apply time. No fetch occurs. If the source workspace is
a repository subdirectory, that directory must exist in the pinned tree and the
new session cwd retains its relative path. Uncommitted source files are not copied.

Destinations are deterministic `STORE/worktrees/UUID` beneath a private parent,
independent of branch text. Existing files, folders and symlinks are refused.
Source repository and destination-parent identity are rechecked before dispatch.
The plan cannot be rebound to a different source/branch/baseline by UUID reuse.
These checks do not reserve filesystem paths atomically or protect against inode
reuse/adversarial local replacement; worktrees are not a permission sandbox.

The private controller database adds `worktree_jobs` in schema 3. Versions 1 and
2 upgrade transactionally while preserving creation intents/attempt flags and
workspace IDs. Versioned missing tables are errors. The existing 64 MiB database
budget remains; worktree files have separate disk usage. Host protocol 3 and Host
journal schema 2 do not change. Older controller binaries reject schema 3.

## Execution and failure behavior

The state transition is `prepared → attempted → completed | failed`. A SQLite
conditional update commits attempted before invoking Git; only one caller may
pass it. A prepared precondition failure does not attempt Git. Git failures never
reset the attempt flag. Another UUID is a distinct operation, not semantic
deduplication. Source preparation does not create or claim a provider session.

Git creates a new branch with `worktree add --no-track -b BRANCH PATH COMMIT`.
Hooks, fsmonitor and recursive submodule checkout are disabled; inherited
`GIT_*` routing variables cannot redirect the command. No setup script runs.
Configured Git checkout filters may execute, so this is not a process sandbox.
Stdout/stderr allow 256 KiB each. A 120-second deadline kills the process group
and reaps the direct child; cancellation also signals the group. Partial
directories, branches, administrative entries or lock files are not deleted.

After Git succeeds, the catalog registers the new cwd and verifies its project,
worktree root, branch and HEAD against the plan. Failed results identify the
known completed steps, error and possible residual directory/branch. Failure to
persist the final result leaves attempted. There is no automatic cleanup or
retry. Successful registration and creation of an agent session remain separate.

Check reads only the saved database outcome. It never claims a Host, invokes Git,
registers an arbitrary directory as a recovered success, or resends the mutation.
A controller crash can leave an attempted plan with Git still running, or a
finished checkout whose result was not saved. Both stay unresolved. A detached
operation supervisor and verified reconciliation are future work; inspect those
residuals with Git before taking further repository actions.

## Native presentation and limits

The workspace page retains the latest 32 creation records on reopen. Prepared
plans offer explicit Create; completed records open the registered workspace;
failed/attempted records show their status without a retry control. Full paths
are copyable; long directory labels are bounded. The CLI exposes older pages.

No worktree removal/archive, switching to an existing branch, setup/teardown,
terminal, file editing, automatic Host selection, provider restart recovery or
durable session ownership routing is added. Safely deleting worktrees requires
active-session ownership and uncommitted/unpushed-work preservation first.
