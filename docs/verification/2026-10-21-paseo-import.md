# Paseo workspace and historical session import — 2026-10-21

The desktop now exposes **Workspaces → Import from Paseo**. Preview a source,
import one workspace and its historical session identities, then explicitly copy
that workspace's Codex/DeepSeek transcripts. Existing folders and Git worktrees
are registered in place. **Sessions → View history** reads a private snapshot,
independent of both daemons. The same operations are available from the CLI.
This verifies offline archive import; interactive continuation and live process
handoff remain open. [Decision and bounds](../adr/0059-offline-paseo-workspace-history-import.md).

The [receipt](assets/paseo-import/receipt.json) records final source/binary hashes,
commands and evidence files. All public fixtures/screenshots are synthetic.
Private real transcripts are excluded from repository artifacts.

## Automated verification

- `cargo test --workspace --locked`: 197 tests pass, including 126 session tests.
- `cargo test -p fleetpane-session paseo_import -- --nocapture`: 10 targeted tests
  pass. Folder and real linked-Git registration, unchanged untracked bytes/branch,
  atomic rollback on an injected storage error, concurrent import deduplication,
  71-session paging, Unicode, orphan reporting, source change rejection, unrelated
  source updates, missing directories, duplicate/malformed/symlink refusal,
  private snapshot permissions, raw-record retention, source disappearance,
  header mismatch, corrupt record cleanup and incomplete final-line warning.
  Concatenated Zstandard frames use synthetic DSH v3 records.
- `cargo test --manifest-path apps/desktop/Cargo.toml --locked`: 19 tests pass.
- Root and desktop strict Clippy (`--all-targets --locked -- -D warnings`), both
  formatters, desktop/Host builds and `git diff --check` pass.
- `python3 scripts/paseo-import-smoke.py`: full offline CLI preview → import →
  workspace history copy → two pages → exact source-record read → source log
  removal → successful snapshot reread. Codex and DSH succeed, unsupported Claude
  is explicitly reported. Original source files hash identically after copying;
  no Host socket/provider process is needed.
- Appearance and recent-directory smoke tests pass on controller schema 8;
  their historical-schema fixtures now remove the newly introduced tables when
  constructing an older database. Earlier draft/catalog preservation assertions
  remain in place. Python script tests: 15 pass.

Logs and JSON results are alongside the receipt. Early full-suite failures came
from old-schema fixtures retaining schema-8 tables while resetting user_version;
the fixtures were corrected without weakening production migration validation.
Desktop Clippy identified an oversized reply variant; its imported record now
uses indirection. No warnings were suppressed.

## Native workflow

Hidden workspace `fp-paseo-import`, display 1500 × 1100, app 1100 × 720. Host
endpoint intentionally absent. No model calls or existing provider takeover.

The first desktop previewed one Unicode-named workspace and three sessions,
imported it, copied both supported transcripts, and displayed one explicit
unsupported-provider failure. Opening the imported Codex archive showed the
Chinese question, Markdown heading/emphasis/table/code, independent tool
call/result cards and supplied data-URL image. Tool expansion/copy read back the
exact tool-call ID and `MIGRATED_TOOL_OUTPUT\n中文 output` through the hidden
display's clipboard. The initial fixture image had invalid PNG data and showed
the expected unavailable-image message; replacing the synthetic fixture with a
CRC-correct generated PNG, then rebuilding its task-owned archive, rendered it.
No production image workaround was added.

Closing/reopening the desktop retained workspace/session identities and both
archives. The persisted DSH archive rendered its Chinese question and Markdown
answer without recopying. The final desktop binary was rebuilt and relaunched
with matching on-disk/process executable SHA-256. It again read the existing
Codex archive, rendered tools/image and produced the same exact clipboard JSON.
**Next page** moved from items 1–32 to items 33–45 with **End of history**;
**First page** restored the earlier content. Only the current page is retained.

Synthetic screenshots and clipboard payload are in `assets/paseo-import`.
Earlier preview/report/DSH screenshots came from the first successfully built
binary; `accepted-*` screenshots and the clipboard repeat use the final binary.
One final-build invocation ended with SIGTERM before completion; a subsequent
normal build succeeded and its log/hash are the accepted artifact.

## Real local readback

Read-only discovery recognized **2,840 workspaces / 2,807 associated sessions /
0 unassociated sessions** in the user's local Paseo data. These are a point-in-time
inventory, not a full migration or performance benchmark.

One real Codex and one real DeepSeek history were copied into a private temporary
controller store and reread through the CLI:

| Provider | Preserved source records | Projected items | First page |
| --- | ---: | ---: | ---: |
| Codex | 2,518 | 1,056 | 12 items within the page byte budget |
| DeepSeek Harness | 18 | 5 | 5 items |

Both snapshots had no incomplete-tail warning. Private evidence remains at
`/tmp/fp-import-real-7_5e5m3n`; only aggregate counts are copied into repository
evidence. Real transcript native rendering, continuation and external model
execution were not exercised in this slice.

The first real import attempt demonstrated that unrelated live Paseo updates
invalidate a global catalog hash too frequently. Revalidation now checks the
selected workspace/session projection. A regression proves unrelated agent
changes cannot invalidate it, while the selected workspace changing still fails.
No retry or relaxed selected-workspace check was introduced.

## Limits and cleanup

This adds a browsable local archive, not active context adoption. Provider resume,
transfer of running tasks, other-provider transcripts, workspace bulk selection,
later source synchronization, pin-order restoration, directory relocation and
external image-file copying remain open. Archives keep complete source records,
including unprojected extensions, but supplied external paths can later become
unavailable. Codex requires the inspected `state_5.sqlite` index; DSH requires v3
history. Damaged/torn compressed logs refuse publication. Archive input bounds
are explicit; copied originals plus projected data have no total disk quota yet.

Only task-owned desktops and the hidden workspace are stopped after acceptance.
The existing native close issue remains: windows disappear, but processes can
linger until isolated workspace cleanup. Graceful exit is not newly accepted.
No existing Paseo daemon, provider session or user's desktop was restarted.
No hosted CI, cross-platform native acceptance, commit or publication is claimed.
