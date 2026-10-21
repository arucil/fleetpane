# Bounded cross-Host session switcher

Accepted 2026-10-22.

Paseo history 853–877 identifies fast keyboard navigation across sessions as a
useful extension beyond a paged directory. Fleetpane uses the GPUI Kit Command
palette with typed actions and a shared, frozen identity catalog.

Opening explicitly reads one bounded metadata page on each configured Host,
without acquiring a lease. One request set may run at a time, including after
close/reopen. The catalog uses the existing activity ordering, includes at most
200 entries and reports omitted loaded rows, Hosts with more pages, unavailable
Hosts and missing controllers. Typing searches only that catalog; All sessions
retains complete paging and archived membership. No transcript scanning occurs
while painting or typing.

Every entry binds socket, observed Host UUID, exact session ID and archive
membership. Selection calls the existing directory-opening path, preserving
unsent drafts, explicit takeover, Host replacement checks and read-only history.
It does not resume provider context, send a prompt or answer a pending request.
A generation fences deferred confirmation from an earlier palette instance.

Ctrl+K (Cmd+K on macOS) toggles the palette. Esc closes even with query text;
Tab/Shift+Tab retain search focus. Closing restores the previously focused
control. GPUI action dispatch precedes low-level key capture, so contextual typed
actions must handle shortcuts and override Input/Command Escape rather than
assuming capture_key_down always sees them. Pointer occlusion prevents clicks
through the overlay; Kit supplies text editing, search and list selection.

The palette and selected session are window-local. Persisted layout, global
full-text search, archived palette entries and configurable keybindings remain
separate. [Local evidence](../verification/2026-10-22-session-switcher.md).
