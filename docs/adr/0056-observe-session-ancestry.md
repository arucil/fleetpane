# 0056 — Observe provider session ancestry before lifecycle actions

Status: accepted for the local Codex prototype, 2026-10-21.

## Context

AG-01 / WS-05 and ADR 0055 still require safe archive mutation. Codex can shut
down spawned descendants when archiving a root. A loaded-context list, an idle
status or `thread/unsubscribe` acknowledgement does not establish that all
background resources have stopped. The installed Codex 0.163.0 generated schema
provides `sessionId`, `parentThreadId`, `forkedFromId` and `thread/loaded/list`;
there is no subtree RPC in that schema.

Paseo replay 175–178 concerns voice interruption, not ancestry. Its foreground
agent cancellation reinforces keeping UI selection separate from exact runtime
identity. That source inspection does not establish voice or lifecycle parity.

## Decision

Put observational collection and projection in `fleetpane-session::session_tree`.
The CLI and native view use the same implementation. The Host admits
`thread/loaded/list` on its existing query channel; no new mutation is added.
Inspection borrows the existing lease and never claims control, resumes, sends,
answers, interrupts, unsubscribes or archives.

Read loaded IDs, both normal and archived provider directories with all advertised
source-kind enum values, then metadata for loaded IDs absent from the directory
and the selected root if needed. Read loaded IDs again and refuse changed
membership. Check Host incarnation and owner again before adopting results.
This is a multi-request observation, not an atomic provider snapshot.

Use parent edges for descendants; forks are separate provenance. Parent-linked
nodes must agree on sessionId. Include archived and loaded ephemeral descendants.
Expose unresolved same-session parent chains, pending requests without a mapped
identity, other loaded contexts, observed turns and global in-flight/unknown
operations. Preserve exact identities in copied JSON and per-row ID copy.
Never infer archive membership for metadata found outside persistent directories.

Reject missing parent fields, hydrated turns, malformed/duplicate identities,
overlapping archive membership, cycles, inconsistent session IDs, repeated
cursors and bounded-inventory violations. Bounds are 100 records per page,
5,000 metadata/loaded identities, 8 MiB encoded metadata, depth 64, 100 continuation
cursors per traversal and a 30-second whole inspection deadline. Count encoded
metadata once per inserted record, avoiding repeated whole-inventory encoding.

The native inspector is an independent background job with a fixed Host/root.
It does not occupy conversation mutation state or disable Stop. It uses a bounded
virtual list within the existing independently scrollable interaction pane.
Refresh errors retain previous results with an explicit stale label; copied data
is the retained observation and includes its sequence range and limitations.
Changing the selected conversation or Host incarnation drops the inspector.
Every snapshot adoption path, including read-only observation and admission
readback, uses the same Host-incarnation invalidation.
The current snapshot and any successful result remain observations after adoption;
later provider changes require explicit refresh.

ACP does not advertise the ancestry/loaded-context contract and is refused
explicitly; its observed history is not converted into an invented tree.

## Limits

Background commands, process/resource closure, external mutations and archive
transaction fencing are not enumerated. Stable loaded membership and observed
idle status are not safe archive authorization. Active archive, subtree stop,
resource-complete receipts and live Session Host handoff remain separate work.
Validation: [session-tree evidence](../verification/2026-10-21-session-tree.md).
