# 0109 — Open the next pending request across Hosts

Accepted, 2026-10-22. Extends ADR 0074 and PRD ATT-05.

The existing queue has exact-source selection and checked navigation, but its
keyboard movement only works inside the queue. Add **Next request** and
Ctrl+Shift+J on Linux, Cmd+Shift+J on macOS. Each explicit gesture observes every
configured Host, cycles through routable requests in configured Host/request
order and reuses the existing source confirmation and ownership boundary.

A separate toolkit-independent cursor binds socket, Host incarnation and complete
original request. It wraps, skips requests without a session and starts from the
first current request if the former source disappeared or changed. Queue filters
and ordinary end-stopping arrow selection remain independent. Opening a source
does not answer, resume, stop or claim a provider; an already controlled view can
inspect its original session through the existing commands.

Keep one gesture pending and suppress held-key repetition. A token binds it to
its observational refresh. If it arrives during an older refresh, wait for one
new read started after the gesture. This is freshness sequencing, not a retry of
a failed read. Leaving the queue, selecting another row/filter, ordinary queue
navigation or opening the session palette cancels the pending navigation. Late
results cannot redirect the user. Check transient UI/export guards again before
applying the confirmed source. Failed reads remain errors; unavailable individual
transport endpoints are explicit and contribute no requests. Faulted Host
snapshots still require the existing source confirmation and show its refusal.

Use the existing input-composition checks for the active tab, Host, file preview,
workspace/import forms and session filters. Modal dialogs, sheets, popup menus
and search panels retain their keyboard handling. Consume the recognized global
shortcut before terminal forwarding. Focus the workbench when no focus exists at
startup. When a structured question replaces the composer, move only that hidden
composer focus to the visible Host; preserve an existing answer editor's focus.

No schema or wire protocol changes. Cursor/queue filters are window-local; drafts
retain their existing persistence. Request timestamps, configurable shortcuts,
OS notification delivery/deduplication and full mute/tray behavior remain open.
Linux fixture acceptance does not establish macOS, IME or real-provider approval
acceptance. See the separate native verification record.
