# 0074 — Navigate pending requests by exact source

Status: accepted, 2026-10-21. Implements a covered part of PRD ATT-05 and keeps
[ADR 0050](0050-observe-global-pending-requests.md)'s observation/ownership boundary.

The global Attention view had type filters and mouse source opening, but no Host
filter or keyboard selection. A row index cannot identify a request across its
two-second refresh: another Host can have the same request ID, or a request can
change scope while retaining its ID.

The toolkit-independent attention cursor binds the endpoint socket, Host
incarnation and complete original request. Refresh retains that identity across
reordering or Host label changes. Resolution, replacement, changed payload or a
filter that hides the selection clears it without selecting a substitute. A
failed whole-queue refresh also clears selection and disables source opening;
unavailable Hosts remain explicit. Items lacking a session remain inspectable
and copyable but keyboard navigation skips them.

The queue combines exact Host and request-kind filters. While its focus handle is
active, Up/Down and Home/End select routable rows, Enter runs the existing checked
source navigation, Ctrl+C copies the original JSON and Ctrl+R refreshes. Movement
stops at the ends. Visible text/border mark the selection, and explicit movement
scrolls it into view. Refresh does not force the user's scroll position. A
selection invalidation returns to the first row and shows a notice.

These actions only observe/navigate. Source confirmation still rechecks the Host
and complete pending payload. Opening does not claim, answer, resume or stop;
original session controls and explicit takeover remain required. The queue focus
is independent of retained composers and terminal input; ordinary typed letters
must not reach a hidden draft.

This is queue-local navigation, not a global next-request command. Filters and
selection remain window-local. Global shortcuts, configurable bindings, request
creation times, completion/failure events, notifications and mute/tray behavior
remain separate work. No provider expansion follows from this slice.
