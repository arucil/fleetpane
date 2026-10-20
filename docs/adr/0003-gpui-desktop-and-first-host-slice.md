# 0003: GPUI desktop and the first Session Host slice

Status: implementation experiment, 2026-10-19. GPUI is the selected toolkit for
this slice; replacement readiness remains subject to Linux input, terminal,
accessibility and workload verification.

The desktop dependency decision below is historical and superseded by
[ADR 0004: GPUI Kit](0004-gpui-kit-desktop.md). The Host implementation and its
limits described here remain current.

## Decision

Use the published Apache-2.0 `gpui` 0.2.2 crate for the native desktop, with a
virtualized session directory. Keep `fleetpane-protocol`, `fleetpane-client` and
`fleetpane-runtime` free of GPUI dependencies. Rendering performs no network I/O.
Directory requests run off the UI executor, one at a time, with the existing
bounded client. The UI keeps explicit pagination and errors; it does not subscribe
to every session's history.

The desktop has a separate manifest and committed lockfile so CLI/host builds do
not require native graphics dependencies. A separate Linux CI job checks it.
This arrangement is a build boundary, not another domain implementation.

The desktop lockfile resolves `libc` to 0.2.186: GPUI 0.2.2 transitively uses
`gpui_http_client` -> `zed-async-tar` -> `xattr` 0.2.3, which references Linux
`libc::ENOATTR`. That alias is absent in 0.2.191 and present in 0.2.186. The pinned
compatible dependency fixes the observed compile failure without patching third
party source or suppressing compiler checks. Recheck this chain when upgrading
GPUI; do not refresh this lockfile blindly. The engine uses its own lockfile.

GPUI's transitive `proc-macro-error2` 2.0.1 emits a future incompatibility notice
with Rust 1.98.1. It is not suppressed; a future toolchain upgrade must check it.

Closing the last window posts the platform quit request to the foreground
executor. GPUI 0.2.2's X11 `WM_DELETE_WINDOW` callback holds a mutable platform
borrow; synchronously calling `quit()` from its close observer reenters that
borrow and panics. Deferring this platform action until the callback returns
preserves the borrow boundary without suppressing errors. A surface creation
failure exits at the startup boundary with a visible error and nonzero status.

## Host boundary

The Unix prototype hosts one initialized provider over private stdio. A separate
`fleetpane-host request` process connects to a private Unix socket, issues one
command and exits. Closing a control client does not close provider stdin.
This is the transport/ownership foundation for the replaceable control daemon,
not a complete control daemon, desktop-to-host integration, or service installer.

The host serializes claims and commands. A claim must match protocol version,
host incarnation and expected controller generation. Each transfer advances the
generation and invalidates the old controller's writes. Commands are admitted
under a UUID before provider I/O; resubmission with the same ID and contents
retrieves the recorded operation instead of submitting again. Changed contents
under the same ID are rejected. Snapshot operation IDs allow a new controller to
discover accepted work. Provider initialization metadata is retained in snapshot.

Provider requests, including approvals, retain their original identity until
answered. No disconnection implies acceptance. A snapshot and bounded event
cursor expose state; an expired cursor requires explicit resynchronization.
After claiming, the new controller reads a fresh snapshot to cover changes made
between its earlier read and the claim. Provider output is consumed while
controllers are absent. The current snapshot is host transport state, not a
complete conversation timeline projection.

## Explicit prototype limits

- Unix sockets only, local trusted controllers belonging to the same OS user.
  The socket parent must be a non-symlink directory with private permissions;
  the socket is 0600. This is not an authenticated multi-user or remote API.
- Start the host independently of control clients. There is no supervisor/service
  installation yet. Never launch it under a control daemon's cleanup group and
  infer upgrade safety from its existence.
- The operation journal is in memory, capped at 128 `execute` commands for the
  entire Host lifetime. [ADR 0012](0012-observational-host-queries.md) separates
  directory/model/history queries from that admission budget. The journal rejects
  further operations instead of evicting identities and permitting duplicate
  mutations. No journal survives Host restart.
- Provider and controller input frames are capped at 256 KiB; responses at 1 MiB.
  Pending provider requests are capped at 64 and 512 KiB; the event ring at 128
  events and 8 MiB. Oversized provider output faults the runtime rather than
  silently truncating a protocol message. Conversation projection, pagination of
  large provider payloads and persistent output storage remain to be implemented.
- Provider loss or uncertain writes mark accepted operations `unknown` and stop
  further writes. There is no automatic restart, resend or claimed reconciliation.
- Ctrl+C stops the host and provider. Host/provider upgrades and crashes are not
  covered by controller replacement. Existing Paseo private stdio sessions cannot
  be adopted by this prototype.
- The desktop reads the existing Paseo directory. It does not yet create or show
  Host-backed conversations. Full LLM turns, streaming tools, terminals and real
  approvals still require their own runtime acceptance evidence.

See [the reproducible checks](../verification/2026-10-19-foundation.md) for what
was actually exercised and what remains unverified.
