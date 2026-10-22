# Workspace tabs and local layout

Accepted 2026-10-22. Paseo history around 1385–1430 and 3801–5000 motivates
independent workspace views with explicit source identity. Fleetpane keeps this
presentation state outside the provider process and its Session Host.

A tab is identified by the saved workspace UUID, exact Host socket and registered
root path/device/inode. Display names are labels. The catalog groups the loaded
page by project; it still retains its existing pagination and pin limits.
Opening a registered workspace creates or selects one of at most 32 tabs. Each
retains its own session, draft-backed conversation, Files view and Terminal view.
Switching away stops history/PTY polling for hidden panels; their Host snapshots
remain observable. Entities are initialized on first selection, retained while
open, and released on tab closure. Host leases are shared by views of that exact
endpoint within one window. An explicit claim therefore controls the Host once;
external takeover still fences every mutation at the Host.

Controller store schema 14 adds one revisioned layout record, bounded to 512 KiB.
It contains ordered tabs and the selected tab, panel, exact saved session
Host/session/cwd/archive identity, relative file location, selected terminal
Host/terminal UUID pair, and sidebar width (240–520 px)/visibility. It does not
store provider contexts, full transcripts, terminal input, widgets or secrets.
Session drafts continue to use their independent revisioned identity-bound keys.
Migration retains earlier tables; older binaries refuse the newer store.

All storage, catalog identity checks and source observations run off the render
thread. Saves coalesce behind one in-flight write. SQLite immediate transactions
compare revisions and retain existing bytes on conflicts, corruption or I/O
failure. The UI exposes failures; a normal window-manager close waits for writes
and refuses to silently discard errors. Reload adopts the stored layout only
after local drafts/input settle and uncertain operations have been checked.
Keep saved layout explicitly pauses this window's layout writes and allows a
normal close while retaining the previous persisted record.

Startup reads the layout, checks registered identities and lazily opens the
selected views. Session restoration observes directory membership and reads
history with resume=false through the allowlisted query channel. It never claims
control, starts a provider context, restores an archived context, sends input or
creates a terminal. Taking control in a workspace tab reloads its retained
history without implicitly resuming an unloaded context; Resume remains a
separate action. A saved terminal is selected only when the Host incarnation and
workspace-scoped inventory both match. Missing/replaced sources show errors and
are not rebound using display names. A Host without an established controller
cannot supply these lease-bound queries until control is explicitly acquired.

Closing a tab removes the presentation only. Accepted work and shells belong to
the Host. The tab stays open while local saves, accepted keyboard input or
unresolved operations require attention. Reopening the catalog entry restores
session drafts but does not promise the just-closed tab's panel/location.

Conversation scroll, split panes, drag ordering, tab keyboard navigation,
cross-window live synchronization, remote Hosts and terminal text-draft
persistence across tab closure remain separate work. This design makes no
release performance or replacement-readiness claim. [Evidence](../verification/2026-10-22-workspace-tabs.md).
