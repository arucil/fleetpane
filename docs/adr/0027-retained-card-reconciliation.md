# 0027: Reconcile retained conversation cards by identity

Accepted 2026-10-20.

The [Paseo history review](../reference/paseo-history.md) identifies repeated
work-amplification fixes during streaming. Fleetpane has its own concrete hot
path in `HostView::rebuild_rows`: retain each card by scanning all new rows, then
clone every row and call every retained entity even when its content is unchanged.
`ItemView::set_row` already avoids rebuilding unchanged Markdown/tool blocks, but
that check happens after the row has been cloned and the entity borrowed.

Use ordered identity indexes for old rows and the new retained ID set. Retain
the actual card cache against the new ID set, including after session switches
where the old row vector has already been cleared. Compute changed/new indices
by complete row equality (label, content, exact copy text and visible error),
then clone/update only those rows. A reorder reuses card entities. Removal drops
their state. A new session's qualified row IDs prevent reuse across sessions.
Keep the existing prefix/suffix timeline splice and scroll behavior.

Membership and update selection are O(n log n) ordered lookups, replacing the
prior O(n²) membership scan. This is an algorithmic statement, not a measured
UI speedup. Full source-to-row construction and equality of retained content
still run per update; no hidden Host polling, cursor, approval, lease or provider
behavior changes. Broader incremental projection and W1 latency measurement
remain separate work. No upstream code is copied.

Verification covers a changing tail across 1,200 other cards, no-change updates,
insert/reorder/remove, all visible row fields and session-cache replacement.
Native acceptance covers expanded tool details and exact clipboard text after
another turn, session switch and history reload; see
[evidence](../verification/2026-10-20-paseo-history.md).
