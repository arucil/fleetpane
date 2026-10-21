# Observed Agent progress beside the composer

Status: accepted, 2026-10-21.

Paseo history 187 adds a working indicator driven by the Agent's running state.
Fleetpane's existing header `Working…` described a pending client action instead.
An acknowledged send can finish that action while its Agent continues silently.
Those two lifetimes need independent presentation.

`fleetpane-session::progress` interprets an already observed Host snapshot and the
selected conversation. It performs no I/O, ownership claim, cancellation or
history mutation. The native composer displays its result outside the scrolling
details area. The header action label becomes `Applying action…`.

Host fault takes precedence. Host-owned Stop progress remains visible even after
the original turn finishes. Pending requests match the selected session and,
where supplied and known, the exact turn; they replace working animation with a
response prompt. Otherwise observed Host activity drives the Kit spinner.
Activity is checked before archived history membership because a provider can
retain archived descendants that still run. An active history row with no Host
activity displays synchronization rather than indefinite working or completion.
Unknown, archived and unloaded histories remain explicit. Paused/failed native
observation suppresses animation and asks for refresh.

The indicator is presentation only. It changes no Send/Stop permission, receipt
or admission rule. No running turn observed does not establish that subprocesses
or background resources have exited. No local elapsed timer or silent-output
timeout infers completion. Spinner allocation is limited to the selected Host
view's observed working state; no per-message timers are added.

Local verification and limits are in
[the acceptance record](../verification/2026-10-21-agent-progress.md).
