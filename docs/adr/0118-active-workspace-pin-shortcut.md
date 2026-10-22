# Active workspace pin shortcut

2026-10-22. Accepted.

Paseo's pin-shortcut tests at `a486b0558168c08890890e787d55cfb88c0d7256`
cover one dispatch per press even when the selected workspace's sidebar row is
unmounted, plus recovery after a rejected write. Fleetpane already has local,
UUID-bound pins and independent mounted workspace tabs. Use their selected UUID
as the shortcut identity; no directory scan, Host ownership or provider command
is required. The reference supplies behavior, not copied implementation/assets.

Ctrl+Shift+P (Cmd+Shift+P on macOS) and the Tabs menu invoke the same atomic
`Store::toggle_workspace_pinned` command exposed as CLI `workspace ... toggle-pin
UUID`. Its immediate SQLite transaction reads and changes the current saved pin,
so two windows requesting a transition each execute one transition. Explicit
`pin UUID on|off` remains idempotent. Both paths share capacity, identity,
rollback and ordering rules. No schema change is required.

Capture shortcuts at the Workbench, independently of catalog rendering. Preserve
existing IME, dialog, popup, search, switcher, alert and close/export guards. Consume
the accepted chord before it reaches a composer or PTY. Serialize this window's
pending pin command and show its committed outcome with the captured Host/label.
Update the cached pin list; other windows require explicit refresh.

Linux native QA reproduced repeated toggles during one physical key hold:
`gpui-pre-linux` 0.3.9's X11 backend suppresses synthetic repeat releases but emits
every down with `is_held: false`. Track P press/release in the Workbench in
addition to the platform repeat flag, including presses owned by other contexts.
Clear the held state on window activation changes, since a release can occur
while another window owns input. This is an action edge, not a timed debounce.

Failed saves retain the previous pins and show the actual storage error. Normal
window close waits for in-flight writes and refuses an unresolved failed save.
An explicit next shortcut/menu action can retry; **Keep saved pins** acknowledges
the failure without another write and permits close. Never retry a toggle
automatically: a second invocation is another state transition.

This adds local pin actions, not server pin synchronization, pin reordering,
project renaming or configurable shortcuts. Native acceptance and platform limits
are recorded in [verification](../verification/2026-10-22-workspace-pin-shortcut.md).
