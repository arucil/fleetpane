# 0092 — Retain conversation state when hiding its directory

Paseo history 568–594 evolves the sidebar from mobile slide navigation to a
desktop shortcut, with repeated animation synchronization and list-cap fixes.
Fleetpane can offer more conversation space without replacing a Host view or
changing provider state.

Each native Host view retains a visibility flag. Hiding omits only the directory
element; directory metadata/scroll, inputs, conversation cards, pending requests
and revisioned drafts keep their existing state. The visible toggle is always
reachable. A Host-scoped key capture handles one Ctrl/Cmd+B press and consumes
repeat events. Focused sidebar inputs relinquish focus when hidden; other input
focus stays unchanged. No filesystem, provider query, control transfer or send
occurs on toggle. Visibility starts open in a new window and is not a device
preference in this slice.

Linux native focus, exact clipboard/draft and pending-approval snapshot evidence
is recorded separately from compiler checks. Other platforms, IME and persistent
layout preferences remain open.
