# 0052 — Distinct conversation row identities

Status: accepted for the native conversation cache, 2026-10-21.

Paseo history 157–161 revisits tool hydration, late tool IDs and permission keys.
Its property/FIFO matching and case-folded call IDs are not Fleetpane's provider
identity contract. Reviewing Fleetpane's row cache exposed a separate concrete
collision: turn status used `thread:turn:status`, while messages used
`thread:turn:item`. An item named `status` replaced the status entity. Two distinct
turn/item pairs `a:b` / `c` and `a` / `b:c` also shared one entity. Pagination could
update one card while another row continued referencing that same entity.

## Decision

Build conversation rows through one pure `Row::conversation` entry point used by
the native Host view and regressions. Row constructors receive separate thread,
turn and item identities. Serialize tagged string tuples as internal keys:
`["turn", thread, turn]` and `["item", thread, turn, item]`. JSON escaping preserves
component boundaries, case and Unicode; no delimiter guessing or hash is used.
The same key drives retained entities, file-action subscriptions and GPUI element
identity. Host views already own separate caches, so endpoint identity remains
at that enclosing boundary. Permission request identity remains independent.

Provider item IDs, Host messages, journals and persisted operation identities do
not change. There is no migration. Repeated updates to the same exact tuple still
update one retained card; another tuple cannot reuse that card. This does not
infer missing call IDs, match tools by name/input, normalize case or merge
ambiguous provider history. Conflicting duplicate IDs within the same provider
turn are outside this collision fix.

## Verification

See [before/after native acceptance](../verification/2026-10-21-row-identity.md).
Two production-row-builder regressions fail before the fix and pass afterward.
Existing metadata and 1,200-retained-card reconciliation tests also pass. A native
synthetic paginated history reproduces wrong cards in the old binary and correct
status/message/command cards in the new binary, including independent expansion
and exact Copy after reload. The same Host/provider remains running between
windows. No broader performance or real-provider acceptance follows from this.
