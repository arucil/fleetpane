# Native session sidebar — 2026-10-22

The selected Host has **Hide sessions / Show sessions** and Ctrl+B (Cmd+B on
macOS). A native key capture works while the composer or filter has focus;
auto-repeat does not repeatedly toggle. Hidden sidebar inputs relinquish focus.
Visibility is window-local and retained independently for each Host view.

An isolated Linux X11 desktop verifies the button and Ctrl+B, exact 32-byte
Chinese multiline draft clipboard before/after collapse and after normal close
and reopen, a retained title filter which cannot receive input while hidden, and
900×560 layout. A synthetic pending command approval remains accessible after
scrolling; full Host snapshots before/after toggles and local edits are identical.
One explicit Decline settles the original request. A separate 22-byte follow-up
draft and the original draft remain saved. The owned workspace stops normally.

`cargo test --manifest-path apps/desktop/Cargo.toml --locked -j 2` passes 36 tests.
Desktop build, format and strict all-target Clippy pass. [Receipt and raw
evidence](assets/sidebar-toggle/receipt.json) bind the checked source and binary.
No engine/provider code changed. No new external model, Wayland, macOS/Windows,
IME, persisted visibility or configurable shortcut acceptance is claimed.
