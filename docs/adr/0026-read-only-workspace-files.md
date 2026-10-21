# 0026: Demand-loaded local workspace file browsing

Accepted 2026-10-20.

Paseo exposes directory listing and bounded file reads before its editing flow.
The inspected reference at `a486b0558168c08890890e787d55cfb88c0d7256` is
`packages/server/src/server/file-explorer/service.ts`, plus the native user's
observable navigation and large read-only source scenarios described in
`packages/app/e2e/browser/file-editing.spec.ts`. No implementation is copied.

Add directory listing and file read APIs to the toolkit-independent session
engine, with CLI and native callers. These are local filesystem observations:
they do not contact, acquire or restart a Host, dispatch provider work, run Git,
execute files or write file contents. They use the registered workspace directory
identity, independently of Git's branch or repository metadata. A file browser
is not a file editor or an agent sandbox.

Open the saved root as a directory with `NOFOLLOW` and compare device/inode with
its registration. Walk children using directory-relative `openat`, rejecting
parent traversal, absolute paths and symlink components. Final regular-file
opens also use `NOFOLLOW` and `NONBLOCK`, followed by a regular-file check.
Listings identify symlinks and special files without following or opening them.
The UI disables those rows. Recheck directory/file identity after an observation;
file reads also compare size and modification/change timestamps. Changes during
an observation produce an error. This is not an atomic filesystem snapshot or a
lock against other processes; changes after the observation require refresh.

Bound one listing to 500 entries and 256 KiB of path data; excess is a visible
error, not a silently partial directory. Sort directories first and then names.
Include dotfiles. A non-UTF-8 filename makes that directory unsupported visibly.
Relative paths are limited to 4 KiB and 64 components. Text reads are limited to
256 KiB; NUL-containing/non-UTF-8 content is classified as binary. Larger files
return a size/limit notice without loading partial contents.

Filesystem syscalls may block even on an otherwise local path. Two permits per
controller process bound detached OS workers. A caller waits at most five seconds;
a timed-out/abandoned worker retains its permit until completion. No automatic
retry or second dispatch occurs. Unlike Tokio blocking-pool jobs, these workers
do not make a per-command runtime's destruction wait indefinitely. Saturation
is an explicit error. CLI processes each have their own process-local budget.

The native panel lists one directory at a time in a virtual list and offers Root,
Up, relative Go, folder refresh and file reload. Text uses Kit's read-only
textarea: focus/selection/copy remain available, edits are rejected. Its preview
is capped at 16 KiB on a UTF-8 boundary, with a visible notice; **Copy full text**
copies the exact complete loaded file. Empty/binary/large files are explicit.
Previous successful observations remain labelled after a failed operation.
Navigation and file selection are transient; no store or Host schema changes.

Editing/conflict-aware save, file creation/removal, watchers, recursive trees,
search, image preview, syntax highlighting, large-file streaming and following
links remain separate work. Git Diff remains deferred.
See [verification](../verification/2026-10-20-workspace-files.md).
