# 0057 — Keep Stop progress with its exact Host and turn

Status: accepted for the local prototype, 2026-10-21.

## Context

The existing Codex Stop path treats an interrupt RPC acknowledgement as command
completion. It does not prevent a follow-up while the original turn is still
running. Conversely, a terminal turn can precede the interrupt reply. Idle status,
window lifetime and control ownership do not prove those two boundaries arrived.
Paseo replay 180–185 also separates independent agents from voice interruption;
the Stop icon alone supplies no lifecycle guarantee.

## Decision

The Host tracks thread ID, turn ID and admitted operation UUID. Its snapshot
reports dispatching, awaiting completion, rejected or unknown, plus any observed
terminal status. Both RPC reply and an exact completed/interrupted/failed turn
notification are required to retire progress. Idle or another turn's completion
cannot retire it. No timer claims completion.

While progress exists, provider mutations targeting that thread are refused with
`stop_pending`. A definitive rejection permits one explicit new Stop identity;
new input stays fenced until the original turn settles. Repeating an existing
UUID uses its receipt and sends no new frame. Other sessions, observational reads,
answers and Host-owned terminals remain independent. Existing ownership and drain
fences still apply. Provider/Host failure marks progress unknown.

Progress is bounded to 128 entries and a 64 KiB reservation, including room for
bounded error excerpts. Original errors remain in the operation receipt. The
optional snapshot field distinguishes older Hosts from a supported empty list;
updated clients refuse Stop on older Hosts instead of guessing its progress.

The desktop waits for Host admission, then polls progress without holding its
global busy state until the provider reply. Admission transport loss retains the
UUID for explicit receipt reconciliation. Fixed composer-area messages keep
waiting/rejection visible in small windows; Stop does not clear or submit drafts.
Snapshot progress changes invalidate presentation independently of conversation
rows. CLI execution still returns the interrupt acknowledgement or error and does
not claim that the target or its resources have finished.

## Limits

The tracker survives control-client replacement, not Host/provider replacement.
Old operation receipts never establish activity in a new Host incarnation. There
is no subprocess/background resource census, recursive Stop, active archive,
rollback, automatic retry or live Host takeover in this slice. Existing ACP
cancellation still cancels its pending permissions and awaits prompt settlement;
Codex permissions are left provider-authoritative. See
[local verification](../verification/2026-10-21-stop-progress.md).
