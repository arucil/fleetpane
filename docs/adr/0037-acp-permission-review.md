# ADR 0037: Review the requested operation before choosing permission

Status: implemented; local verification recorded separately.

Paseo replay ordinal 76 presents permission.toolCall in per-agent cards, including
operations without an earlier tool event. Fleetpane instead preferred a history
lookup and only used the request title as a fallback. A private native reproduction
showed Old operation/old-command while the actual request proposed new settings.

Decode the ACP request's ToolCallUpdate in the common Rust session crate. Preview
the supplied non-null fields first; fill missing fields only from a matching tool
in the selected session's active turn. Replace raw JSON and collections in full,
including false, zero and empty arrays. Do not rewrite observed tool items. Without
an observed tool, use presentation defaults for title/status and show the supplied
details. Those defaults are not evidence that execution started or succeeded.
Validate field types, route/session identity, content and absolute locations before
enabling native choices. Unknown request fields remain in the complete raw copy.

Reuse the retained ItemView renderer for a distinct, initially expanded permission
preview. Its cache identity includes Host incarnation and request ID. Build/update
the projection on snapshot or conversation changes, not on every render. Incremental
changes retain card expansion; answered requests drop their preview. Copy contains
the original request rather than a reconstructed tool snapshot. Input/content are
source previews; no vendor-specific plan/file/command keys imply execution rules.

Constrain the interaction region and its inner detail viewport relative to window
height. Scrollable content must keep its intrinsic height rather than flex-shrink
away; this exposes all review details and options while leaving the draft and
Stop/Send controls accessible in the default small window. Existing conversation
tool cards keep their prior detail sizing. The region is bounded only when an ACP
permission preview is present.

Pending requests stay Host-owned until an answer is written or cancellation resolves
them. Replacing a window or switching sessions must recover the exact pending
request; no automatic choice, local optimistic removal or history replay is added.
Existing lease fencing rejects stale controllers and resolved request IDs. Offered
choice IDs and labels remain exact; permission kinds do not grant extra capability.

This is transient request review, not durable permission audit history. It does not
implement Git Diff expansion, navigate affected files, run proposed operations,
add an approval policy or claim real-model acceptance.

References: [ACP tool calls and permissions](https://agentclientprotocol.com/protocol/v1/tool-calls),
[verification](../verification/2026-10-21-acp-review.md).
