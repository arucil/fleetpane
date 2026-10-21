# 0087: Validate session directories at the Session Host boundary

Accepted 2026-10-22.

Paseo replay 524 adds server-side directory validation. Fleetpane's shared Create
command checked only absolute syntax; a raw request using a nonexistent path was
accepted and sent to the synthetic provider. Provider behavior alone cannot give
Codex and ACP one creation contract.

Before fresh `thread/start` admission, the Host validates a string cwd, absolute
syntax, 4 KiB bound, absence of NUL, and filesystem directory type. Return the
explicit `invalid_workspace_directory` error without journal or provider effects.
Preserve the exact supplied path and allow symlinks resolving to directories.
Do not create directories, change paths, infer Git requirements or alter resume.

Filesystem checks run outside the Host event loop, with at most two workers and
one total two-second deadline. Timed-out workers keep their permits until exit.
The preflight is an observation, not a lock: provider use can still fail if the
directory changes afterward.

The existing lease/admission and durable operation-ID checks take precedence over
the prepared result. A duplicate returns its original receipt even when its cwd
was subsequently removed; a changed payload stays an operation conflict. Errors
use existing GUI/CLI handling, preserving workspace input and session drafts.
