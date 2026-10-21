# 0077: Order loaded session metadata across Hosts

Date: 2026-10-22. Status: accepted.

Paseo ordinal 222 flattens its home list across Hosts and orders running sessions
before recent activity. Fleetpane previously had only Host groups, making users
scan each Host separately. Add Activity and By Host presentation modes, defaulting
to Activity, without changing queries or authority.

The shared engine builds source-indexed rows from already loaded metadata. Fresh
rows precede stale observations, active rows precede other states, supplied update
times descend, and endpoint/session IDs break ties deterministically. Codex
updatedAt is Unix seconds. ACP updatedAt remains an opaque stored string, parsed
as RFC3339 only for this view; offsets and subseconds compare as UTC instants.
Missing or invalid timestamps stay explicit and have no invented recency.

Cache this projection when metadata, filters or sort change. Rendering reads the
cache and virtualizes rows; it does not parse timestamps, sort or perform I/O.
Each row preserves its original Group and session indices. Source opening still
checks the observed Host incarnation and archive membership. Sorting never
claims a lease, resumes, sends, answers or interrupts. Failed observations are
labelled Last observed and cannot be opened. Host summaries retain independent
paging, empty and unavailable states; initial reads say they are loading.

Only loaded rows are ordered. The existing 100-row queries and 5,000-row per-Host
cap do not establish a global full-index or realtime order. No last-user-message
time is inferred from update time, no Android/store-library port is required,
and this preference is currently window-local.

Verification: [activity ordering](../verification/2026-10-22-activity-order.md).
