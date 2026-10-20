# 0023: Bounded provider-reported task plans

Accepted 2026-10-20.

## Problem and references

Agent task progress previously had no native panel. Codex emits
`turn/plan/updated`, separately from its Markdown plan-mode proposal items.
Standard ACP emits `session/update` with `sessionUpdate: plan` and a complete
replacement list. See the [ACP plan contract](https://agentclientprotocol.com/protocol/v1/agent-plan).
Paseo's inspected Codex/ACP adapters at
`a486b0558168c08890890e787d55cfb88c0d7256` follow those two notification forms.
No reference code or assets are copied.

A real DSH 0.2.0-rc.2 check reproduced a further compatibility gap: successful
`todo_write` calls appear as generic ACP tool events, without a standard plan
notification. Its installed ACP bridge sets `title` to the tool name, preserves
parsed arguments in `rawInput`, and reports success as `status: completed`.
The installed todo tool defines a complete replacement `todos` list with
`content` and `status`, without priority.

## Decision

- Put the task model and parsing in the toolkit-independent runtime crate. Keep
  thread/turn identities, explanation, content, reported status and optional
  priority. Normalize Codex `inProgress` to `in_progress`; preserve future
  nonempty status/priority strings and display unknown states explicitly.
- Standard ACP entries require their protocol priority. For DSH's exact
  `todo_write` tool, parse its retained input only on transition to successful
  completion, with absent priority preserved as absent. Pending or failed calls
  do not update tasks. Later content updates of an already completed tool do not
  replay its old task list. The original tool card remains available.
- Each update replaces the entire list, including an explicit empty list. Do not
  infer task completion from a completed, cancelled or failed turn. Show a
  previous-turn label when another turn is active without a new task report.
  Markdown plan-mode proposal items keep their existing independent renderer.
- Bound a plan to 128 entries and 32 KiB serialized UTF-8, each step to 4 KiB and
  status/priority to 128 bytes. IDs are nonempty and at most 4 KiB. Reject malformed
  known reports through the existing provider boundary error path.
- Retain the latest list for each of at most 64 recently updated sessions,
  additionally capped at 1 MiB serialized plan data, evicting oldest updates.
  This Host-owned cache survives event-ring expiry and controller replacement.
  It is not a durable transcript and does not survive Host restart.
- Add optional protocol-v3 `fleetpane/tasks/read` as a local, controller-fenced
  observational query. It takes only `threadId`; strict parsing rejects other
  keys. It neither contacts the provider nor consumes a query slot, operation
  UUID, journal entry or event sequence. Reads remain available while draining
  or after provider fault. CLI `session SOCKET tasks THREAD` uses the established
  lease without claiming ownership; a Host with no owner explicitly refuses.
- Include the Host read boundary sequence in the view. A thread accepts only
  newer live reports and never lets an older replay overwrite a later query.
  Native reads run off the UI thread and reject wrong-Host/thread/stale results.
  The GPUI Kit panel exposes refresh, copy, collapse, reported completion count,
  explanation and a scroll area limited to 12 rem.

## Limits and consequences

Cache misses cannot distinguish never-reported, evicted and earlier-incarnation
plans; the UI says no plan was reported or retained. It does not claim all tasks
are complete or reconstruct plans from text or old tool cards. Historical task
replay, editable checklists and provider-independent plan-mode controls remain
separate work. Earlier Hosts can reject the new query; the panel exposes that
error without discarding the conversation.

The real DSH compatibility case and synthetic/native coverage are recorded in
[local verification](../verification/2026-10-20-agent-tasks.md). Real OpenCodex
turns that produced no plan-tool call are not counted as task-plan acceptance.
