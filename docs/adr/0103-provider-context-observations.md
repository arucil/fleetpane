# Provider context observations

Accepted 2026-10-22. Implementation and synthetic process verification are separate
from parent-owned native acceptance and installed-provider/model acceptance.

Paseo history #2180, #2191 and #2415 exposed a context visibility gap. Fleetpane's
ACP adapter discarded `usage_update`, and its thread projection ignored Codex
`thread/tokenUsage/updated`. Context occupancy must use actual telemetry rather
than model-name lookup tables or cumulative session totals.

The authority for Codex is the locally generated app-server schema from installed
`codex-cli 0.163.0`: `ThreadTokenUsageUpdatedNotification` requires `threadId`,
`turnId`, `tokenUsage`; usage requires both `last` and `total`. Each breakdown
requires `cachedInputTokens`, `inputTokens`, `outputTokens`,
`reasoningOutputTokens`, `totalTokens`; `cacheWriteInputTokens` has a schema
default and may be absent. Fleetpane validates the present integer fields and
uses **`last.totalTokens`**, with nullable/absent `modelContextWindow` as capacity.
This agrees with inspected Paseo `toAgentUsage` field selection, while retaining
zero rather than treating it as absent. Negative token counts or values outside
nonnegative int64 fail the known-payload boundary.

The installed ACP SDK schema defines unstable `UsageUpdate.used` as current
context occupancy and `size` as capacity (required uint64), with optional/null
cost and opaque `_meta`. Prompt response `Usage.totalTokens` describes session
totals and never replaces this observation. Unknown extensions and the complete
original notification remain in `raw`, including Codex cumulative breakdowns and
ACP cost/metadata. This is exact parsed JSON telemetry, not original whitespace,
key ordering or byte-for-byte JSONL framing.

A toolkit-independent runtime `Observation` contains thread identity, optional
observed-turn association, source, **Host receipt Unix milliseconds**, used
count, nullable capacity and raw telemetry. ACP has no provider turn ID; any
association is the Host's active prompt at receipt, not proof of prompt success.
Known payloads validate atomically before publication. Normalized
`thread/contextUsage/updated` notifications carry the complete snapshot; readers
revalidate projections against raw telemetry to reject stale-field mixtures.
No success, readiness, compaction, or remaining-token claim is inferred.

The Host retains latest observations by thread with 64-thread / 1 MiB FIFO
replacement bounds, and a 64 KiB limit on an individual complete observation.
Evicted/unobserved threads read as unavailable. `fleetpane/contextUsage/read` is
an additive controller-fenced read, like the existing tasks read. It returns the
current Host sequence even for an unavailable observation, without provider I/O,
new operation receipts, ownership mutation or events. GUI reads reject mismatched
thread/sequence and cannot regress newer event state. Turn start does not clear
usage. Reload, GUI replacement and controller takeover query this cache even
when the original event ring has expired. Host protocol version remains 3; no
existing request, response or operation schema is changed.

ACP observations also reside in the existing bounded session archive, committed
before event publication. Host restart validates and seeds available archive
snapshots, retaining their **original receipt time**. Resume does not refresh the
measurement until actual telemetry arrives. Older archives without observations
remain readable. Corrupt observations refuse restoration. The archive contains
only activity observed by this Host, not complete provider history. Codex context
observations are in memory only: Host restart needs new provider telemetry;
Fleetpane does not read private rollout files or manufacture observations.

The native strip shows exact used/capacity counts, source and last observed Host
receipt timestamp. Missing observation/capacity are explicitly unavailable. A
zero capacity is displayed but has no percentage; percentages are approximate,
never clamped, including provider overcapacity. `Copy usage JSON` explicitly
copies the complete observation envelope including thread/receipt identity and
untouched parsed `raw` payload. Copy never claims a lease or calls a provider.
Unix milliseconds are rendered directly without timestamp arithmetic. The strip
is inside the bounded scrollable interaction region; composer and Send/Stop keep
the existing fixed area. Parent native QA owns 900×560 reachability and clipboard
acceptance with pending choices.

Synthetic regression tests cover required/missing/null/zero/overflow fields,
capacity replacement, unknown extension retention, other threads, sequence
boundaries, bounded eviction, interturn retention, no inferred turn completion,
archive restoration/resume and invalid known payload faults. The private process
smoke uses the real Host transport and a dedicated synthetic provider; no model
calls or live sessions. Receipts and command logs are in
`/tmp/fp-context-usage`. Native and installed-provider runtime claims require
separate acceptance evidence.

Covered Linux native fixture acceptance, exact Copy, minimum-window controls and
independent pending-state snapshots are now recorded in
[verification](../verification/2026-10-22-context-usage.md). Installed-provider
context runtime acceptance remains separate.
