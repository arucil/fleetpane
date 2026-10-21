# 0055 — Browse and restore provider archives

Status: accepted for the local Codex prototype, 2026-10-21.

## Context

AG-01 and WS-05 require archive and restoration. The inspected Codex source makes
`thread/archive` shut down the root and spawned descendants; ADR 0009 requires tree impact and
explicit-stop acceptance before exposing that mutation. Restoring an existing
provider archive is a separate required path. Codex 0.163.0's generated protocol
supports an `archived` directory filter and `thread/unarchive` returning the
specified thread. It does not resume a context, send a turn or restore descendants.

Paseo history 169 proposes registry-only deletion; 173 actually calls close first
and emits deletion even after close/registry errors. Neither behavior establishes
safe archive or successful durable deletion. We do not port that error handling.

## Decision

The shared directory query includes archive membership. Title/cwd filters,
continuation cursors, retention bounds and query equality include this scope.
Normal and archived lists remain distinct provider queries, not filters over a
partially loaded list. DeepSeek ACP does not advertise provider archive listing;
its adapter refuses that request explicitly. Its observed read-only history and
context-resume path retain their existing meaning.

The desktop exposes Sessions and Archived. Opening an archived row reads metadata
and paginated history without resuming. Selected archive membership is a local
observation from the directory, independent of the provider's loaded/idle status.
While that observation applies, sending and resuming that selection are blocked.
Other session navigation remains available. Refresh is needed after external
membership changes; a retained observation is not a provider transaction lock.

Restore to sessions uses typed `Command::Restore`, the original thread ID, current
Host lease, durable operation UUID and existing uncertainty handling. It is a
mutation, unavailable while the Host drains. A successful response must contain
the original thread identity and a valid projection. Only then is archive
membership cleared and the directory refreshed. Failures preserve the selected
history and draft. No command is automatically retried.

Restoration keeps the loaded conversation and revisioned draft. It does not adopt
an empty metadata response as replacement history. For an unloaded Codex context,
Resume context is a separate explicit action; Send remains a separate action.
The CLI exposes `list --archived` and `restore THREAD` through the same commands.

## Limits

This completes covered browsing/restoration behavior, not safe archive mutation,
deletion, descendant restoration, workspace/worktree retirement or Host recovery.
Directory scope and selected archive observation are window state. Native evidence
uses synthetic provider data; installed Codex has separate protocol/process
acceptance against an empty home and loopback rejecting model service. No external
LLM, account, remote transport, platform-wide UI or performance acceptance follows.

See [verification](../verification/2026-10-21-archive-restoration.md).
