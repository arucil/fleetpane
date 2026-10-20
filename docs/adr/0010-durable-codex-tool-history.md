# Durable Codex tool history

Accepted 2026-10-19. Supersedes the new-thread history-mode choice in
[ADR 0005](0005-host-backed-conversations.md).

## Problem and evidence

Codex 0.163.0 emits nested code-mode terminal executions as `commandExecution`
items while a turn runs, but legacy history omits them from later full reads.
The loss occurred in the raw provider response, before Fleetpane's projection.
Consequently a successful command card could disappear on Reload conversation or
desktop replacement.

We had selected legacy after an empty paginated thread returned
`list_turns is not supported yet`. That probe tested a thread before Codex had
materialized its history. Repeating it after the first user turn, even a turn
whose local model endpoint rejects it, makes `thread/read` and `thread/turns/list`
work. The empty-thread error does not establish a missing runtime capability.

Installed-binary checks establish behavior. A source cross-check at upstream
revision `f82e36592278a0a782fb0d0fc97e5fa10e832ac2` is consistent with it:
[rollout policy](https://github.com/openai/codex/blob/f82e36592278a0a782fb0d0fc97e5fa10e832ac2/codex-rs/rollout/src/policy.rs)
retains completed command items for paginated storage, while legacy skips that
completed-item representation. The
[history read validator](https://github.com/openai/codex/blob/f82e36592278a0a782fb0d0fc97e5fa10e832ac2/codex-rs/thread-store/src/local/thread_history/read.rs)
returns Unsupported when indexed thread metadata is absent. This source revision
is corroboration, not attestation of the installed binary's exact build.

## Decision

The shared CLI/desktop Create command sends `historyMode: "paginated"` on
`thread/start`. It uses Codex's native disk representation; full reads and
`thread/resume` hydrate the existing typed timeline. Existing identity upserts
continue to reconcile streamed and saved items.

The thread projection carries optional `historyMode` metadata. Missing metadata
and future strings stay readable. An explicitly legacy thread displays a warning
that command history may be unavailable. Existing threads keep their mode; there
is no conversion, command reexecution, synthetic result construction or merge
from the Host's bounded event buffer. Discarded legacy items cannot be recovered
by choosing a different default for new threads.

Creation does not automatically reload an empty thread or retry it as legacy.
The returned empty thread remains usable for its first message. A user-triggered
read before that message still reports the provider error.

## Bounds and remaining work

This initial storage change still used full reads or resumes.
[ADR 0011](0011-incremental-conversation-history.md) subsequently adds metadata-only
native open/resume and bounded turn/item pages. Retained item/byte bounds and the
transport's 256 KiB frame limit remain; arbitrary large-history traversal still
requires byte-aware pages and windowing.
Visible text previews remain capped at 16 KiB with full-item Copy within transport
bounds. This is not an unlimited output archive: the corroborating Codex source
also truncates persisted command output over 64 KiB with a marker; large-output
durability was not accepted in this slice.

Completed history recovery does not restore Host operation UUIDs, ownership,
pending decisions or in-flight processes after a Host failure. The Host journal
remains in memory, and version drain/adoption remain future work. This slice
restarts only a disposable Host/provider after its turn has completed.

## Verification

The [verification record](../verification/2026-10-19-durable-tool-history.md)
separates a deterministic installed-Codex/local-SSE regression from one actual
OpenCodex model turn and native window checks. The regression fails with the
former legacy default; the fix preserves exact completed item identity, output
and exit status after reload and provider replacement without reexecuting the
command. The real turn preserves all three command items across read and resume.
Existing legacy warning rendering uses a synthetic fixture.

This establishes the covered Codex 0.163.0 workflows through OpenCodex 2.73.0.
Other providers/versions, large histories and active-turn Host recovery require
their own acceptance.
