# Workspace tab drag order

Accepted 2026-10-22. Extend the existing bounded local workspace layout and
[navigation](0108-workspace-tab-navigation.md) with native pointer reordering.
The inspected Paseo reference is `a486b0558168c08890890e787d55cfb88c0d7256`,
`packages/app/src/screens/workspace/workspace-desktop-tabs-row.tsx`: tab IDs and
pane ownership determine a move, and close buttons do not start a tab drag.
Use GPUI's own drag API; no Paseo code, drag dependency or assets are copied.

Each payload carries the originating Workbench entity and exact workspace UUID.
Resolve source and target against current entries at drop time. Moving to a tab
inserts the source at that tab's previous position, preserving intervening order.
A same-source, missing/stale identity, foreign Workbench or outside drop does
nothing. Session switcher, window closure and pending export retain their guards.
The label starts a drag with a native preview and target highlight; its close
button remains a separate click target. Ordinary clicks keep existing selection.

Move the whole workspace entry, including its conversation, file and terminal
entities and subscriptions. Keep the selected UUID; dragging an inactive tab
does not select it. Capture through the existing revisioned layout save and
scroll the dropped tab into view. This adds no Host lease, provider request,
terminal input or database schema. Failed saves keep the old persisted layout
and existing reload/keep-saved/normal-close guards. Successful reload also clears
the close refusal belonging to that resolved layout error, preserving unrelated
close errors.

This covers reordering visible workspace tabs within one window. Edge drag
autoscroll, cross-window movement/detach, split panes, conversation scroll
restoration and actual macOS/Windows pointer acceptance remain separate work.
[Linux native evidence](../verification/2026-10-22-workspace-tab-drag.md).
