# Independent continuation from saved Codex histories

Accepted 2026-10-21. Extends [offline import](0059-offline-paseo-workspace-history-import.md)
and [bulk migration](0060-bulk-paseo-workspace-history-migration.md).

An imported transcript and a live provider context are separate objects. For a
complete legacy Codex rollout, the provider's `thread/fork` RPC can construct a
new context from an explicit rollout path. Fleetpane exports the raw records of
its private SQLite archive to a private JSONL file; it never resumes the source
thread or sends the rendered transcript as a new prompt. Unknown raw records
are preserved. Existing exports must have identical bytes. Temporary publication
uses a new 0600 file, fsync, hard-link creation and directory fsync; an existing
target is not replaced. Provider-owned files are never written.

Preparation requires a real source thread UUID, matching source/header identity
and directory, no incomplete-tail warning, and an absent or `legacy` history
mode. Codex paginated histories also require their separate thread store and are
explicitly rejected. Null, malformed and unknown history modes are rejected.
The imported workspace must remain registered with the same directory/repository
identity. A ready Host must explicitly identify itself as Codex. Older Hosts
without that marker require an explicit upgrade; an ACP Host is not a substitute.

Controller schema 9 adds `paseo_continuations`, keyed by imported session UUID.
Preparation saves one operation UUID, source thread, workspace, export path,
canonical cwd, endpoint, Host incarnation and admission generation. Concurrent
preparations converge on the first committed record. Existing bindings are
returned before probing the Host, including offline/restarted targets. They do
not silently change endpoint, Host or generation. No preparation claims control.
Explicit replacement of an unattempted preparation is described in
[ADR 0062](0062-reprepare-unsent-codex-context.md).

Dispatch needs explicit controller ownership, a fresh target check after export
validation, and an atomic transition of `attempted` from false to true before
submission. It sends `excludeTurns: true` for a bounded reply and
`deferGoalContinuation: true` to prevent an inherited goal from running on fork.
The successful reply must contain a different nonempty thread ID, the selected
cwd, idle status and no hydrated turns. Desktop hydration then uses the existing
paged/legacy read flow and a new thread-scoped draft. Starting a subsequent turn
is a separate action. Original source execution remains independent.

Every later dispatch is refused, including after a provider error or uncertain
transport result. Read-only status checks verify operation UUID, method and
params against the saved binding. Prepared, pending, created, provider failure
and unknown outcomes remain distinct. Controller loss after admission can recover
the original thread ID; Host restart during a pending operation reports unknown
without replay. A controller crash between persisting `attempted` and admission
can also remain unresolved; no automatic resend is inferred from a missing receipt.

GUI **Continue in Codex…** prepares the binding; the target view exposes **Fork
saved context**, **Check saved fork status** and **Open forked session**. CLI
`prepare-fork`, `fork ... --take-control` and `fork-status` share the engine.
Reopening the imported session restores the binding. Sidebar controls scroll
within a viewport-based bound to keep the session directory accessible.

This slice does not transfer processes, active tools or pending permissions.
DeepSeek ACP has no independent fork contract here and remains transcript-only.
Paginated Codex context copying, refreshing imported archives, repeated independent
forks from the same imported identity, cross-provider conversion and a full disk quota remain open. The source snapshot
may contain an interrupted tool exchange; the provider may reject its context.
External images retain the existing import limits. No user real-session migration
or replacement-readiness claim follows from the isolated acceptance.
