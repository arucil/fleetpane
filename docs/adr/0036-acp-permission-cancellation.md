# ADR 0036: ACP cancellation settles permissions before the prompt

Status: implemented; local process and Linux native verification.

Paseo replay ordinal 75 adds per-agent pending permission promises and user
responses. Its cancellation path marks ready immediately and does not resolve
pending permissions. Fleetpane previously sent session/cancel but left permission
responses until prompt settlement. A provider waiting for those responses could
never settle; the new gated process fixture reproduces this against the old Host.

Follow the [ACP cancellation contract](https://agentclientprotocol.com/protocol/v1/cancellation):
sending session/cancel also answers every pending permission for that session with
outcome cancelled. Do not wait for another provider cancellation notification.
The Host removes those actionable requests via existing serverRequest/resolved
events after the wire batch is written. Requests for other sessions are unchanged.
Late permissions during cancellation receive cancelled directly and never enter
the actionable request list. Validation and frame/count bounds still apply.

Store an ephemeral cancellation-request flag on the owned session. Repeated Stop
for the same active turn acknowledges intent without sending another notification
or permission response. Stale responses are rejected by the Host pending-request
check. Serialized provider/command handling determines whether an approval or
Stop wins; no cancellation can retract a response already written to the provider.
Partial write outcomes retain the existing provider fault fence and never retry.
Storage fencing also remains in force before wire publication.

Keep the prompt active until its actual provider response. Continue processing
cleanup updates; expose active/cancelRequested in reads and status notifications.
The common Rust projection and native UI show the waiting state, disable duplicate
Stop and preserve the draft. A replacement window reads the same flag without
reissuing cancellation. Missing flags in older/Codex status frames decode false.

The flag is not archived. A Host restart during cancellation marks the unfinished
turn unknown and read-only, with no permission/prompt/cancel replay. A confirmed
cancelled stopReason survives normally. This does not transfer provider processes
across Host upgrades, force-kill unresponsive agents or invent a timeout completion.

[Verification](../verification/2026-10-21-acp-cancel.md).
