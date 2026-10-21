# Durable Paseo batch receipts

Accepted 2026-10-21. Extends [batch migration](0060-bulk-paseo-workspace-history-migration.md).

View-local reports disappear after closing the desktop. Repeating a selection
already preserves imported identities and complete snapshots, but without a
durable attempt record a killed worker cannot explain its last confirmed unit.

Controller schema 10 adds `paseo_batches` and `paseo_batch_results`. A batch saves
its immutable selection/source revisions, provider roots and target endpoint
before discovery. Per-workspace results and aggregate progress checkpoint in one
SQLite transaction before/after each history copy. Archive publication and batch
checkpoint are separate: process death between them may leave a valid archive
whose count is absent from the last receipt. Explicit retry reconciles by reusing
that archive; a running-state receipt alone never establishes completion.

Each attempt holds a private, no-follow regular-file `flock` for its lifetime.
No PID or elapsed-time heuristic establishes worker death. Lock inodes remain in
place. A running receipt with an available lock is presented as interrupted with
completion unconfirmed. Reads do not advance state or start a worker. An async
worker owns the lock independently of its awaiter; explicit cooperative Stop
finishes the current boundary and saves a stopped receipt. Whole-runtime/process
death can still interrupt work. Temporary copy files from process death are not
automatically removed in this slice.

Retry acquires and holds the original attempt's lock, refuses another writer,
then creates a new attempt. The original report stays intact. It revalidates the
original metadata revisions, preserves stale/disappeared selection failures and
reuses first-import identities/complete snapshots. It does not synchronize later
source messages or adopt active provider processes. Concurrent retries of the
same original attempt are refused. New plans or different prior attempts can
still run independently; underlying import transactions and immutable archive
publication enforce identity/content reuse, without a global migration lock.

Summaries have 32-record cursor pages; workspace results have 32-record/256 KiB
encoded data pages, permitting one larger bounded record to make progress. Plan,
progress, error and each result are limited to 4 MiB and the existing controller
database remains limited to 64 MiB. Oversize/corrupt records and storage failures
are explicit errors; no truncation or default-success state. A checkpoint failure
does not roll back already published archives. Existing schema migrations retain
their data; missing tables at the current version fail instead of being recreated.

CLI and desktop use the same Store APIs. The native view loads saved summaries,
pages results, displays original source/target and last checkpoint, and exposes
explicit refresh/retry. It never claims a Host or launches a provider for copying.
Completion means the selection was checked; per-workspace registration errors,
history failures/warnings and incomplete traversal remain separate.

Global archive quotas, later synchronization and active-session ownership
transfer remain separate work. [Local acceptance](../verification/2026-10-21-paseo-receipts.md).

Follow-up [ADR 0065](0065-explicit-paseo-scratch-collection.md) adds explicit
lock-gated collection for new versioned scratch files. Older temporary files
remain retained; automatic collection and quotas remain open.

Follow-up [ADR 0066](0066-cooperative-cli-migration-signals.md) connects CLI
SIGINT/SIGTERM to the existing cooperative Stop and durable reports.
