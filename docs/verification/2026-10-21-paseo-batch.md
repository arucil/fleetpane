# Bulk Paseo workspace and history migration — 2026-10-21

**Workspaces → Import from Paseo** now supports workspace title/path filtering,
individual selection, Select matching/Clear and **Migrate selected + history**.
It sequentially imports the selected identities and copies their supported
histories. Progress, cooperative Stop, per-workspace diagnostics and repeated
execution use the shared engine. CLI exposes reviewable JSON plans and results.
This extends [archive import](2026-10-21-paseo-import.md), with the bounds in
[ADR 0060](../adr/0060-bulk-paseo-workspace-history-migration.md).

The [receipt](assets/paseo-batch/receipt.json) binds the final source and binaries
to these artifacts. Public fixtures/screenshots contain synthetic data only.

## Automated checks

- `cargo test --workspace --locked`: **201 tests** pass: 4 client, 4 protocol,
  63 runtime and 130 session. Four added migration tests cover independent stale/
  disappeared selections, missing directories/logs, unsupported providers,
  repeated identity/snapshot reuse after source-log removal, 71-session paging,
  incomplete-tail warnings, invalid/duplicate/relative selections, pre-start
  cancellation and cancellation after a copy starts. The in-flight regression
  verifies all 10,000 projected rows were published before cancellation settled,
  then resumes the remaining history with the original snapshot preserved.
- All **14 migration tests** passed in the focused run before the final wording/
  progress-task changes; the final whole-workspace run includes all 14 again.
- `cargo test --manifest-path apps/desktop/Cargo.toml --locked`: **19 tests** pass.
- Root and desktop `cargo clippy ... --all-targets --locked -- -D warnings`, both
  format checks, Host/desktop builds and `git diff --check` pass.
- `python3 scripts/paseo-batch-smoke.py`: plan → batch → repeat → source change
  → explicit selected failure with independent successful import. Missing
  directory retained, two provider histories available and two explicit history
  failures, source SHA-256 invariance and no Host socket.
- The existing single-workspace migration, appearance and recent-directory smoke
  tests pass. `python3 -m unittest discover -s scripts/tests -p '*_test.py'`:
  **15 tests** pass. The batch smoke is added to non-Windows CI; hosted CI has
  not been run for this working tree.

## Native behavior

Hidden Linux workspace `fp-paseo-batch`, display 1500 × 1100, application window
1100 × 720. Synthetic source: three workspaces, 16 associated sessions. A large
workspace has twelve 60,000-item Codex logs, alongside a Unicode workspace with
Codex/DeepSeek/unsupported-provider sessions and a missing-directory/log case.
The configured Host socket remains absent throughout. No provider launches,
model calls, source changes or running-Paseo takeover.

The initial desktop selected all, began copying, displayed live progress and
disabled navigation. Stop settled after five complete archives. The report
retained the imported workspace and marked history traversal incomplete. Repeat
finished all three workspaces: **14 available histories / 2 history failures**.
The reused workspace was labeled Already imported. Filtering to `missing`
displayed one candidate while retaining the three selected IDs, making hidden
selections visible in the count. Details displayed the unsupported-provider
failure. These initial screenshots precede a small progress-task lifetime fix.

The final desktop was rebuilt with one retained polling task per batch and an
initialized total count. Its running executable matches the on-disk binary.
After reopening the original controller, the saved workspaces/session archives
remained available. Repeat produced the same result and marked every workspace
Already imported. Native readback plus CLI/SQLite checks confirmed **3 stable
workspace IDs, 16 stable session IDs, 14 complete snapshots, 46 unchanged source/
archive files and zero temporary archive files**. Final screenshots cover
selection/filtering, report counts, an error containing the provider session
name/ID and saved DeepSeek Chinese/Markdown history browsing.

The same final binary was also launched against a fresh task-owned controller.
Stop during the first large copy retained one complete 60,000-item snapshot and
the workspace's twelve session identities, with an incomplete-history report.
Repeating the selection completed the remaining copies and reused that exact
snapshot and its identities. The cancellation readback records final counts,
snapshot preservation, private temporary-file cleanup and source invariance.

The initial app used the machine's default appearance path, whose missing parent
produced an explicit appearance error unrelated to migration. Final acceptance
uses a task-owned `--preferences-dir`; production appearance behavior was not
changed. One initial Python test-discovery invocation used its default pattern
and selected zero tests; the accepted command above uses the repository's
`*_test.py` pattern and runs all 15.

## Limits and cleanup

Stop waits for the current scan/inspection/transaction/file copy. It does not
interrupt decompression or a transaction midway. Repeating reuses existing
snapshots; it does not refresh them with later source messages. A batch checks
one fresh metadata snapshot, not a daemon-wide consistent export. Registration
errors, history failures and incomplete-tail warnings remain separate.

Plans and CLI JSON results can be saved privately. GUI selection/results are
view-local; durable job receipts, overall disk quotas, CLI signal-aware progress/
cancellation, crash-temporary cleanup, later synchronization, other providers,
external image copying and interactive context continuation remain open. This
slice does not run a full migration of the user's real Paseo database or establish
a performance benchmark. Earlier real-source readback is recorded separately.

Only the task-owned apps/hidden workspace are stopped after acceptance; no user
desktop, Paseo daemon or existing provider process is restarted. No cross-platform
native acceptance, hosted CI, commit or push is claimed.
