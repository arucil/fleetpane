# ADR 0047: Observe an exact turn independently of sending

Status: implemented; Unix CLI/protocol acceptance is recorded separately.

Paseo replay 140 separates nonblocking prompt dispatch from `wait_for_agent`.
Fleetpane already distinguishes dispatch receipts from turn completion, but its
shared CLI had no wait command. A caller should be able to wait for a specific
turn without taking control, accidentally waiting on a newer turn, answering an
approval or turning a timeout into Stop.

Add `Connection::wait_turn` and `session SOCKET wait THREAD TURN`. The CLI reads
the existing controller lease; it never claims one, and rejects `--take-control`.
An unowned Host requires a separate explicit controller action. Initial bounded
turn-history queries use that lease; subsequent snapshots/events bind to the same
Host incarnation and continue across controller replacement. No provider mode,
draft, operation receipt, admission or process lifetime is changed by waiting.

The report distinguishes `finished`, `unknown`, `attention` and `timed_out`.
Finished preserves completed/failed/interrupted, error and explicit stop reason;
none establishes task success. Unknown retains an unfinished archived turn after
Host restart and never resumes/replays it. Attention preserves exact pending
request objects. A thread-only ACP permission is bound to the Host's active turn;
an explicit Codex turn ID must also match. Other sessions and later turns cannot
satisfy the wait. Requests remain observations and may resolve after the report.

An exact `(thread_id, turn_id)` in the initial Host activity snapshot identifies
the running target without a provider history query. On installed Codex 0.163.0,
`turn/start` can return before `turn/started`, while the first-turn history store
still rejects `list_turns`. The Host now tracks the admitted start's thread and
successful reply's in-progress turn. Pending starts retain bounded matching
completion identities so a delayed reply cannot reactivate a finished turn,
including when a newer turn has already started. Rejections never create activity;
invalid known reply fields and exhausted bounds remain explicit errors.

When activity does not identify the exact target, find it through metadata-only
history pages, retaining no item transcript. Incomplete activity, other threads
and other turns cannot bypass lookup. Lookup has a 4,096-page bound and rejects
repeating cursors/missing targets. Capture an event cursor **before** lookup: the query reply's sequence is
its arrival boundary, not a provider snapshot version. A completion can precede
a delayed stale active read. A regression reproduced a false timeout before this
correction. Replay from the earlier cursor; advance only to delivered events,
never to a snapshot ahead of a bounded event page.

Snapshots/events are polled at 250 ms when caught up; backlog drains immediately.
This uses existing observational RPCs rather than a new Host protocol or push
subscription. A finite 1–3,600,000 ms deadline (default 30,000) covers shared wait
lookup, I/O and observation. CLI Host discovery happens before this deadline and
has the existing transport deadline. Timeout and dropping/killing the observer
cancel observation only. Fault, expired cursor, ownership conflict during lookup,
protocol error, socket loss or incarnation replacement returns an explicit error;
none triggers retry, takeover, resume, Stop or new prompt.

No MCP server, activity transcript export, daemon supervisor or new UI is part of
this change. Linux acceptance uses both protocol fixtures and the installed Codex
app-server with a gated loopback SSE model. Broader provider/model versions,
remote transports, unbounded lookup, performance budgets and sustained high-rate cursor retention
need separate acceptance. [Original verification](../verification/2026-10-21-wait-turn.md).
[First-turn regression and real OpenCodex/DeepSeek acceptance](../verification/2026-10-22-wait-first-turn.md).
