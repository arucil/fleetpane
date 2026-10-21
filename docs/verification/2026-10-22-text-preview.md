# Complete source preview — 2026-10-22

Covered Linux X11 native fixture workflows pass locally. The workspace reader
is unchanged; the desktop now uses Kit's read-only Editor for the complete
loaded UTF-8 string, numbered lines, enabled syntax grammars and soft wrapping.
Unknown extensions remain plain text. [Decision](../adr/0098-complete-read-only-source-preview.md).

The isolated desktop loaded a 36,944-byte Rust file with Chinese text, CRLF,
trailing spaces, a long fifth line and 1,202 numbered editor rows. The top and
tail were reachable; the long line wrapped or horizontally scrolled to its
LONG_END marker. The 900×560 window retained the gutter, controls and preview.
Empty and unknown-extension files rendered explicit empty/plain text states.
The Copy full text result and the selected read-only editor result both matched
the entire source byte-for-byte, including after an attempted edit. These are
clipboard readbacks, not screenshot-only claims.

An assistant source link opened original line 1000 while preserving access to
line 1. Missing line 9999 displayed an error and retained exact full-file Copy.
After normal window close and launch, explicit takeover and Open last session
recovered the same 24-byte Chinese multiline draft with trailing spaces; its
clipboard readback matched the original. The independent ACP Host's complete
snapshot, including its pending permission, stayed identical during the first
file-preview pass. No prompt or approval was sent.

`cargo test --manifest-path apps/desktop/Cargo.toml --locked` passes 36 tests,
including original source-line and absent-anchor regression coverage. Desktop
format check, build and strict all-target Clippy pass. Existing file/navigation
smokes cover the unchanged reader boundaries. [Screenshots, fixtures and check
receipt](assets/text-preview/receipt.json) bind the evidence to source hashes.

No installed-provider, macOS/Windows, IME, accessibility, performance or complete
editor/search acceptance is claimed. Source links do not create attachments or
modify files. Editing/conflict saves, recursive trees and streaming remain open.
