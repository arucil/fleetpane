# Stop acknowledgement and original-turn settlement — 2026-10-21

The Session Host now owns Stop progress. A successful interrupt reply no longer
allows input before the original turn finishes; a terminal turn no longer allows
input before the interrupt reply arrives. Native admission returns independently
of that provider reply, leaving other sessions usable. See
[ADR 0057](../adr/0057-host-owned-stop-progress.md) and the source/artifact
[receipt](assets/stop-progress/receipt.json).

## Reproduction and engine checks

The pre-fix owned provider acknowledged Stop, emitted idle, and deliberately held
the original turn completion. The Host still admitted `turn/steer`.
[Recorded failing output](assets/stop-progress/before-fix-output.txt) preserves the
tool result; the private reproduction's snapshot filename was later overwritten
by its passing rerun. [After-fix snapshot](assets/stop-progress/after-fix-snapshot.json)
and [output](assets/stop-progress/after-fix.log) show `stop_pending` instead.
No pre-fix JSON snapshot is claimed.

- `cargo test --workspace --locked`: **183 passed**, including six new runtime
  regression tests and one shared-client test. Both notification orders, exact
  turn identity, idle/wrong completions, definitive errors, explicit retry,
  unchanged full receipt, old/new controller fences, legacy snapshot capability,
  unknown outcomes, malformed terminal status and byte reservation are covered.
- `cargo test --manifest-path apps/desktop/Cargo.toml --locked`: **19 passed**.
  Format checks and strict Clippy pass for both workspaces. Builds pass.
- `python3 scripts/stop-progress-smoke.py`: both orders, rejection/retry,
  receipt UUID replay, controller replacement, independent query/approval and
  provider loss pass with five original interrupt frames and no automatic replay.
- `python3 scripts/acp-cancel-smoke.py`: pending/late permissions cancel, stale
  answers refuse, other-session permission survives, completed archive remains
  intact, and restart while cancelling reports unknown. New duplicate Stop
  identities now receive `stop_pending`; the original UUID still deduplicates.
- `python3 scripts/steering-smoke.py`: exact text, same-turn follow-up, saved
  draft attempts, drain/ownership and no-replay checks pass after the client refactor.
- Runtime, session, drain, ACP mode and permission-review smoke scripts pass.
- `python3 scripts/codex-session-smoke.py`: installed **Codex 0.163.0**, isolated
  CODEX_HOME and two rejecting loopback model requests pass the existing history,
  model-options, directory, archive/restore and ancestry checks. No external LLM.
- A separate installed-Codex process check starts one gated loopback request,
  submits Stop and observes the exact turn become **interrupted**, with no pending
  progress and unchanged Host/provider. A new stale Stop refuses before dispatch.
  The loopback writer logs BrokenPipe after Codex closes its cancelled request;
  this is retained in raw evidence and is not a passing model response.
- `python3 -m unittest discover -s scripts/tests -p '*test.py'`: **13 passed**.
  Ruff check and format pass for the three touched Python scripts. Local checks
  are not hosted CI or macOS/Windows runtime acceptance.

An additional `python3 scripts/concurrency-smoke.py` run timed out waiting for
all twenty turn completion events. The original checker discarded its temporary
state, so that failure has no exact-history snapshot. A diagnostic copy adding
timeout snapshots and histories passed twice with **19 completed / 1 interrupted**
each
and unchanged Host/provider. Both [failure](assets/stop-progress/concurrency-smoke.log)
and [diagnostic result](assets/stop-progress/concurrency-probe-result.json) are
retained. The initial failure's cause is **unconfirmed**; no retry, deadline
extension or production workaround was added. This is an unresolved additional
concurrency check, not accepted performance/capacity evidence. A possible
observer-cursor race remains a hypothesis, not a confirmed root cause.

## Native inspection

The hidden workspace `fleetpane-stop-20261021` uses an owned synthetic provider
and private controller store. The final native binary verifies:

1. A turn ending before the Stop reply displays the exact waiting reason and
   disables Send. It has no remaining active turn, but snapshot progress still
   fences input. [Completion-first view](assets/stop-progress/completion-first.png),
   [snapshot](assets/stop-progress/completion-first.json).
2. During that pending reply, switching to another session leaves its approval
   controls enabled. Native Decline resolves that exact request without completing
   or resubmitting the original Stop. [Other session](assets/stop-progress/other-during-stop.png),
   [after answer](assets/stop-progress/other-answered.json).
3. At **1100 × 650**, an interrupt rejection displays its error, disables input,
   and offers **Retry Stop**. That explicit retry has a new UUID for the same exact
   turn and displays awaiting completion after acknowledgement.
   [Rejected](assets/stop-progress/rejected.png),
   [retry acknowledged](assets/stop-progress/retry-acknowledged.png).
4. Releasing the exact completion removes waiting state and enables Send without
   sending the draft. [Settled](assets/stop-progress/settled.png). The original
   **38-byte** Chinese/multiline draft, newline and two trailing spaces remain at
   revision **3**, with no send attempt. [Exact readback](assets/stop-progress/draft-readback.json).

An earlier native window replacement also retained acknowledged Stop progress,
Host/provider identity and that exact draft without an additional interrupt.
[Reopened view](assets/stop-progress/ack-reopened.png). A native layout refinement
moved waiting/error text from the scrollable interaction region to the fixed
composer area; final-binary small-window evidence covers that refinement. Polling
now invalidates presentation on Stop progress changes even without item changes.

The Host used for native checks predates the shared-client-only nonblocking
admission refactor; its runtime tracker source is unchanged. Final-built-Host
process tests and the final desktop cover the refactored client. The receipt
records both binary identities rather than claiming a matching Host artifact.
The replacement and final desktop close normally with exit code 0. The first
desktop's window closed but its process required SIGTERM during workspace stop;
graceful shutdown of that first instance is not claimed. All owned Host/provider
processes and the workspace were stopped; no user sessions were touched.

## History and limits

Paseo replay **180–185** reads full changes in **11 selected paths**, **31,609 diff
bytes**, preserving the prior 179 ledger entries. Each commit's `plan.md` is
excluded; historical runtime/tests were not run. The
[review receipt](assets/paseo-replay/stop-progress/receipt.json) records hashes and
scope. Next is **186**, `94862b73f46794717a42f27d8c802d7e8e31fa4d`.

Exact turn settlement does not prove subprocess/background resources exited.
Recursive Stop, active archive mutation, live Host takeover, unknown-outcome
recovery and release packaging remain incomplete. No new real DeepSeek/OpenCodex
external model cancellation, voice support or performance gain is claimed.
