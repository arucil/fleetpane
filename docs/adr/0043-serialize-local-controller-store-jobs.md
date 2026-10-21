# ADR 0043: Serialize local controller store jobs before opening SQLite

Status: implemented; local verification recorded separately.

Full workspace regression exposed `database is locked` errors in concurrent
recent-directory writes and missing successes in the 32-workspace pin capacity
test. Each Store job opened its own SQLite connection on Tokio's blocking pool.
Opening also performs an immediate schema transaction, so reads and writes from
one controller competed for the same database lock before their actual action.
SQLite's two-second busy timeout was not a scheduler for these jobs.

Share a FIFO async mutex for each canonical store directory within the process.
Resolve this identity after validating the private, non-symlink leaf directory,
on the blocking pool. A short registry lock associates independently opened
handles and ancestor aliases with the same queue. Weak entries do not keep idle
stores alive and are pruned on the next open. Distinct stores have independent
queues. Keep the original configured path for workspace-root behavior and retain
the existing private-directory/database checks on every connection open.

Acquire the queue before spawning the SQLite job. Move the owned mutex guard into
the blocking closure and drop the database before the guard. Cancelling an async
caller does not cancel an already spawned blocking action: it may still commit,
and the queue must remain held until that action actually finishes. Cancelling a
caller still waiting for its queue removes that waiter without running its action.
Errors release the slot normally and remain visible to the caller.

Keep SQLite immediate transactions, revision/CAS checks, the existing two-second
cross-process busy timeout, schema 7 and durability settings. No retries, longer
busy timeout, WAL migration or empty-state fallback are introduced. Independent
processes still coordinate through SQLite and may report an external lock error;
this queue does not promise cross-process fairness. Same-user directory aliases
are supported, not adversarial path rebinding after Store open.

The queue covers local preferences, drafts, creation bindings and workspace
records. It does not serialize provider RPC, Host streams, terminal I/O, filesystem
listing or rendering. An async operation waiting for the database does not occupy
a blocking worker. This is a correctness repair, not a measured UI latency or
multi-agent capacity claim.

[Verification](../verification/2026-10-21-controller-store-queue.md).
