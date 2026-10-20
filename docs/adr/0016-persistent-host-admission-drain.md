# Persistent Host admission drain

Status: accepted for the local Host slice, 2026-10-20.

Session Hosts own provider stdio and must keep running through control-client
replacement. Updating a Host eventually needs overlapping versions, with old
work remaining on its original process. Previously every endpoint admitted new
work until it exited, so an old generation could never enter a controlled drain.

Each endpoint now owns a durable admission record: `ready` or `draining`, and a
monotonic generation. The controller supplies its existing lease and the exact
admission generation when changing mode. Both are checked. Repeating the current
mode with the current generation is a no-op; a transition increments generation.
SQLite commits the compare-and-swap before confirmation or in-memory visibility.
A lost reply requires a fresh snapshot, not an assumed transition. Storage errors
fault normal writes. Controller takeover never resets admission mode.

While draining, fresh mutations are rejected before journal admission or provider
dispatch. The sole supported mutation exception is `turn/interrupt`. Queries use
their existing bounded read channel, and pending provider answers keep their
existing lease/identity validation. This intentionally blocks new turns on old
sessions as well as new sessions, resume, rename, configuration and command
execution. Unclassified future mutations are blocked. Existing UUID lookup runs
first: saved results remain discoverable without replaying provider work.
Accepted operations and provider output are never cancelled by entering drain.

Protocol 3 adds admission/activity snapshots and `set_admission`. Update clients
and Hosts together; older protocol clients cannot take control. Journal schema 2
adds the admission singleton. New journals and schema-1 migration initialize
ready generation 0 within the migration transaction. Existing schema-2 journals
must contain a valid admission record; missing tables/rows or invalid generation
reject startup rather than silently reopening writes. Schema-1 binaries reject
the upgraded journal. Other receipt/binding/locking rules remain in ADR 0015.

Observed activity contains up to 128 active thread/turn identities, outstanding
accepted RPC count, and durable unknown receipt count. Pending provider requests
are already present in the snapshot. Known turn start/completion notifications
are matched by thread and turn. Idle status alone cannot clear a known active
turn; a pre-identity active marker can be retired by idle/not-loaded status.
Malformed known notifications, conflicting unsettled turns and capacity overflow
fault explicitly. Unknown future notifications are ignored. Only observed
activity is tracked; this is not a complete resource or provider-state ledger.

The native window exposes **Drain Host** and **Accept new work** for the owner,
shows activity and the first three active identities, disables new-work controls,
and retains permission/Stop/read controls. Every shared command is checked again
at dispatch; the Host remains the authority if snapshots race. CLI commands are
`fleetpane-host session SOCKET [--take-control] drain|ready`. The independent
named-Host workbench lets the user select another endpoint for new work. Mode
persists across reconnect and restart; a restart still creates a new provider
incarnation and does not restore its active work.

There is deliberately no automatic exit when activity reaches zero. Background
tools, PTYs and other descendants are not fully inventoried. Full version drain
still needs durable session ownership/routing, process supervision, resource
reconciliation and an authoritative retirement contract under ADR 0002. This
slice does not launch a new Host, migrate sessions, automatically route a new
session, force-stop work or restore providers after crashes. The tests use two
instances of the current executable, not a packaged cross-version upgrade.

See [process and native evidence](../verification/2026-10-20-host-drain.md).
