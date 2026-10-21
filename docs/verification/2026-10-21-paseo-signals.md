# Cooperative Paseo CLI signal stop — 2026-10-21

CLI batch migration and retry now handle SIGINT/Ctrl+C and SIGTERM through the
engine's existing cooperative Stop. They await the current unit and durable
stopped report before returning one JSON result. See [ADR 0066](../adr/0066-cooperative-cli-migration-signals.md).

[Evidence receipt](assets/paseo-signals/receipt.json) binds the candidate source,
binary and process artifacts. Tests use synthetic local sources with the target
Host offline.

## Process acceptance

`python3 scripts/paseo-signals-smoke.py` creates two 60,000-row Codex histories
plus the existing Codex/DeepSeek/unsupported-provider fixture. It observes the
batch's committed current-session checkpoint and actual scratch file before
SIGSTOP, then uses `waitpid(WUNTRACED)` to confirm the writer is stopped. SIGINT
is delivered twice before SIGCONT. The process exits 0 with one JSON report,
`cancelled: true`, a durable stopped receipt and one complete 60,000-row archive.
No next history is copied and no scratch file remains.

An explicit `retry-batch` reuses the first archive and is stopped with repeated
SIGTERM during the second 60,000-row copy. It finishes that archive, saves a new
stopped receipt with two available histories, and releases its writer lock.
Another explicit retry finishes with four available histories and one explicit
unsupported-provider failure. Both earlier reports stay unchanged, complete
archive hashes and source hashes match, and no provider/Host starts. Repeated
signals are exercised without changing the worker identity; Unix standard
signals may coalesce while a process is stopped.

The new test is configured in non-Windows CI. Linux local acceptance does not
establish macOS runtime acceptance. SIGKILL remains covered by the separate
[interrupted report](2026-10-21-paseo-receipts.md) and
[scratch cleanup](2026-10-21-paseo-scratch.md) checks.

## Other checks and limits

Host build/tests, workspace Clippy with warnings denied, root format check and
Python syntax compilation pass. The earlier checkpoint's full local suite
passed 223 engine tests, 19 desktop tests, 15 Python tests and 42 process smoke
commands. These broad results belong to that checkpoint; the CLI-only change has
its own targeted signal process check and compiler/lint checks.

The signal handler does not alter standalone `copy-history` or
`copy-workspace-history`, read commands, SIGHUP, power-loss behavior or native
window lifecycle. Storage failures still report an error instead of claiming a
saved Stop. No real-user full migration, model call, performance comparison,
Windows signal support or live session adoption is claimed.
