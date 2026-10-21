# Controller store queue — 2026-10-21

Local source state is the retained work on `c91d269247207a08898e264c81083df4fbd4e745`.
[Receipt](assets/controller-store-queue/receipt.json) binds source and log hashes;
this is not remote CI, deployment or release acceptance.

The image-feature full regression first exposed lock errors in concurrent MRU
writes and only 29 successful pin writes where exactly 32 should succeed. The
original failure log is preserved. A new cancellation/alias test fails before the
repair with `Local state: database is locked`: a caller is cancelled while its
blocking transaction remains active, and another handle opens the same database
through an ancestor alias. With the repair, the alias open remains asynchronously
queued beyond the unchanged SQLite timeout, can be cancelled safely, and opens
normally after the original transaction commits. An independent store can open
while this transaction is held.

A second test verifies queued cancellation never runs its action, FIFO ordering
across independently opened handles and releasing the next slot after an action
error. Existing MRU and pin concurrency tests now mix cloned and independently
opened handles. Pin failures must be the explicit capacity refusal, rather than
being silently counted as generic failures. Existing schema migrations, corruption
errors, private-file/symlink validation and exact draft revision checks still run.

The workspace suite passed 149 tests and desktop passed 11; final storage test
strengthening reran all six storage tests. Formatting, strict Clippy, Linux builds
and the Windows engine cross-check passed locally.

The affected shared engine, desktop build/tests and CLI process workflows are
checked with the commands in the receipt. Draft CLI contention still has one CAS
winner across separate processes; pin, recent-directory and appearance process
checks retain data after reopening. Image smoke reruns the durable combined
text/image draft flow. No new native interaction is claimed by this engine repair;
the image [native acceptance](2026-10-21-image-attachments.md) records its own
earlier source/binary identities separately.

Remaining limits: external-process locks can still produce an explicit SQLite
busy error; a spawned job can commit after its caller is cancelled; no UI latency
or controller database throughput benchmark is claimed. See
[ADR 0043](../adr/0043-serialize-local-controller-store-jobs.md).
