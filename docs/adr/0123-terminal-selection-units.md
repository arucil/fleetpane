# 0123 — Terminal words, logical lines and range extension

Status: accepted for the Linux prototype, 2026-10-23.

The fixed viewport in [0122](0122-bounded-terminal-selection.md) now supports
double-click word selection, triple-click logical-line selection and explicit
range extension. These remain local operations on displayed cells. They never
send input, claim a Host or allocate another PTY.

The toolkit-independent selection engine has cell, word and line units. Words
keep shell paths, punctuation and flags together; whitespace and quoting/bracket
delimiters select a single cell. Wide-cell continuation hits map to their leading
cell, and combining characters stay with that cell. Word ranges stay within a
physical row, as in the inspected Paseo behavior. Logical lines join visible
soft-wrapped rows and stop at hard boundaries or the captured viewport edge.
Copy preserves printed trailing spaces and excludes unoccupied right padding.

Dragging after a double/triple click expands by the selected unit, including
reverse drags. Shift+click converts the current unit range to cell endpoints and
keeps its opposite anchor edge. Shift+Left/Right/Up/Down/Home/End extends that
range through the same engine. Left/Right step over whole wide characters; Home
uses column zero and End uses the last printed cell of the physical row. Movement
stays inside the captured viewport. The existing snapshot input gate also blocks
terminal text, Tab and paste during these operations. Esc returns to the current
screen; keyboard extension requires an existing selection.

Resolved endpoints are cached when gestures change, rather than scanning word
boundaries for each cell during rendering. Native row hit testing continues using
the painted shaped-text geometry and the toolkit's native click count. No custom
double-click timer is introduced.

A real native trial exposed a focus-click race: releasing a single click discarded
both selection and displayed snapshot, so a second click arriving before repaint
had no picture to select. A completed focus click now returns its snapshot to the
displayed slot. A subsequent paint refreshes that slot normally; an immediate
second click can use it directly. A no-repaint state-transition regression fails
with the previous clear operation and passes with the corrected transition.

Reference: Paseo `terminal-selection.ts` at
`a486b0558168c08890890e787d55cfb88c0d7256`, especially its word separator and
whole-word selection behavior. Rust implementation and synthetic tests are new;
no source/assets copied and no history-review cursor advanced. Triple-click
logical lines and Shift extension are Fleetpane product decisions.

See [Linux native acceptance](../verification/2026-10-23-terminal-selection-units.md).
Drag-edge autoscroll, selection beyond the captured viewport, starting selection
solely with the keyboard, IME, accessibility, terminal replies/mouse reporting,
automatic geometry, other-platform GUI and active Host update transfer remain
separate work. This is not terminal compatibility or performance acceptance.
