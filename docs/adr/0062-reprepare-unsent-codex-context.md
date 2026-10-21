# Explicitly reprepare unsent Codex context

Accepted 2026-10-21. Extends [independent continuation](0061-independent-codex-history-continuation.md).

A saved preparation pins a Host incarnation and admission generation. A Host
restart or drain/ready transition makes it stale even when no fork was attempted.
Keeping that stale binding forever prevented the user from proceeding.

`reprepare_paseo_continuation(session, expected_operation)` explicitly observes
the same imported workspace endpoint, validates its current directory/repository
identity and raw archive export, then observes a ready Codex Host. It saves a
new operation UUID and current Host/generation through a conditional transaction:
the current UUID must equal the caller's expected UUID and `attempted` must still
be false. Failure leaves the previous record available. No Host is claimed and
no provider mutation is sent. The imported history/export is reused unchanged;
repreparation does not synchronize newer source messages or change endpoints.

Dispatch also requires its caller's expected operation UUID. A stale desktop
cannot dispatch a replacement it never reviewed, even when replacement occurs on
the same Host/generation. The final attempt transition compares that UUID again,
so competing replacement and dispatch cannot both win. Initial preparation still
returns an existing binding without a Host probe; it never replaces implicitly.

Native **Reprepare unsent context** and CLI `paseo-import reprepare-fork STORE
SESSION_UUID EXPECTED_OPERATION_UUID` use this shared path. Native rereads saved
status after replacement; if another window sent first, it reports the conflict
and offers the normal saved-status check. Its dispatch gate remains explicit
controller ownership plus **Fork saved context**. A locally disabled dispatch
after a pre-admission refusal can be reconciled through **Check saved fork status**.

Attempted, pending, created, failed and unknown bindings cannot be replaced.
This includes the pre-admission-crash window with no operation receipt; missing
proof never authorizes a second send. Multiple independent forks, changing the
target endpoint and durable audit history for replaced unsent bindings remain
open. No schema upgrade is needed: schema 9 already stores these identities.

[Verification](../verification/2026-10-21-paseo-reprepare.md).
