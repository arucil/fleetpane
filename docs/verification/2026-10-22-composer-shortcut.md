# Composer shortcut — 2026-10-22

Ctrl/Cmd+Enter uses the focused composer's existing Send or active-Codex Steer
action. Enter and Shift+Enter continue inserting newlines. The Kit action is
captured before the textarea's default newline operation.

Owned Linux X11 workspace `fleetpane-shortcut-20261022` verifies exact Chinese
multiline send and active-turn steer, matching the original thread/expected turn.
Provider rejection remains visible and keeps `[options-fail]`. A shortcut in the
session search field creates no send; an empty composer creates no send. The
32-byte Chinese multiline draft survives normal window close/reopen, and sends
exactly after the final binary rebuild. The fixture records four send attempts
(one rejected), one steer and one explicit Stop: six journal operations, no fault,
no outstanding work. Both windows close normally; the owned workspace is stopped.

`cargo test --manifest-path apps/desktop/Cargo.toml --locked` passes 35 tests.
Desktop format, build and strict Clippy pass after the final code change. Raw
fixture effects, snapshots, screenshots and source/binary hashes are in the
[receipt](assets/composer-shortcut/receipt.json).

This is synthetic Codex/local Linux acceptance. Marked IME text is passed to the
input engine, but IME, macOS/Windows shortcut mapping and shortcut-specific native
ACP/image/read-only refusal have not been independently accepted. Their existing
button/command checks are reused. No force-stop, automatic retry or queue added.
