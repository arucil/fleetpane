# Observe local workspace preferences across windows

Date: 2026-10-23. Status: implemented; Linux fixture acceptance is recorded
separately in [verification](../verification/2026-10-23-workspace-sync.md).

## Context

Workspace/project names and pins were saved in the shared local Store but read
only when a window opened/refreshed the catalog or performed its own mutation.
Other windows could display an old label or pin list indefinitely. Paseo's
sidebar derives labels/pins from shared session-store observations (reference
`a486b0558168c08890890e787d55cfb88c0d7256`,
`packages/app/src/hooks/use-sidebar-pins.ts`). Fleetpane's preferences are local
controller data, so refreshing them should not query or restart a Session Host.

## Decision

Expose `Store::workspace_preferences(ids)` and CLI
`workspace STORE preferences UUIDS_JSON`. One read transaction returns up to 65
distinct saved workspace identities, with revisioned workspace/project labels,
plus the complete bounded 32-pin list in saved order. Reuse validated saved-row
decoding. Unknown/invalid/corrupt records fail the entire snapshot; do not inspect
directories, query Hosts, migrate sessions or scan all catalog pages.

Each desktop Workbench uses its existing two-second background cycle. Observe
open known tabs, the current catalog page and its inspected workspace. Allow one
read in flight; pause during local catalog actions/pin shortcuts and close.
Catalog and shortcut epochs invalidate an observation started before a local
operation, including a completed operation. Names merge only by matching saved
UUID/project UUID and nondecreasing revisions. An unchanged snapshot requests no
preference-driven repaint. This is eventual refresh, not a two-second deadline.

Preserve active editor text **and its original save revision**. The displayed
catalog/tab label can change while the editor remains based on an older value;
Save still detects that conflict. Only explicit Reload name discards the edit.
Background reads never clear failed-save errors or their close guards. After a
failed pin write, retain the observed pin order until Reload pins or Keep saved
pins explicitly resolves that state. A later read can then observe other changes.

Read failures retain previous values and show a bounded, wrapping error area
with Reload workspace updates. Stop automatic reads until explicit successful
reload; never replay a failed mutation. A read failure alone does not prevent
normal close. The existing write-specific close protection remains independent.

## Limits

No storage schema change. This observes existing labels and pins only: it does
not synchronize tab selection/order/layout, catalog registration membership,
recent directories, drafts, models or remote devices. It preserves existing
pagination and Host-specific ownership. Filesystem watching or a local event
service can replace polling later if measured work justifies it; none is needed
for the bounded local contract. No release performance claim is made.
