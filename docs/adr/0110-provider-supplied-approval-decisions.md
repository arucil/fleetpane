# 0110 — Respect provider-supplied approval decisions

Accepted, 2026-10-22. Extends ADR 0109 and the original provider request UI.

Installed Codex 0.163.0 offered a command approval with `accept`, a structured
execpolicy amendment and `cancel`. The fixed Allow once/Decline UI presented an
unoffered decision. The Host accepted any response and immediately forgot the
request after writing it to provider stdin. A successful transport write did not
prove the response matched the provider's choices.

Parse Codex's optional `availableDecisions` in the runtime crate so the same
contract serves the desktop, shared client and Host. Preserve offered order for
the supported string decisions: `accept`, `acceptForSession`, `decline`, and
`cancel`. Distinguish declining while continuing the turn from cancelling and
stopping it. Explain the session-scoped grant when offered. Missing/null lists
preserve the previous one-shot accept/decline contract; an explicit empty list
offers no response. Malformed/duplicate known entries and lists above 16 entries
are errors, never permission defaults.

Policy amendments and future string/object variants remain in the inspectable
original request. Show their unsupported count and offer no automatic substitution
or persistent policy mutation. The typed response supports the four string
decisions only. Unknown-only requests remain pending and can be stopped through
the existing Stop command. ACP keeps its exact advertised option IDs and adapter
validation; no Codex decision mapping applies to ACP.
Require strings during decision decoding: Serde's default unit-enum object
representation is not part of the provider contract.

Validate before the shared client sends a response, and again at the serialized
Host boundary against the actual pending request. An invalid Codex response
returns `invalid_approval`, writes no provider frame, advances no event/journal
state and retains the request. Existing lease fencing, unknown-request errors
and unknown transport outcome rules remain unchanged. No retries, new wire fields
or persistent schema changes are needed.

Pin Codex/ACP response buttons beside the input area, outside the scrolling
review content. Place pending approval details/questions before optional context,
tasks and settings; let pending review occupy up to 75% of the remaining
conversation area. Retain the composer draft and explicit Stop. Large reviews
and question forms still scroll; do not infer acceptance merely from visibility
of a button. Structured questions retain their existing explicit submission.

Installed-provider/model and synthetic process acceptance are recorded
[separately](../verification/2026-10-22-real-provider-requests.md). This does not
establish persistent policy support, every provider/version, IME, other platforms
or complete notification/mute behavior.
