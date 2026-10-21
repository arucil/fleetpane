# Independent provider and terminal fault domains

Accepted 2026-10-21. Paseo root-history ordinals 28–34 introduce artifact/audio
state, an MCP terminal boundary and transport-independent Session ownership.
Fleetpane already separates its Rust engine from the controller; its actual
provider-failure reproduction exposed an incomplete resource lifetime boundary:
Host-owned shells survived but input and Close shared the provider admission fence.

Classify operation ownership from the exact immutable method allowlist. Terminal
create/input/resize/rename/close belong to the Host's PTY manager; all other
methods, including unknown terminal-prefixed methods, belong to the provider.
Controller generation, Host incarnation, drain and durable receipt checks apply
to both. No string-prefix escape or unjournaled cleanup path is introduced.

A provider failure marks only its pending operations Unknown in memory and in
one journal transaction. Pending PTY receipts remain Accepted until their actor
supplies a result; existing and new terminal commands remain available. Provider
RPCs and answers stay fenced. Completion origin must match operation ownership
before changing either the durable or resident result. A provider response may
never finish a PTY receipt, nor may a PTY result finish provider work.

A Host, journal or terminal-completion infrastructure failure marks every pending
operation Unknown and fences all mutations, including admission-mode changes.
Journal completion/persistence errors escalate to this scope. A later provider
error cannot downgrade it. Observational local PTY reads and explicit controller
claims remain available; a claim cannot reopen mutation admission. Uncertain
input remains fenced independently on that individual PTY.

Snapshot gains an optional `fault_scope` (`provider` or `host`); absent scope
deserializes for old snapshots. Existing `fault` remains the aggregate diagnostic
used by Host routing, so a failed provider is never recommended for fresh agent
work. No protocol-version bump or journal-schema migration is needed: new
snapshots add data and ownership is derived from already persisted methods.

This does not restart a provider, recover dead agent sessions or move a PTY
between Host processes. Host updates/handoff, native terminal rendering and
conversation attachment remain independent required work.
