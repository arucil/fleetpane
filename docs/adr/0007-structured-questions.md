# Structured Codex questions

Accepted 2026-10-19 for the initial local Host slice.

## Problem

`item/tool/requestUserInput` previously entered the generic approval card. Its
JSON was inspectable, but both decision buttons were disabled. The shared client
only knew command/file approval responses, so the agent could not receive an
answer. The card also selected the first global request, even when another
session was open.

## Decision

Keep request decoding, response validation and Codex answer semantics in
`fleetpane-session::questions`. The installed Codex 0.163.0 experimental JSON
schema is the wire contract; Codex's TUI is the reference for single selection,
`None of the above` when `isOther` is set, and `user_note: ` for optional notes
and free text. Do not infer multiple selection from the response's string array.

The native GPUI Kit form uses controlled RadioGroup, Textarea and masked Input
states. There is no initial selection. Previous/Next preserves each question's
answer; submission requires every question to be answered. Users can stop the
turn through its existing command. There is no implicit skip or timed answer.
`isBlocking` affects the explanatory label; deprecated `autoResolutionMs` does
not trigger a client timer. Responses have a 64 KiB encoded limit, checked before
the transport boundary. Invalid or unsupported requests remain inspectable.

GUI and CLI both call `Connection::answer_questions`, which validates the same
typed response and writes through the Host's fenced Respond action. The CLI reads
answers from stdin, prints acknowledgment only, and requires explicit takeover
when the Host has an owner. Example:

```sh
fleetpane-host session SOCKET --take-control answer '"REQUEST_ID"' < answers.json
```

Pending requests are associated with their provider thread ID. A selected session
shows its first request; requests in other sessions have an Open session action.
Requests without a usable thread ID remain inspectable globally, with approval
actions disabled. Unchanged question identity and parameters retain the same form
entity across polls, reconnects and session switches. A new Host incarnation or a
resolved request drops the form. Drafts are in memory only, including sensitive
values. A masked field is presentation protection, not secure storage or a
promise about what a provider will persist or echo.

Submitting locks the form against another click. Host acknowledgment removes it
locally immediately; a transport-uncertain result requires explicit reconnect and
an authoritative snapshot before another submission. No automatic resubmission
or ownership takeover occurs. Loss of ownership, provider faults and paused
event replay disable interaction. The Host rejects stale leases and already
resolved request IDs.

## Evidence and limits

See [local verification](../verification/2026-10-19-questions.md). The native
fixture covers several questions, choices, Other, Unicode multiline input,
masking, session routing, reconnect and stop. A real installed Codex app-server
round trip uses a local SSE model fixture and verifies the answer in its next
model request. Authenticated model behavior, IME, other provider question types,
transport fault injection and cross-platform desktop acceptance remain pending.
