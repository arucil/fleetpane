# Bulk Paseo migration

Accepted 2026-10-21. Extends [offline import](0059-offline-paseo-workspace-history-import.md)
with explicit selection, progress, cancellation and repeatable batch execution.
Workspace and transcript guarantees from ADR 0059 still apply.

`BatchPlan` contains canonical source home, selected workspace IDs and preview
revisions, target endpoint and provider history roots. The desktop builds it
from a preview. CLI `paseo-import plan` emits the same version-pinned selection
for review; `batch STORE PLAN_JSON` reads and executes it. Plans reject empty,
duplicate and unknown selections, malformed revisions and relative paths. They
contain paths and identities; no provider credentials or transcript contents.

Execution discovers the source once, checks each selected revision against that
fresh snapshot, and processes workspaces sequentially. This avoids a full source
scan for every selected workspace. A disappeared/changed selection has its own
failure result; other valid selections can continue. Workspace registration and
session identity insertion use the existing per-workspace transaction; each
history uses its existing atomic publication. There is no batch-wide transaction
or promise that an actively changing source is one point-in-time export.

Result records distinguish new and reused workspace imports, registration errors,
history availability/failures/warnings, and incomplete history traversal. A
missing directory can retain imported identities while reporting its registration
error. Unsupported/missing/corrupt logs remain failures, never successful empty
histories. Available-history counts include existing snapshots. Repeating a plan
reuses the original identities and archives and tries the missing histories; it
does not refresh old metadata or append to a saved archive. Selected metadata
changes still require a fresh preview. Original source records are never changed.

`BatchControl` shares one progress slot and an explicit cancellation flag. Stop
finishes the current inspection/transaction/file copy and checks the flag before
starting the next unit. Published archives are complete; already saved workspaces
remain available. The desktop disables navigation/close while the job settles,
polls that slot while running and retains only one polling task. It does not
enqueue an event per history row. The initial source scan and a single large
file copy are not interrupted in their interiors, so Stop can take time.

The desktop filters by workspace title/path, preserves selection across filters,
offers Select matching/Clear and an explicit Migrate selected + history button.
Candidate and result lists are virtualized. Per-workspace Details exposes the
registration error and individual history failures/warnings. Batch reports and
selection live in the current view; durable workspace identities and snapshots
survive restart. CLI callers can save the plan and JSON result. Durable job
receipts, persisted GUI selection, CLI signal-aware progress/cancellation,
overall disk quotas, later synchronization and context continuation remain open.
Abrupt process death is distinct from cooperative Stop; this slice does not
claim cleanup of abandoned private temporary archive files after a crash.

Acceptance is recorded in [verification](../verification/2026-10-21-paseo-batch.md).
