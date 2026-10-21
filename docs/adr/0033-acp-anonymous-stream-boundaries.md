# ADR 0033: Preserve ACP message identity and anonymous stream boundaries

Accepted 2026-10-21.

Paseo root-history commit 53 revisits activity chronology. Deterministic replay
exposed a Fleetpane defect: every unidentified message used the literal
`stream` identity. Text arriving after a tool or thought patched an earlier
message, and a provider's real messageId `stream` collided with that fallback.
Two regression tests failed before the change; the old Host binary also failed
the process reproduction.

ACP allows optional opaque [message IDs](https://agentclientprotocol.com/protocol/v1/prompt-turn#message-ids).
Keep chunks with an explicit ID in one identity-addressed message, even when
interleaved with tool updates. First observation determines its timeline position;
later chunks patch that item. Separate generated IDs into explicit-ID and
anonymous-segment namespaces, scoped to the turn and role.

Without an explicit ID, append only to the immediately preceding anonymous
stream of the same role. A tool call/update, role change, identified message,
plan or other retained activity ends that segment. Configuration/usage-only
notifications do not add timeline content. Tool updates continue to patch their
original call rather than creating duplicate cards. Preserve original text,
including whitespace, instead of trimming or regrouping it for presentation.

Keep the current anonymous segment pointer on the owning Session, reset it on
each new prompt, and omit it from persistence. Restored turns are archived and
cannot receive chunks. New segments use the normal item notification, paging,
bounded adapter and journal paths; no separate desktop ordering algorithm or
archive schema migration is needed. Persisted IDs remain stable on reload and
restart. Existing already-merged history cannot be reconstructed from its lost
boundaries and is not rewritten.

This does not add a textual activity MCP API, transcript import, provider modes,
or plan-history persistence. More accurate segmentation can reach the existing
500-entry/2-MiB observed-session bound sooner; exceeding it remains an explicit
error, never silent eviction. Generic unknown updates and autonomous provider
turns retain their existing compatibility limits.
