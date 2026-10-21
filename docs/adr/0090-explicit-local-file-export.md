# Explicit local file export

Paseo history 554–555 adds downloading workspace files. Fleetpane's current Hosts
use local Unix sockets; its useful equivalent is a new local byte copy from the
saved workspace, shared by the desktop and CLI. HTTP tokens and mobile sharing
remain outside this slice.

The user chooses an absolute destination. Export reads current bytes from a
regular file, using existing workspace identity and no-follow traversal. It does
not serialize the preview or transcode text. A 64 KiB copy buffer and 64 MiB file
limit share the existing two-worker bound. Recheck source metadata/path identity
and destination folder identity before publishing.

Write a random private sibling spool, sync it, then hard-link it to the requested
new name. Publication refuses an existing file, directory or symlink atomically,
including a concurrent exporter. Remove the spool and sync the directory. Errors
after publication explicitly say the copy exists; cleanup errors name the spool.
Copies use mode 0600 and return exact bytes, destination and SHA-256.

An admitted write waits for its result; a read timeout cannot imply that a write
was cancelled. The native view retains the task and prevents leaving the view or
normal window close while exporting. There is no automatic retry, overwrite,
destination directory creation or live Host mutation. Force termination and
filesystem crash recovery remain separate work; a private spool can remain.
