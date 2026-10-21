# Offline Paseo workspace and history import

Accepted 2026-10-21 for the user's explicit workspace **and historical session**
migration requirement. Continuing an old provider context is a separate operation.

Paseo's current workspace registry is `projects/workspaces.json`. Its current
and legacy `agents` JSON records contain session bindings, not transcripts.
Calling its timeline API can load a session. Instead, read the registry and
allowlisted agent identity/title/time/archive metadata from disk. Do not copy
provider configuration, MCP credentials, controller leases or pending requests.
Associate by workspace ID, or by a unique exact cwd for older records; report
unassociated sessions rather than guessing. The original project/workspace IDs,
path, branch, kind and pin/archive metadata remain in the imported record.

Preview runs off the rendering thread. Import revalidates the selected workspace
and its session snapshot; unrelated agents updating in an active Paseo daemon
must not invalidate it. Register an available folder or Git worktree in place,
using the normal directory/repository identity checks. Preserve unavailable
workspace metadata and session bindings with a registration error. SQLite schema
8 commits registration and copied identities together. The deduplication key is
canonical Paseo home + source workspace ID + destination Host socket. Repeating
import returns the existing snapshot; it is not a synchronization service.

History copying is explicit, sequential per workspace, and individually atomic
per session. Resolve Codex's exact provider ID through read-only `state_5.sqlite`;
resolve DeepSeek Harness v3 logs using its UTF-16 path encoding and compressed
filename preference. Stream all concatenated Zstandard frames. Validate the
header's provider session ID and cwd before publishing. Unsupported providers,
missing bindings/logs, corrupt records and bounds failures stay explicit.

Copy a fixed prefix of the source file. Keep every complete JSON record in a
private per-session SQLite archive, including records not projected to cards;
`source-record` reads an exact numbered record. Project user/assistant messages,
reasoning, tool calls/results and supplied images to existing content types.
Historical tool cards do not infer a running state or actionable approval.
A final non-newline-terminated record is excluded with a warning. Corrupt JSON
or a torn compressed frame refuses publication. Temporary file + fsync + atomic
non-overwriting hard-link publication keeps concurrent/repeated copies stable.
Snapshots have private permissions and remain readable after source logs vanish.

The engine and desktop share the same operations. Session lists use 32-record
pages; history pages use 32 items / 256 KiB, except one item up to 4 MiB. The UI
replaces each page instead of accumulating an unbounded timeline. Source limits
are 20,000 workspaces/agents/directories, 16 MiB per metadata file, 128 MiB total
metadata, 64 MiB per JSONL record and 1 GiB encoded/decoded log. Both original
records and projections occupy storage; these are input bounds, not disk quotas.

This slice is a local archive import. It does not move working directories,
create branches, copy external image files, import project preferences/secrets,
restore Fleetpane pin order, synchronize later Paseo changes, import other
provider transcripts, transfer provider processes or resume contexts. External
image paths can become unavailable even though the original record is retained.
The old process keeps ownership; importing historical questions never answers
or replays them. A future explicit resume flow must validate provider and
directory identity and coordinate ownership before making an imported context
interactive. The [bulk workflow](0060-bulk-paseo-workspace-history-migration.md) reuses the same
per-workspace operations with selection, progress, cooperative Stop and repeat execution.

Reference inspected: local Paseo fork `a486b0558168c08890890e787d55cfb88c0d7256`
plus its working tree: `workspace-registry.ts`, `agent/agent-storage.ts`,
`agent/providers/dsh-session-history.ts`, and installed Codex/DSH storage headers.
Fleetpane uses an independent implementation; no Paseo source/assets were copied.
