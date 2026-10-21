# 0079: Project only changed conversation items during polling

Status: accepted for the native desktop slice, 2026-10-22.

Paseo's December state-store changes separate drafts, lightweight directories and
stream data. Fleetpane already retains independent card entities and skips row
construction for metadata-only polls. Its content-event polls still constructed
every loaded row, including Markdown inputs and serialized Copy payloads, before
comparing the results. This was directly visible in `HostView::poll` and
`Row::conversation`; no latency measurement is needed to establish that work.

After the selected-session decoder accepts an event, collect its exact turn/item
tuple. Deduplicate targets within the poll and project each target from the final
thread state. An index of tagged row identities maps existing items to retained
cards. Update only changed cards and invalidate their measured list heights.
Keep draft, focus and expansion entities intact.

Structural events, unknown accepted content methods, missing targets and changes
between visible and hidden content require full reconciliation. History pages,
explicit reloads and session switches also rebuild the identity index. Metadata,
foreign-session events and events rejected by history watermarks cannot project
content. Permission previews continue to reconcile independently.

The implementation preserves the shared content decoder and exact raw Copy data;
it introduces no alternate Markdown/parser semantics or provider-ID normalization.
An opaque provider ID may contain separators. Existing tagged tuple identities
remain authoritative.

This reduces row decoding and serialization, not all work proportional to loaded
history. Source-item lookup still scans the bounded thread, domain event
validation still runs and structural changes still construct all loaded rows.
There is no history eviction or measured whole-application speedup claim.
The [verification](../verification/2026-10-22-sparse-projection.md) records unit,
process and isolated Linux native evidence separately.
