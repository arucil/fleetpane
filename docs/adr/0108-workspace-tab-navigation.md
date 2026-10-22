# Workspace tab navigation and order

Accepted 2026-10-22. Extends [workspace layout](0107-workspace-tabs-and-local-layout.md)
with presentation commands for the existing ordered workspace UUIDs. No Host or
provider mutation is added. This follows the workspace-tab workflows already
reviewed in Paseo history; no source or assets are copied.

The primary modifier is Ctrl on Linux and Cmd on macOS. Primary+PageUp/PageDown
selects the previous/next tab, wrapping at the ends. Primary+1–8 selects that
position when present; Primary+9 selects the last tab. Primary+Shift+PageUp/PageDown
moves the selected tab one position, stopping at the edges. Linux Ctrl+Shift+W
and macOS Cmd+W close the selected view through the existing draft/input/operation
guards. Ctrl+W remains available to Linux text inputs and shells. Held events
are consumed without repeating selection, moves or closure.

A fixed **Tabs** menu exposes selection, move and close commands beside the
horizontally scrolling strip. Selection/moves use the same UUID-bound entities;
they never reconstruct the source using tab position or label. Reordering keeps
the current input focus and uses schema 14's existing revisioned layout write.
Selecting a hidden tab scrolls the strip just enough to show it. A Workbench
focus handle keeps navigation available when a tab has no editable content.

The Workbench captures recognized chords before terminal key encoding. Once a
workspace command is recognized, refusal or an in-flight export cannot forward
it into the PTY. Plain editor/terminal keys keep their existing path. Session
switcher, provider-choice palette, Kit popup menus, source Find, dialogs and
sheets retain their own focused interaction. Character-preferred events and
marked text in focused conversation, question, file and terminal editors are
excluded. This implements guards; actual IME acceptance remains open.

Closing a view keeps accepted provider tasks and shells owned by the Host.
Failed draft saves refuse closure. Failed layout writes retain the previous
stored layout and expose its existing reload/keep-saved controls; removal from
the live tab strip alone does not assert a durable successful save.

Drag ordering now has [separate decision and acceptance](0112-workspace-tab-drag-order.md).
Splits, scroll restoration, configurable bindings, terminal
text-draft persistence after tab closure and cross-window synchronization remain
separate work. Only Linux X11 has native acceptance for this addition.
[Evidence](../verification/2026-10-22-workspace-tab-navigation.md).
