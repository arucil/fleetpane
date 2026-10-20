# 0014: Independent named local Hosts in one native workbench

Status: implemented, 2026-10-20; local acceptance is scoped in the
[verification record](../verification/2026-10-20-multi-host.md).

Later local slices remove the lifetime mutation cap with durable receipts
([ADR 0015](0015-durable-operation-receipts.md)), add persistent admission drain
([ADR 0016](0016-persistent-host-admission-drain.md)), and extend configuration with
optional explicit selection groups ([ADR 0017](0017-explicit-ready-host-selection.md)).
The original scope/limits below describe the initial workbench slice.

## Context

The desktop previously accepted one `--host` socket and created one `HostView`.
Using OpenCodex and DeepSeek therefore required separate windows, and there was
no shared configuration/probe for multiple independent runtimes. A provider's
thread, turn, request or pagination IDs are local to its Host; two runtimes may
emit identical values. Flattening these into a global session map would route
drafts, history or permission answers incorrectly.

## Decision

Add a version-1 JSON configuration containing 1–8 named local Unix endpoints.
The toolkit-independent `fleetpane-session::hosts` module validates names,
absolute endpoint paths, duplicate names/paths, unknown fields, version and the
16 KiB file budget. Names are labels, never provider or runtime identities.
Configuration contains no commands, credentials, account settings or provider
launch policy. Socket symlink aliases are not resolved; configure each runtime
once. Explicit controller fencing still applies if clients reach the same Host.

`--hosts FILE` creates a native workbench with a separate retained `HostView` per
endpoint. Only the first Host is initialized at startup; other connections start
when selected. First connection reads a snapshot and does not take over an
existing controller. Each Host retains its own directory, selected thread, model
options, drafts, history traversal, event cursor, lease, pending operation and
approval/question forms. Selecting a tab initializes its view once and changes
focus to its composer, preventing a hidden input from receiving new keystrokes.
It never claims control, resumes a thread or resubmits a command.

Opened views continue their existing bounded polls when hidden. The parent
observes each view so a background permission request or error updates its tab.
Only the selected view renders. Per-Host I/O runs independently on background
tasks with the existing deadlines; one failed endpoint does not gate other Host
views. Host/provider processes remain independently supervised outside the
desktop, including when the complete window exits.

CLI `fleetpane-host hosts FILE` uses the same validated configuration. It probes
all configured endpoints concurrently, reports snapshots/errors in configuration
order and never claims ownership. A successful report can contain unavailable
endpoints; its exit status means the report was produced, not that all providers
are healthy. The native workbench does not wait for this aggregate report.

The original `--host SOCKET` uses the same workbench with one slot and no tab
bar. `--demo`, `--host` and `--hosts` are mutually exclusive; repeated config flags
fail instead of silently replacing an endpoint. Configuration is read before
GPUI startup; no filesystem reads occur during rendering.

## Limits

This is endpoint selection, not a control daemon, runtime migration, routing CAS,
automatic drain or failover. It does not adopt or start existing provider
processes. A missing endpoint stays visibly unavailable until an explicit
reconnect. Account/configuration and provider storage isolation remain Host
launch responsibilities. Runtime incarnation/lease checks remain authoritative;
labels are not trusted as identity. Different labels that resolve to the same
Host are not independent runtimes.

Configuration is fixed for the window's lifetime. Drafts and layouts remain
memory-only, host lists are not automatically saved, and there is no aggregate
cross-Host search. Eight retained views can each reach existing cache/directory
bounds; there is no global memory allocator or measured eight-Host budget.
The per-Host 128-mutation lifetime journal, ACP resident-session cap and recovery
limits remain. Remote transports, durable routing and complete project/workspace
tooling are separate entries in the [parity inventory](../parity.md).
