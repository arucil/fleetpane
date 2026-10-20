# Durable operation receipts and conservative crash recovery

Status: accepted for this local Host slice, 2026-10-20.

Compatibility update: [ADR 0016](0016-persistent-host-admission-drain.md) advances
the Host protocol to 3 and journal schema to 2, adding durable admission drain.
The protocol-2 description and verification below record the preceding slice.

The former in-memory journal retained every mutation UUID and stopped admitting
work after 128 operations, even when every operation had finished. Evicting UUIDs
would allow a controller retry to dispatch a mutation twice. The old executable
reproduced this rejection after 128 settled mutations.

Each Unix endpoint now owns `SOCKET.journal/operations.sqlite3` and an exclusive
`owner.lock`. The directory is 0700 and files are 0600. A single SQLite connection
runs serialized jobs through Tokio's blocking pool; the Host awaits each job,
with no unbounded storage queue or filesystem work on the rendering thread.
This adds `rusqlite` 0.40.2 with bundled SQLite to the Unix runtime dependency.

Before writing a mutation to provider stdin, commit its UUID, exact method/params,
admitting Host incarnation and `accepted` outcome. Commit provider RPC results
before publishing them through operation status or the event ring. SQLite uses
4096-byte pages, DELETE rollback journaling and `synchronous=EXTRA`; directory
creation and the initial database directory entry are synced. These guarantees
depend on the local filesystem/device honoring sync and locking. Local process
kill tests do not establish power-loss durability or network-filesystem support.

Every execute first looks up the durable UUID. An identical request returns its
existing receipt; different method/params returns `operation_conflict`. Settled
RAM records may be evicted, but their identities and results remain on disk.
The resident set is at most 128 records / 8 MiB, including a maximum response
reservation for each in-flight record. Pending records cannot be evicted.
The same count and byte limits now bound outstanding work rather than lifetime
work. The database has a 256 MiB page budget and a 2 MiB SQLite cache budget;
rollback files add transient disk overhead. Disk errors stop further mutation
admission. There is no automatic receipt deletion, compaction or retention policy.
Changing sockets or deleting the journal creates a new deduplication domain.

Startup checks the schema, database integrity and binding to canonical provider
path, arguments, workspace and configured HOME/CODEX_HOME/DSH_HOME paths. The
binding detects configuration changes, not credential/account changes within the
same configured home. It does not copy credentials. The lock spans the entire
Host lifetime. Another owner, a mismatched binding/schema, corruption or unsafe
file permissions rejects startup. Under that lock, a refused regular Unix socket
can be removed after a crash; live listeners, symlinks and arbitrary files are
never removed.

Each restart still creates a fresh Host incarnation, controller lease domain and
event sequence. All saved `accepted` receipts become `unknown` before serving
requests: the provider may have acted before the Host lost its response. Never
automatically replay these operations. Previously finished responses remain
readable, with `completed_sequence: null` when read from another Host incarnation.
Clients must not apply an old sequence to a new conversation projection. The
shared client reports a historical receipt explicitly instead of applying it.

Protocol version 2 makes bounded discovery explicit. `snapshot.operation_ids` is
the resident subset and `journal_total` is the durable receipt count. Read-only
`operations` returns 32 metadata rows per page, newest first, with a journal-bound
position cursor unaffected by prepending new admissions. `operation UUID` returns
the original request and result. CLI entry points need no controller claim:

```sh
fleetpane-host operations SOCKET [--cursor CURSOR]
fleetpane-host operation SOCKET UUID
```

Update the disposable Host and clients together; v1 clients cannot claim v2
Hosts. The native window may reconnect while an operation needs reconciliation.
It keeps the old UUID and checks its status without dispatch. An unknown receipt
keeps normal actions blocked until the user explicitly selects **Continue without
resending**. The window then requires a history reload/reconnect or a different
session before continuing conversation work. Acknowledgement does not change the
saved outcome, establish whether the side effect happened, or resubmit anything.

This journal stores RPC receipts, not the complete provider conversation. In
particular an ACP `host-dispatched` receipt is not a completed agent turn. It does
not restore provider stdio, an in-flight tool, pending approvals, private provider
state or ACP observed history after a Host crash. A full native receipt browser,
durable routing, provider reconciliation and overlapping Host version drain
remain separate work under ADR 0002.

See [verification](../verification/2026-10-20-durable-journal.md). Storage API
references: [rusqlite connection/transactions](https://docs.rs/rusqlite/0.40.2/rusqlite/struct.Connection.html),
[SQLite synchronous semantics](https://www.sqlite.org/pragma.html#pragma_synchronous),
[Rust file locking](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock).
