# 0024: Explicit active-turn follow-ups with durable input receipts

Accepted 2026-10-20.

Codex exposes `turn/steer` with a required `expectedTurnId`, text input and a
`turnId` acknowledgement. The inspected protocol and app-server processor reject
missing, completed or different active turns. Paseo's Codex adapter at
`a486b0558168c08890890e787d55cfb88c0d7256` checks that acknowledgement and keeps
follow-up admission separate from starting a new turn.

Add typed `Command::Steer` to the shared Rust session engine. It carries exact
session/turn IDs, each nonempty and at most 4 KiB, and nonblank text up to 64 KiB.
It sends neither model overrides nor a turn/start fallback. An acknowledgement
must name the expected turn; an unexpected/missing identity is an uncertain
outcome associated with the original operation UUID.

The native **Send to active turn** action requires an active Codex turn,
controller ownership, ready admission, settled draft saves and no unresolved
operation. It uses the existing private, revisioned draft store before dispatch,
retaining method, exact parameters and operation UUID. No schema migration is
needed. A matching successful receipt clears text; provider rejection restores
editable text. Unknown/pending/malformed acknowledgements preserve the attempt
and text, without automatic resend after a window or Host restart. Read-only
recovery checks the original receipt. CLI direct and saved-draft commands use the
same command and validation as the GUI.

Acceptance is not delivery or execution: only provider events add a user item to
the timeline. The UI does not manufacture a second turn, complete a task or
resolve a tool approval. Pending approvals remain separate; the existing question
form still owns structured question replies. Draining permits Stop and answers,
but refuses this additional input. A later UI poll cannot retarget an already
saved request to another turn.

DeepSeek ACP v1 has no equivalent follow-up operation in the installed adapter.
The UI explains that limitation while active and keeps ordinary Send disabled;
CLI gets an explicit unsupported response with its draft retained. Stop followed
by a normal send is a user action. Cross-provider queued input, mode transitions,
permission-clear semantics and model changes during a turn remain separate work.
See [verification](../verification/2026-10-20-active-turn-follow-ups.md).
