# Explicit ready-Host selection for new sessions

Status: implemented for local recommendations, 2026-10-20.

Persistent admission drain prevents new work on old Hosts, but the workbench
previously required inspecting individual tabs to locate a ready endpoint.
Endpoint names are display labels and do not establish equivalent providers,
accounts or credentials. Selecting any arbitrary ready endpoint could change
which harness/account runs the user's task.

Version-1 Host configuration now accepts optional `routes`: up to eight uniquely
named groups, each referencing 1–8 existing Host names in priority order. Unknown
members, duplicate names/members, unknown fields and invalid text fail validation.
The existing eight-Host and 16 KiB file budgets still apply. Omitted routes retain
manual selection. This extension is backward compatible for new readers of old
files; older readers reject `routes` rather than ignoring it. Group membership is
an explicit launch/configuration choice, not verified account equivalence.

The shared session engine probes only the selected group's endpoints in parallel,
using read-only snapshots. It chooses the first ready, non-faulted Host in group
order, regardless of reply timing or tab order. Draining, unavailable and faulted
members remain visible with their reason/activity. No member ready produces no
target; it never searches another group. Duplicate live Host incarnation IDs
within a group reject selection, including configured socket aliases. There is
no load-balancing heuristic, implicit account discovery or provider-label guess.

A recommendation contains the endpoint, Host incarnation UUID and admission
generation. A fresh check before preparing the workspace rejects changed
incarnation/generation, drain or provider fault. It neither claims ownership nor
reserves capacity. The normal Host lease/admission gate remains authoritative
when a later mutation actually arrives. A drain concurrent with submission is
rejected there; an already admitted operation continues under ADR 0016. There is
no cross-endpoint retry after rejection, transport loss or an uncertain result.

CLI `fleetpane-host route CONFIG GROUP` reports this read-only selection. No ready
target returns the full report and a nonzero exit status. The native **New via
group** page uses the same engine, shows candidate states and **Prepare on NAME**.
Preparing selects the existing retained Host view and fills its workspace field.
The user then uses that Host's ordinary **New session** command and explicit
**Take control** if another controller exists. The retained view also checks the
recommended identity/generation before creation using its latest snapshot.
Creation therefore keeps the existing durable UUID/pending-operation behavior;
the recommendation layer never submits a provider mutation.

The chooser replaces the central content rather than shrinking conversation and
approval areas. Opened Host views continue polling and retain independent drafts,
leases and pending questions. Opening the chooser focuses its directory input;
returning to a tab focuses that tab's composer. Input/group changes clear stale
recommendations, controls cannot start concurrent selections, and late selection
results do not switch away after the user returns to another tab. The directory
probe is a bounded background job, not rendering-thread I/O or a continuous scan.

No new Host protocol or journal schema is introduced (still protocol 3/schema 2).
Group membership persists only in the supplied configuration file, reread on
window startup; selection results, prepared identity guards and drafts are
memory-only. There is no persistent session ownership/routing table, Host launch,
automatic routing, migration, provider restoration, supervisor or retirement.
Full version drain remains open under ADR 0002. See
[verification](../verification/2026-10-20-host-selection.md).

ADR 0018 subsequently persists initial creation bindings and permits at most one
dispatch attempt per saved intent. Recommendations remain read-only and do not
provide automatic failover; see [that decision](0018-durable-local-creation-intents.md).
