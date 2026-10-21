# Literal command paths — 2026-10-22

A regression with the same literal filename in the conversation root and a
Chinese command subdirectory fails with the previous root-relative resolution.
Provider read links now resolve against the command's cwd. Absolute paths remain
absolute; `%20`, `#`, colons and trailing spaces are literal filename characters.
The original conversation identity still scopes the request, and workspace open
retains its root, parent-traversal, symlink and special-file checks. Invalid
relative command directories produce a visible card error.

`cargo test --workspace --locked` passes 271 tests. Separate desktop tests pass
35. Strict Clippy passes for both workspaces; process checks pass for
`file-navigation-smoke.py`, `creation-directory-smoke.py` and `images-smoke.py`.
[Receipt and logs](assets/command-cwd/receipt.json) include the failing and passing
regression and the exact source/binary hashes.

In the isolated Linux native desktop, clicking the provider read link opens the
subdirectory file despite a different same-name root file. Clipboard readback
matches all 33 bytes, including CRLF, Chinese text and trailing spaces. Returning
to the conversation preserves the exact 43-byte draft. Snapshot readback shows
zero journal operations, no active turn, no pending request and no fault. The
window closes normally and the owned workspace is stopped.

Only synthetic provider/history data was used; no external model, shell action,
remote path, non-Linux native or performance acceptance is claimed. Earlier
web-search, command-actions and creation-directory receipts retain their own
source/binary identities rather than being rewritten to describe this build.
