# Cross-Host session switcher — 2026-10-22

The shared catalog and Linux X11 native fixture workflows pass locally. The
palette binds exact endpoint, Host UUID, session ID and archive membership;
colliding IDs stay distinct. Catalog tests cover activity order, the 200-entry
limit, explicit omissions/more pages and rejection of stale observations.

`cargo test --workspace --locked` passes 295 tests, and the separate desktop
suite passes 36. Both formatters and strict Clippy pass. [Logs and digests](assets/session-switcher/receipt.json).

The owned isolated desktop used two Codex fixture Hosts with 237 sessions each,
an independent ACP Host waiting for permission, a Host with no controller and
an offline endpoint. Opening showed 200 entries, one omitted loaded row and two
Hosts with more pages. Title/ID/Host/path search uses Kit defaults. Selecting
`same-003` opened the original Host banner without taking control. After explicit
takeover, the original Codex B history rendered. Direction-key selection also
retained the distinct Codex A source with the same session ID.

The initial low-level key capture missed shortcuts consumed by GPUI's action
routing. Contextual typed actions fixed Ctrl+K, Esc with a nonempty query and
Tab/Shift+Tab focus. At 900×560, status/ID/cwd details, errors, search and choices
remain reachable. A nonexistent Chinese query stays empty; Esc restores the
original composer. Exact clipboard readback retained the 33-byte Chinese draft,
including two spaces before a newline, across palette actions, normal close,
window replacement and explicit takeover. The final Host readback preserves all
four Host UUIDs, activity, pending requests and fault state. Codex B's owner
changed only at the two explicit takeovers; observational reads advance journal
sequence, so complete snapshot equality is not claimed. The ACP permission
remained unchanged. The owned desktop and fixture processes were stopped.

No installed-provider, IME, macOS/Windows, remote CI, full-index search or measured
performance acceptance is claimed. [Architecture](../adr/0095-bounded-cross-host-session-switcher.md).
