# 0005: Host-backed conversation basics

Status: implemented incrementally, 2026-10-19. Extends ADRs 0002–0004; does not
change the accepted Host generation/drain design or establish replacement readiness.

## Decision

Add `fleetpane-session`, independent of GPUI, for the Codex conversation model
and typed commands. The desktop and `fleetpane-host session` CLI share this
adapter: list/create/open, send a text turn, interrupt a turn, inspect uncertain
operation results, and answer command/file approval requests. Provider-specific
commands and event projection live in the shared adapter. Kit Input/Textarea own text editing;
the variable-height timeline uses the engine's virtualized `ListState` with
stable thread/turn/item identities. The existing read-only Paseo directory mode
continues to use its separate WebSocket projection.

The GUI connects to an existing private Unix Host socket. It never starts or
stops the Host or provider. A vacant Host can be claimed once; an occupied Host
needs an explicit Take control action. Every mutation carries the lease, and
replaced controllers cannot send or answer approvals with their old lease.

Use the Host operation UUID before submitting any provider mutation. A lost reply
retains that UUID, blocks further commands and offers a status query; never
automatically repeat a prompt. Unknown outcomes include the provider-fault reason.
The in-memory operation journal still has a 128-operation lifetime cap; exhausting
it is an explicit error. [ADR 0012](0012-observational-host-queries.md) moves
observational reads to a separate bounded channel. There is no durable
uncertain-operation recovery across desktop/Host crashes yet.

Host operations expose an optional `completed_sequence`, the event sequence
of the provider response; observational queries return `sequence` directly.
A successful history read establishes the selected
projection and its replay cursor at that boundary. A turn/start response does
**not** advance the cursor: notifications can precede its reply. Stale background
polls are discarded using a local revision; an unsuccessful reconnect reload
cannot advance the old cursor. An expired cursor or malformed known event stops
incremental replay and requires an explicit history reload. This ordered response
boundary is transport ordering, not an upstream atomic snapshot guarantee;
active assistant-token streaming reconnect still needs its own acceptance.
Subsequent [live OpenCodex checks](../verification/2026-10-19-opencodex-live.md)
cover client replacement during a real running command and its completed reply.

Upsert completed items by identity and retain earlier streamed items when the
turn completion contains an empty items list. Preserve unknown tool item fields
as inspectable JSON, and keep unknown request types visible without answering
them. Command/file approvals expose explicit accept/decline only. A provider
`serverRequest/resolved` notification clears its matching pending request without
inventing an approval decision; a conflicting thread ID is a protocol error.

## History compatibility and limits

The history-mode choice in this ADR is superseded by
[ADR 0010](0010-durable-codex-tool-history.md): new threads now use **paginated**
provider storage. Existing legacy threads keep their mode and show a warning.

The original slice selected legacy because a new empty paginated thread returned
`list_turns is not supported yet`. That was an invalid capability inference:
Codex 0.163.0 materializes history after the first user turn, including a failed
turn. The same paginated thread then supports full reads and turn listing.
The [saved-history rendering check](../verification/2026-10-19-real-workspace-rendering.md)
had already read a populated paginated thread using copied indexed databases.

The later [live OpenCodex check](../verification/2026-10-19-opencodex-live.md)
showed that legacy storage omits nested code-mode command items from subsequent
reads. Paginated storage preserves them; the
[durability verification](../verification/2026-10-19-durable-tool-history.md)
checks exact live/read/resume item equality after a completed Host/provider
restart. No bounded replay buffer is merged into history, and old discarded items
are not reconstructed from outer tool text.

Before the first user turn, the create response can be displayed in the current
window, but a reload can return a provider error and the persisted directory can
omit the empty thread. The real Codex conversation check verifies that a failed
first turn materializes paginated history.

The Host directory shows the first 100 threads with an explicit continuation
notice. Only the selected thread keeps hydrated turns; directory entries retain
metadata. Conversation previews are capped at 500 items / 2 MiB; the existing
256 KiB provider-frame limit can reject smaller full-history reads. Individual
rendered text sections are visibly limited to 16 KiB. ADR 0006 subsequently
preserves the full item for Copy within the same transport bounds. Approvals show
one pending request at a time, its total pending count, original thread identity
and a scrollable detail area. Drafts live only in the current desktop process.
History pagination is subsequently added for paginated threads in
[ADR 0011](0011-incremental-conversation-history.md). Complete item export and
persistent drafts remain future work.

## Evidence and remaining work

See the [conversation verification record](../verification/2026-10-19-conversations.md)
and [real saved-history rendering checks](../verification/2026-10-19-real-workspace-rendering.md).
Synthetic streaming/stop/approval workflows, real Codex with a local rejecting
HTTP fixture, rendered Linux windows and the later live OpenCodex model/tool
turns are separate evidence levels. Real tool approval, IME composition,
macOS/Windows desktop behavior, remote CI and release performance improvement
remain unaccepted. Markdown, tool cards,
diffs and image previews were subsequently added in
[ADR 0006](0006-rich-conversation-items.md). Attachment upload/send,
terminal emulation, remote connectivity, control-daemon service
lifecycle, Host restart recovery and version drain are still pending.

Structured `requestUserInput` support and request routing are subsequently added
in [ADR 0007](0007-structured-questions.md).

Host directory page loading, provider filters and naming are subsequently added
in [ADR 0008](0008-session-directory.md).
