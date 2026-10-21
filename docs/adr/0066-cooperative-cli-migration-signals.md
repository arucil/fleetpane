# Cooperative CLI migration signals

Accepted 2026-10-21. Extends [durable migration reports](0064-durable-paseo-batch-receipts.md).

The native batch import page has an explicit Stop that finishes its current
unit. The CLI previously used the same engine with a default control, but never
connected OS signals to it. Ctrl+C/SIGTERM therefore terminated the process,
leaving an interrupted report and possibly temporary files despite the engine's
available cooperative cancellation contract.

The CLI installs Unix SIGINT and SIGTERM listeners before polling `batch` or
`retry-batch`. Either signal sets the existing `BatchControl` cancellation flag,
prints a waiting message to stderr and awaits the original worker. The worker
finishes the current registration/copy boundary, writes its stopped receipt and
releases its ownership locks. Stdout contains the ordinary single JSON report
with `cancelled: true`. A successfully saved Stop returns exit code 0; operational
and signal-registration failures remain errors. Callers must inspect the report
to distinguish stopped traversal from completion and per-history failures.

Repeated SIGINT/SIGTERM do not escalate to forced termination. Tokio's installed
handlers remain in place while the original future is awaited; no second worker,
automatic retry or short timeout is added. Waiting is intentionally bounded by
the existing copy/I/O behavior, not by an arbitrary forced-exit timer. Users can
still force SIGKILL, which retains the earlier interrupted-report semantics.

Signal registration belongs to the CLI boundary, not the migration engine or UI.
There is no schema change. Read commands, standalone history-copy commands,
provider Host signal handling and native window behavior are unchanged. A CLI
closed by terminal SIGHUP is not covered by these two listeners.

[Local process verification](../verification/2026-10-21-paseo-signals.md) observes
actual scratch creation, pauses the writer, delivers the signal and resumes it.
It verifies complete 60,000-row archives, durable stopped reports, unchanged
original reports/source and explicit retry from both initial and retry commands.
