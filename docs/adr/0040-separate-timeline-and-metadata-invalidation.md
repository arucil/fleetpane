# ADR 0040: Separate timeline and metadata invalidation

Accepted 2026-10-21; local verification recorded separately.

Paseo replay 93–100 moves input and status controls into a shared footer, while
101 adds memoized messages and defers heavy agent content by a frame. Fleetpane
already retains card entities and uses a virtual list, but its poll loop used one
changed flag for directory updates, selected metadata and conversation content.
Any of these changes called rebuild_rows, which decoded, serialized and compared
every loaded item before deciding whether an existing card needed updating.

Track selected conversation invalidation separately from chrome/directory changes.
Only events actually applied to the selected session can invalidate its rows.
Known name, status, ACP session info, config, modes and task-plan metadata change
no Row::turn or Row::item inputs. They update the surrounding UI without rebuilding
the timeline. New event kinds conservatively invalidate when the session decoder
reports them applied, so adding a content decoder cannot silently leave stale rows.
Projection errors retain the previous conservative rebuild-and-pause behavior.

Pending requests, ownership and fault changes still notify the view. Reconcile
the permission preview even when rows are unchanged: a new standalone request
or its resolution can require a different preview without a conversation item
event. Existing question synchronization stays independent of timeline rows.
History reads, session switches, new content, deltas and turn settlement keep
their established row reconciliation and virtual-list splice behavior.

The regression applies real shared-session metadata updates, verifies that their
Row inputs are unchanged, and checks that they do not invalidate the timeline.
It fails under the previous applied-event invalidation rule. Content and foreign
event cases guard against stale content or cross-session invalidation. Native
acceptance checks large loaded history, metadata churn, exact drafts and sequential
standalone permission previews.

This removes a specific redundant operation; it does not establish a latency or
throughput improvement. Content events still construct all loaded rows; incremental
content projection, history eviction and full concurrency budgets remain open.
No frame delay, animation timer, voice pipeline or global footer is introduced.

[Verification](../verification/2026-10-21-timeline-invalidation.md).
