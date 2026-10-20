# 0021: Demand-loaded read-only workspace diffs

Accepted 2026-10-20 for the local Unix workspace foundation.

## Context

The saved workspace catalog exposes Git status but cannot inspect a selected
file's changes. PRD WS-04 and DEV-03 require local Diff review. The reference is
Paseo `a486b0558168c08890890e787d55cfb88c0d7256`, particularly
`packages/server/src/utils/checkout-git.ts` (`getTrackedDiffTextForPath`,
`getUntrackedDiffText`, `getCheckoutDiff`) and its workspace service. It loads
literal paths and bounds per-file output. This implementation reuses that
workflow, without copying source or importing Paseo's checkout cache/protocol.

## Decision

`fleetpane-session::Workspace::diff(Scope, path)` is the shared engine for CLI
and native workspace review. Scope is explicit: HEAD→index (`staged`) or
index→working tree (`unstaged`). Git handles an unborn HEAD using the empty tree.
It is a local filesystem observation; it needs no Host connection or lease and
changes no Host protocol, journal or controller-store schema.

Each request freshly validates the saved workspace/repository identity, finds the
selected worktree-root-relative path in Git's current status, and loads one file.
Tracked comparisons use top-level literal pathspecs. Staged renames include both
source and destination; unstaged uses the destination's index entry. Status and
diff explicitly enable 50% rename detection, avoiding incompatible behavior when
repository `diff.renames` is false. The real-Git regression first failed at that
configuration boundary and passes with the consistent rule.

Untracked regular files and symlinks use `git diff --no-index /dev/null`;
exit 1 is accepted only for that comparison. Git displays symlink target text.
Directories, special files, and inputs over 1 MiB receive explicit unavailable
results. Unmerged indexes are reported as conflicts, rather than represented as
an ordinary two-side patch. Binary changes retain Git's metadata summary.

Git runs without optional locks, inherited `GIT_*`, fsmonitor, external diff or
textconv. Output format, prefixes, colors, relative paths and diff algorithm are
explicit. The existing 10-second subprocess deadline and 256 KiB per-stream bound
apply. Text patches also allow at most 8000 lines and 16 KiB per line. Non-UTF-8
text/paths and NUL-containing forced text fail visibly; output is not silently
truncated. These limits bound returned/rendered data, not Git's peak RSS.

The desktop runs filesystem/Git operations off the render thread, retains only
one result and Kit `DiffState`, and uses Kit's virtualized unified Diff. It clears
the previous result before loading, shows scope and observed HEAD, and provides
refresh, back and exact raw-patch copy. Rendering failures are explicit while raw
output remains copyable. Workspaces and provider views keep independent state.

## Limits

Identity and HEAD/branch are rechecked after comparison. They do not lock the
repository or provide an atomic content/index snapshot. Same-status concurrent
edits may occur, and symlink/directory replacement races are not a security
boundary. There is no automatic watcher, commit/base-ref comparison, untracked
directory expansion, Git write, merge resolver or ownership attribution to an
agent. A rename's metadata can retain Git's quoted filename notation.

[Verification](../verification/2026-10-20-workspace-diffs.md) distinguishes real
Git/native rendering from synthetic provider isolation and remote CI.
