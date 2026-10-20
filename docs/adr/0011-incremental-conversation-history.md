# Incremental conversation history

Accepted 2026-10-19 for Codex paginated threads. Extends
[ADR 0010](0010-durable-codex-tool-history.md).

## Decision

The desktop opens paginated history with a metadata-only `thread/read`, or
`thread/resume` with `excludeTurns: true` for an unloaded thread. It then requests
the newest turn with `thread/turns/list`, limit 1, descending order and
`itemsView: "notLoaded"`. `thread/items/list` loads up to 16 entries from that turn
in descending order; the projection reverses them for chronological display.

Load earlier messages continues within that turn, then requests the preceding
turn's metadata and item page. The button shows the retained entry count and
disables at the beginning or the retained-history bound. Reload starts again at
the latest turn. This also bounds the first read of a single turn with many tools.

`fleetpane-session::history` owns continuation validation, ordering, bounds and
event reconciliation. CLI `inspect`, `history-turns` and `history-items` and the
native desktop use the same typed commands. Cursors stay opaque; production code
does not decode their internal JSON. Existing legacy/unspecified history modes
retain the previous bounded full-read path. The explicit CLI `open` command also
continues to request full history for inspection.

## Reconciliation during reads

Hydration uses several responses, so their sequence boundaries differ. Advancing
the event cursor to the last response could discard a new turn started between
the first metadata read and the last item read. Replaying all events onto the
hydrated items could duplicate text already present in the item snapshot.

Keep the cursor at the metadata response boundary. Track the turn metadata
boundary, the first item page's tail boundary, and each older hydrated item's
boundary. Replay later events while skipping fields already covered by those
snapshots. The first item page defines the visible tail: earlier item events
inside that read window remain behind the older-history continuation. They must
not be appended after the latest entries. Later pages suppress overlapping
updates only for their own items; they cannot suppress a recent item's unseen
delta. New turns continue to receive their events.

All page validation happens on candidate state before replacing visible data or
continuations. Reject wrong-turn entries, duplicates, repeated/oversize cursors,
oversize pages and empty continuing pages. A failed page keeps the current
history and continuation usable. Exceeding the retained-history bound disables
further older loads and leaves Reload available. Events retain their original
ordering and UUID-based operation reconciliation remains unchanged.

## Native scroll behavior

Preserve the common prefix and suffix when splicing rendered rows so prepending
older items preserves existing row measurements and scroll anchors. Use GPUI's
tail-follow mode for streaming. Its geometric `is_scrolled_to_end` is unknown
while any virtual rows have unmeasured heights; treating unknown as permission to
follow caused a verified jump to the bottom after a page load. Tail mode tracks
actual user scrolling and resumes following when the user returns to the end.

## Limits and evidence

Retained state is still bounded to 500 items, 500 turns and 2 MiB. There is no
eviction or bidirectional history window; Reload returns to recent entries.
Sixteen entries is a count bound, not a byte guarantee: a large item or page can
still exceed the provider's 256 KiB frame limit. A byte-aware provider/transport
contract remains necessary for that case. The Host retains its 128-operation
lifetime journal cap for `execute`; [ADR 0012](0012-observational-host-queries.md)
moves directory/model/history reads to a separate bounded channel. This slice
does not establish performance gains.

The [verification record](../verification/2026-10-19-history-pagination.md)
covers a 1,200-command synthetic history, native anchor preservation, retained
limits, delayed history reads during streaming, and paged recovery of a copied
real OpenCodex conversation. Deterministic installed-Codex tests use local SSE;
the copied real conversation introduces no additional external model turn.
