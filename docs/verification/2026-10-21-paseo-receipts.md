# Saved Paseo migration reports and interrupted-worker retry — 2026-10-21

The shared engine, CLI and Linux native import view retain migration attempts
across application replacement. Reports preserve the last confirmed progress and
per-workspace diagnostics. Explicit retry checks the original selection and saves
a new attempt, reusing imported identities/complete archives. Reading a report
does not resume work. See [ADR 0064](../adr/0064-durable-paseo-batch-receipts.md).

[Evidence receipt](assets/paseo-receipts/receipt.json) binds source, binaries,
commands and artifacts. Fixtures and screenshots are synthetic.

## Automated verification

- `cargo test --workspace --locked`: **218 tests pass** (4 client, 4 protocol,
  63 runtime, 147 session). The 31 import tests include receipt reopen, exact
  diagnostics, source revision revalidation, prior-report preservation,
  interruption versus an active writer, lock-file validation, schema-9 upgrade,
  current-schema damage refusal, 70-result/35-summary cursor traversal, byte
  bounds, oversize reads and atomic checkpoint rollback. Cancelled awaiter
  acceptance copies all 60,000 rows before the worker releases its lock and
  saves a stopped receipt; an overlapping retry is refused.
- `cargo test --manifest-path apps/desktop/Cargo.toml --locked`: **19 pass**.
- Both workspace/desktop Clippy commands with `--all-targets --locked -- -D
  warnings`, both format checks, Host/desktop builds and `git diff --check` pass.
- `python3 scripts/paseo-receipts-smoke.py`: a real subprocess starts a synthetic
  batch, checkpoints one complete history, then is killed during the following
  60,000-item copy. CLI refuses active-worker retry, reports interruption after
  death, reads the completed archive and proves reading starts no second attempt.
  Explicit retry creates a new receipt, reaches **4 available histories / 1
  unsupported-provider failure**, preserves original report/session identities/
  complete snapshot hashes and leaves source hashes unchanged. No Host socket or
  provider starts. The new smoke command is configured in non-Windows CI; hosted
  CI has not run for this working tree.
- Existing batch, single-workspace, appearance and recent-directory CLI smoke
  commands pass, including schema upgrades with exact saved drafts retained.
  `python3 -m unittest discover -s scripts/tests -p '*_test.py'`: **15 pass**.
  Ruff is unavailable in this environment; no Ruff success is claimed.

## Native acceptance

Isolated hidden Linux workspace `fp-paseo-receipts`, display 1500 × 1100,
1100 × 720 application window. A private synthetic source/controller/preferences
profile is used; the configured Host remains offline throughout.

The native view displays completed and interrupted attempts loaded from the
process-kill fixture. Opening the interrupted report shows the retained single
history and incomplete traversal. **Retry saved selection** displays progress,
then completes with four histories and one unsupported-provider failure. CLI
readback confirms a new batch UUID and unchanged workspace/session IDs and
archives; the interrupted report stays unconfirmed.

A final desktop build adds the saved last checkpoint and readable shared error
details. It is relaunched against the same controller, proving summaries and
completed reports survive application replacement. Details show the failing
provider session name/ID and full cause. The final binary is checked against its
running `/proc/PID/exe`. Native final retry/readback and screenshots are retained
alongside this document. Native pagination of more than 32 attempts/results is
not exercised; those bounds have shared-engine tests.

## Limits and cleanup

This is offline archive migration with explicit retry, not synchronization or
running-session adoption. Completion is selection traversal, not zero failures.
SIGKILL leaves private `.tmp`/SQLite journal files from the interrupted copy;
they are not visible histories and the retry publishes a separate complete
archive. Automatic cleanup and archive-wide quotas remain open. No full real-user
migration, new real model calls, cross-platform native acceptance or throughput
benchmark is claimed. Initial screenshots precede the last details/checkpoint
wording update; final screenshots and readback use the final binary.

Only task-owned desktop apps and hidden workspace are stopped after acceptance.
No user desktop, existing Paseo daemon, provider or account is modified. No commit
or push is made by this slice.

Follow-up [scratch cleanup verification](2026-10-21-paseo-scratch.md) covers
explicit collection of newly versioned temporary files; the legacy files from
this earlier implementation are reported and retained.
