# ADR 0075 — Observational Host diagnostics

Date: 2026-10-22. Status: accepted for the local Unix prototype.

Paseo's historical per-Host Test action checks a specified endpoint independently
of device settings. Fleetpane already has raw CLI snapshots, but no native page
for checking all configured endpoints without opening conversation views.

Add a toolkit-independent diagnostic projection in `fleetpane-session`, shared
by `host-diagnostics CONFIG [HOST_NAME]` and native **Hosts**. Each endpoint gets
one bounded Snapshot RPC using the existing transport deadline and frame cap.
Checks run concurrently (configuration limit: eight), return configuration order,
and preserve endpoint-specific errors. There is no claim, provider query,
conversation resume, mutation, implicit restart or automatic retry.

A reachable socket does not establish protocol compatibility, readiness or safe
retirement. Display incompatible protocol before other states; report provider,
Host or unreported-scope faults separately from draining admission. Reachable
remains an observation. Pending/active/in-flight/unknown/terminal/Stop counts
are reported without inferring missing resources or permission to shut down.
An older Host without Stop progress remains explicitly unreported.

Reports include the check start time, elapsed monotonic duration, local endpoint,
Host/controller/process identities, admission generation, event sequence and
retained operation count. They exclude provider initialization metadata, prompt
and request payloads, session/turn IDs and individual operation IDs. Fault and
transport messages are preserved; copied JSON is not a universal secret scrubber.
Copy is explicit and may contain local paths, process identities and error text.
The existing raw `hosts CONFIG` command retains its earlier behavior.

The native page checks on opening and explicit single/all refresh. While a check
runs, retained reports are labeled previous results and action/copy buttons are
disabled. Whole-collection failure keeps previous reports with an explicit error.
An ordinary unavailable result replaces the endpoint's old observation. A late
reply updates diagnostic state only, never navigation or another page's inputs.
The dedicated focus handle prevents hidden composers from receiving typing.
View sessions explicitly selects the configured endpoint; control acquisition
remains a separate existing action. No filesystem or RPC work runs in rendering.

Checks deliberately wait for the bounded batch before updating cards. Progressive
per-endpoint arrival, automatic recovery/subscriptions, configuration editing,
remote authentication and live Host update remain separate work. Native evidence
is Linux with owned Codex/ACP fixtures; external models and other platforms are
not covered. See [verification](../verification/2026-10-22-host-diagnostics.md).
