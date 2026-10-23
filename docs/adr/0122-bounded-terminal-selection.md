# 0122 — Terminal selection from a fixed visible viewport

Status: accepted for the Linux prototype, 2026-10-23.

The PTY screen previously offered whole-screen Copy and direct keyboard input,
but no mouse selection. A selection tied only to row numbers could copy different
text after a shell scrolls, rewrites a line or enters its alternate screen.

Dragging now selects a fixed snapshot of the displayed emulated viewport. The
toolkit-independent session engine captures only visible cells, wrap boundaries
and cursor metadata, bounded by the existing 60×200 native screen limit. It does
not clone the scrollback history or create another PTY owner. The desktop retains
its currently rendered viewport so a new drag uses the picture being displayed,
rather than a later output observation.

Native row hit testing uses the same shaped text, font/style runs and painted
bounds as the renderer. Byte offsets map back to terminal cells; Chinese wide
cells and combining sequences stay whole. Reverse drags normalize their endpoints.
Copy joins soft-wrapped rows, inserts newlines at hard row boundaries, removes
unoccupied right padding and preserves spaces actually printed by the PTY. ANSI
styles remain visible and do not enter clipboard text.

The UI identifies the fixed selection. Background output reads and already
accepted operations continue. New terminal text/keys, paste and context-bound
Tab are blocked while selected. Ctrl+Shift+C (or Cmd+C) and **Copy selection**
copy the selected text; **Copy screen** copies the displayed snapshot. **Esc**
clears it; **Live output** also returns to the live scroll position. Page up/down
clear the old selection before moving. A plain click focuses without retaining
a one-cell selection; releasing a drag outside the screen terminates the gesture.
Terminal or Host replacement clears the old selection. Window replacement keeps
the existing Host and shells; selection is transient and is not restored.

This is desktop mouse selection within a bounded snapshot, including a selected
scrollback viewport. Double-click word selection, triple-click logical lines and
keyboard range extension now have a separate [contract](0123-terminal-selection-units.md)
and acceptance. Drag-edge autoscroll, IME, terminal mouse/query replies, automatic PTY geometry,
other-platform GUI acceptance and active PTY transfer during Host updates remain
separate work. This does not establish terminal compatibility or performance.

Reference: `packages/app/src/terminal/native-renderer/terminal-selection.ts` and
its test at Paseo `a486b0558168c08890890e787d55cfb88c0d7256`. Those sources
provided behavioral context for ranges, coordinates and wrap-aware Copy; the
Rust implementation and synthetic tests were written here. No code/assets copied.

See [native verification](../verification/2026-10-23-terminal-selection.md).
