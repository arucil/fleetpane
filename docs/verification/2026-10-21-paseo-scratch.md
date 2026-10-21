# Interrupted Paseo copy cleanup — 2026-10-21

The engine, CLI and Linux native import page provide explicit inventory and
collection of versioned temporary migration files. Current writers and collectors
exclude each other through a shared/exclusive process lock. Older temporary files
are reported and retained. Source transcripts, published histories, exports and
saved batch reports stay outside deletion. See [ADR 0065](../adr/0065-explicit-paseo-scratch-collection.md).

[Evidence receipt](assets/paseo-scratch/receipt.json) records source/binary hashes,
commands and artifacts. All fixtures and native screenshots in this slice are
synthetic. No user migration or provider runtime is modified.

## Reproduction and regression

Before the change, killing a copy and retrying left its temporary SQLite database
and journal in `paseo-history`. The reproduction retained two files (16,384 and
4,616 bytes); a successful retry did not reclaim them. This is a crash-lifetime
gap, not a corrupt visible history. Existing older files still cannot safely be
collected because their writers never acquired the new ownership lock.

New copies/exports write under `paseo-scratch-v1` with a shared lock held inside
the blocking operation. Collection requires an exclusive lock. Whole-directory
preflight precedes unlinking; malformed/unknown entries fail explicitly. A
cancelled async awaiter cannot release a still-running blocking writer's guard.
The lock inode remains stable and a published hardlink survives scratch unlink.

## Automated verification

- `cargo test -p fleetpane-session paseo_import::scratch --locked`: **5 pass**.
  Shared writers, inverse collection/writer exclusion, lock/directory validation,
  unknown/noncanonical names, public files, symlinks, nested paths, whole-preflight
  preservation, legacy retention, published hardlink preservation, repeated empty
  cleanup and cancelled-awaiter ownership are covered.
- `cargo test -p fleetpane-session paseo_import --locked`: **36 pass**.
  This retains existing import, archive, fork/export, cancellation, receipt and
  revision-validation behavior alongside the five new tests.
- `cargo test --workspace --locked`: **223 pass** (4 client, 4 protocol,
  63 runtime, 152 session). Desktop manifest tests: **19 pass**.
- Workspace and desktop Clippy with `--all-targets --locked -- -D warnings`,
  both format checks, both binary builds and `git diff --check` pass locally.
- `python3 scripts/paseo-scratch-smoke.py`: waits for an actual temporary file
  while the 60,000-row history is being copied, pauses that process with SIGSTOP,
  and confirms both inventory and collection refuse its live lock. SIGKILL
  releases ownership; explicit batch retry publishes four histories with one
  unsupported-provider failure and keeps the original report unchanged. Preview
  reports two abandoned files / **16,904 logical bytes**. An external shared lock
  refuses cleanup; an unexpected filename fails before deleting any valid file.
  Explicit collection removes the two files, a second collection reports zero,
  and two old-format temporary files remain. Source/published hashes match and
  all four histories remain readable. No Host or provider starts.
- Single-workspace, batch, DeepSeek v4 and synthetic independent Codex fork CLI
  smoke commands pass after the scratch path change. The v4 check uses the
  synthetic codec and fork uses the protocol fixture; these are not new real
  model checks. Python script regressions: **15 pass** with
  `python3 -m unittest discover -s scripts/tests -p '*_test.py'`.

The new process smoke command is configured for non-Windows CI. Hosted CI has
not run for this working tree. The initial Python discovery command used the
default filename pattern and found zero tests; the corrected pattern above ran
all 15. No zero-test run is counted as validation.

## Linux native acceptance

Hidden workspace `fp-paseo-scratch`, 1500 × 1100 display, 1100 × 720 application
window. The synthetic destination Host is offline throughout. The running
desktop `/proc/PID/exe` hash matches the tested binary.

**Check temporary files** shows two files / 16,904 bytes / one older retained
file, then enables **Clear unused temporary files**. With an external shared lock
held, Clear reports the engine's active-copy error and all scratch hashes remain
unchanged. After releasing the lock, Clear removes the two files and disables
itself. Independent CLI readback confirms zero scratch files, unchanged source,
published and legacy hashes, unchanged batch summaries and readable history row
counts of **2 / 60,000 / 2 / 45**.

After application replacement, Check again reports zero files and one retained
legacy file; saved workspaces/reports and native history remain available.
Preview state itself is view-local. Closing the first window did not exit its
process within three seconds; the workspace's task-owned app was explicitly
terminated before relaunch. This known lifecycle gap is not a clean-exit claim.

Screenshots retain [preview](assets/paseo-scratch/native-preview.png),
[active-writer refusal](assets/paseo-scratch/native-active-refusal.png),
[cleanup result](assets/paseo-scratch/native-cleared.png),
[reopened empty inventory](assets/paseo-scratch/native-reopen-empty.png) and
[retained native history](assets/paseo-scratch/native-history-after-cleanup.png).

## Remaining scope

Cleanup is explicit and covers only the new versioned scratch directory. Legacy
collection, automatic startup/periodic cleanup and archive-wide quotas remain
open. No live session transfer, transcript synchronization, new real model call,
macOS/Windows native acceptance, power-loss simulation, fault-injected partial
unlink or throughput benchmark is claimed. Unknown-file preflight and normal
filesystem cleanup are covered; arbitrary hostile same-user filesystem changes
are outside the lock-cooperating writer contract.

Only task-owned apps and the hidden workspace are stopped after acceptance. No
user desktop, existing Paseo daemon or provider is restarted. No commit or push
is made by this slice.
