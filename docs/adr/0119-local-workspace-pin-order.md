# Device-local workspace pin order

2026-10-23. Accepted.

Paseo at `a486b0558168c08890890e787d55cfb88c0d7256` applies a stored workspace
order to its newest-first pins (`packages/app/src/hooks/use-sidebar-pins.ts`
and its test). A newly pinned workspace stays first. Fleetpane adopts that
observable behavior using existing UUID-bound local pins, without copying code
or moving provider sessions, workspace catalog rows or open tabs.

`Store::reorder_workspace_pins(expected, order)` accepts two bounded UUID lists.
The candidate must be an exact permutation of the observed list, with each UUID
once and at most 32 pins. One immediate SQLite transaction reads the current
list, checks exact order and membership, then replaces its pin rows in reverse
insertion order. A failed delete/insert/read/commit leaves the old order intact.
An unchanged candidate performs no writes. Existing schema 18 needs no migration;
pin positions remain internal ordering metadata, separate from catalog positions.

This is comparison of current values, not a revision counter: an intervening
change that restores exactly the same list permits the action. Changes to names
or catalog metadata also permit it. A different order or membership requires an
explicit reload; no automatic reorder retry can overwrite another window.
Explicit pin assignments remain idempotent; new pins and unpin/re-pin go first.

The Workspaces catalog provides a dedicated drag handle and Up/Down buttons for
each pin. The drag captures the observed UUID list and view owner; a drop on
another row moves the source to that row's position. Same-row, cancelled and
foreign-view drops perform no save. Buttons are an alternative for long lists;
drag-edge autoscroll is not part of this change. No filesystem inspection, Host
control or RPC runs during ordering or rendering.

GUI actions display committed lists rather than optimistic order. Saves are
serialized within the catalog. Failures show the boundary error, disable further
pin edits and block normal close. **Reload pins** reads the latest saved list
without replaying the failed move; **Keep saved pins** acknowledges failure
without writing or refreshing. A stale retained display still requires reload
before a subsequent reorder can succeed. Close waits for in-flight pin actions.
Other windows refresh explicitly; automatic synchronization is separate.

GUI and CLI share the same command. CLI:
`fleetpane-host workspace STORE reorder-pins EXPECTED_UUIDS_JSON ORDER_UUIDS_JSON`.
Each JSON argument is limited to 4096 bytes before parsing. Lists contain public
workspace UUIDs, not credentials or provider data.

[Acceptance](../verification/2026-10-23-workspace-pin-order.md) records separate
engine, CLI and Linux fixture results. Installed-model behavior, other-platform
GUI, cross-window drag and release performance are outside this acceptance.
