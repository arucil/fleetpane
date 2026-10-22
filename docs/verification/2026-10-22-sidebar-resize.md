# Native session sidebar resizing — 2026-10-22

The session directory now uses GPUI Kit resizable panels with a retained state
per Host view. Dragging the divider and Narrow/Widen/Reset width share Kit's
size constraints: 240 px minimum, at most 520 px and at least 480 px for the
conversation. Width is window-local, starts at 320 px and remains separate from
sidebar visibility, provider state and persisted drafts.

The initial integration failed native QA: a first Widen could paint narrower and
a drag stayed stale. The externally created `ResizableState` had no parent view
observer. Adding the retained GPUI subscription makes state changes schedule the
Host view redraw; no timer, forced reload or registry patch was added. Continuous
mouse movement is necessary to exercise GPUI drag initiation; the workspace's
single-jump helper alone did not reproduce a human drag.

An isolated Linux X11 window verifies drag, the three buttons, 240–420 px bounds
at 900×560, shrink/grow, hide/show, separate Host layouts, and Light appearance
without losing the selected session or 360 px width. Exact clipboard readback
before and after normal window replacement matches the 35-byte Chinese multiline
draft. Reopening starts the sidebar at 320 px; opening saved history explicitly
restores the draft. The complete independent ACP snapshot, including its pending
permission, is identical before and after the final normal close.

`cargo test --manifest-path apps/desktop/Cargo.toml --locked -j 2` passes 36 tests.
Desktop build, format and strict all-target Clippy pass. The change is desktop
layout only. [Receipt, screenshots and logs](assets/sidebar-resize/receipt.json)
retain source/binary identities and the rejected synthetic-fixture probe.
Persisted width, tabs, IME, Wayland and macOS/Windows acceptance remain open.
