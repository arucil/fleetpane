# Agent activity and concurrency observer — 2026-10-21

The selected native session now distinguishes Agent work, waiting for a response,
Host-owned stopping, synchronization and unavailable/archived/unknown context.
The status stays beside the composer while conversation/details scroll.
The header's pending UI action uses a different label.
[Decision](../adr/0058-observed-agent-progress.md),
[source and binary receipt](assets/agent-progress/receipt.json).

## Verification

- `cargo test --workspace --locked`: **187 tests pass**, including four new
  shared progress tests. After the final priority/label refinement, the four
  affected progress tests pass again. Exact session/known-turn matching,
  ACP requests without turn IDs, snapshot activity before paged history,
  archived-but-running observation, stale history, unknown and Host fault are
  covered. Those exceptional states have unit evidence, not new native acceptance.
- `cargo test --manifest-path apps/desktop/Cargo.toml --locked`: **19 pass** on
  the final source. Strict Clippy passes in both workspaces; format checks and
  Host/native debug builds pass. Engine all-target Clippy also passes for the
  Windows GNU cross target; this is compile evidence, not Windows runtime QA.
- `python3 -m unittest discover -s scripts/tests -p '*test.py'`: **15 pass**,
  including two concurrency observer regressions. Ruff format/check pass for the
  two touched scripts. The existing CI test pattern `*_test.py` includes them.
- `python3 scripts/stop-progress-smoke.py`: both completion orders,
  rejection/explicit retry, UUID replay, controller takeover, independent
  approval and provider-loss checks pass.
- `python3 scripts/concurrency-smoke.py --output ...`: two corrected-observer
  runs pass with **19 completed / 1 interrupted**, exact Unicode text and
  single execution, unchanged Host/provider and stale-owner refusal. The final
  run's [raw output and samples](assets/agent-progress/concurrency-final.json)
  are retained. These debug runs under other local work are correctness evidence;
  they do not replace the old release measurement or establish a speedup.

## Root cause in the concurrency checker

The old checker started all twenty turns, queried them all, then used a new
snapshot sequence as its first event cursor. A completion during startup/read
was outside that cursor and would never enter its finished set. A deterministic
fake transport emits completion during each start: the old ordering fails,
the corrected pre-start cursor passes. The
[failure](assets/agent-progress/race-before.log) is deliberately expected evidence.
This proves a checker defect; the previous turn's deleted timeout state still
cannot establish that this was the cause of that particular timeout.

Simply moving the cursor earlier produced an explicit
[expired-cursor failure](assets/agent-progress/early-cursor-error.log): the Host
retains 128 events, and startup/queries had not consumed any of them. The final
checker consumes events between every start and every metadata read, advances
only delivered event boundaries, and matches both session and original turn ID.
It keeps cursor expiry as failure. It does not increase Host retention, extend
deadlines, add automatic retries or accept history-only settlement instead of
completion events. Failure snapshots, histories and Host logs are captured before
temporary-directory removal for cursor, active-count and settlement failures;
capture errors remain visible. `--output` persists that diagnostic JSON.

## Native inspection

The hidden workspace `fleetpane-progress-20261021` uses synthetic Codex-protocol
sessions and private local storage, with no external model or user session.

1. An indefinitely silent `[wait]` turn shows the Kit spinner and
   **Agent working…** after its start action has completed.
   [Working](assets/agent-progress/working.png).
2. Native **Stop turn** gets an acknowledgement but the provider holds original
   completion. The spinner becomes **Stopping…**, Send stays disabled and the
   exact original target remains in Host activity despite provider idle.
   [Stopping](assets/agent-progress/stopping.png),
   [snapshot](assets/agent-progress/stopping.json).
3. At **1100 × 650**, switching to the other session shows **Waiting for your
   response · 1 pending**. Scrolling details exposes enabled **Allow once** and
   **Decline** without removing composer/status. Native Decline answers that
   request. [Controls](assets/agent-progress/waiting-buttons.png).
4. Releasing original completion clears stopping. Returning to the first session
   displays **No running turn observed** and enables Send without sending the
   saved draft. [Settled](assets/agent-progress/settled.png),
   [snapshot](assets/agent-progress/settled.json).
5. A replacement window with explicit takeover restores the same **38-byte**
   Chinese/multiline draft, newline, indentation and two trailing spaces, at
   revision **1**, attempt **null**. Host **1130862**, provider **1130864** and
   session IDs remain unchanged; no Stop/send is replayed.
   [Reopened](assets/agent-progress/reopened.png),
   [exact draft](assets/agent-progress/draft-readback.json).

Both native windows closed, but both processes remained alive and required
SIGTERM during isolated workspace stop. Graceful process exit is **unverified**
and remains a follow-up, as in prior native checks; no workaround was added.
The initial owned setup accidentally omitted fixture-required `historyMode` and
its provider exited. It was replaced before native testing, leaving one unknown
receipt in the same private journal; final snapshots expose it. This is not a
claim that every historical receipt resolved. Cleanup confirms both desktops,
Host socket and provider were removed; no user processes were touched.
[Cleanup](assets/agent-progress/cleanup.json).

## History and limits

Paseo continuous scoped review now reaches **193 · `9feec8104`**. Eight commits,
34 selected path occurrences and **94,461 diff bytes** were read; prior 185
entries are unchanged. Plan/prompt/lockfile/PNG content exclusions are explicit
in the [review receipt](assets/paseo-replay/working-state/receipt.json).
No historical app, dependency, typecheck or test execution is claimed.

Voice stays P2. Git Diff expansion stays deferred. External DeepSeek/OpenCodex,
macOS/Windows native status, reconnect/fault native acceptance, graceful close,
resource-complete stop, active archive, live Session Host upgrade and release
performance remain separate work. There is no hosted CI or publication claim.
Next history commit: **194 · `95bacf8fc7dd7ba86b7442011c624109e5315174`**.
