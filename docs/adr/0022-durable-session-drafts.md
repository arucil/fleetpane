# 0022: Durable session text drafts and conservative send recovery

Accepted 2026-10-20.

## Problem and reference

Composer text previously lived only in a Host view's session map. Replacing the
desktop lost unsent work. Persisting text alone would create another failure:
after a send reached the Host but the desktop died before clearing the draft,
reopening could present that text as an ordinary message ready to send again.

Paseo's inspected local draft store (`packages/app/src/stores/draft-store/`) uses
scoped identities, persistence coalescing and explicit lifecycle state. Fleetpane
adopts those principles in its shared Rust session engine; no reference code or
assets are copied. Its independently surviving Host and durable RPC receipts
also let recovery distinguish editing from an attempted send.

## Decision

- Store text in controller SQLite schema 4, transactionally migrating schemas
  1–3 while preserving creation, workspace and worktree records. Host protocol
  and journal remain v3/schema 2. The existing private file permissions, 64 MiB
  database limit, synchronous transactions and two-second SQLite busy deadline
  remain in force.
- Key drafts by exact socket, Host incarnation UUID, session ID and cwd. A new
  Host incarnation cannot inherit an earlier draft just because a provider
  reuses a session ID. CLI recovery lists earlier records in 32-row pages.
- Each write compares the saved revision and previous serialized data in one
  immediate transaction. Empty cleared rows retain their revision, preventing
  stale windows from resurrecting sent text. Conflicts preserve local input and
  require copying/reloading or resolving the error explicitly.
- Before dispatch, bind the draft to an operation UUID and exact provider RPC
  method/params. Neither editing nor another send can replace that attempt.
  Recovery only queries the original Host's receipt and checks that binding.
  Matching finished success clears text; finished rejection restores editing.
  Pending/unknown/transport failure preserves the attempt and text for review.
  Recovery never dispatches or claims control. A normal explicit send uses the
  existing controller lease and admission checks.
- Use the same engine from CLI and GPUI Kit. All SQLite/Host I/O runs off the
  render thread. Each opened Host view keeps one current draft, at most one
  save in flight and one coalesced latest text. Returning from A to B to A while
  B is being saved must still persist A.
- Disable navigation/send while saving the current text or resolving a save
  error. Host tab switching can leave that view's save running. A normal window
  close waits for all visited views' saves; save errors keep it open with text
  available to copy. Forced termination can restore only the last confirmed
  save. Pending sends are durable and do not need a window to remain open.

## Limits and consequences

Only selected Host-backed session text is persisted, at most 64 KiB per record.
Drafts are local plaintext protected by filesystem permissions, not encryption.
Question answers, model options, media attachments, new-session input, layout and
restored selection are separate work. Earlier-incarnation and unknown-send
recovery remains conservative: copy/review the retained text, without a resend
or automatic reassignment control. This does not provide active provider
restoration after a Host restart.

Schema-4 stores are incompatible with earlier controller binaries. Existing
worktree/Git behavior is unchanged; further Git review work is deferred by the
user. Verification is recorded in
[the local receipt](../verification/2026-10-20-session-drafts.md).
