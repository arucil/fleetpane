# Host-owned creation directory checks — 2026-10-22

The old Host accepted nonexistent cwd through raw `thread/start`; the new process
regression rejects it before admission. Codex and ACP fixtures each reject absent,
regular-file, FIFO, broken-link, relative, empty, non-string, NUL and overlong paths
without changing sequence, journal, activity, pending requests or Host identity.
Each accepts exact Unicode/trailing-space directories and existing directory
symlinks. Removing each path and resending the same UUID returns the identical
saved operation, without another dispatch.

`cargo test --workspace --locked` passes 270 tests; separate desktop tests pass 35.
Both formatters and strict Clippy pass; Windows GNU engine Clippy and 16 Python
script tests pass. Process checks pass for `creation-directory-smoke.py`,
`session-smoke.py`, `acp-smoke.py`, `creations-smoke.py` and `images-smoke.py`.
[Source, logs and process receipt](assets/creation-directory/receipt.json) record
the coverage separately from native behavior.

In the owned `fleetpane-create-20261022` desktop, Codex visibly rejects an absent
directory and ACP visibly rejects a file path. Both retain the input and recover
by explicitly creating one valid session in a Chinese directory. Each Host has
exactly one journal operation, unchanged process identities and no fault. At
900×560 both sessions remain usable. A 40-byte Chinese multiline draft survives
Host switching and native window replacement with exact clipboard readback.
Replacement requires explicit control; no turn was sent. The window closes
normally and the owned workspace is stopped.

This is local synthetic-provider acceptance. Directory checks cannot prevent a
filesystem change between check and provider use. They do not provide directory
creation, remote-filesystem support, Host updates, external-model or non-Linux
native acceptance.
