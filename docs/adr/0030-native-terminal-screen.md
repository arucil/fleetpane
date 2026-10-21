# 0030 — Native terminal screen and explicit input

Status: accepted for the Linux prototype, 2026-10-21.

## Context

Paseo's initial terminal tools provide a separate workflow from agent command
cards. ADR 0028 introduced Host-owned PTYs and ADR 0029 separated provider faults
from terminal admission. Users still needed a native surface to observe and
operate those shells. The first native trial also reproduced dropped clicks:
one `busy` flag disabled user controls during the 250 ms background read loop.

## Decision

The saved-workspace view opens a GPUI Kit terminal panel. Creation and mutations
use the existing typed `fleetpane-session` commands, workspace validation,
controller fencing, input sequence and durable operation journal. Listing/output
reads observe the current owner without claiming it. Taking control explicitly
transfers the whole Host. Closing a panel or desktop drops presentation state,
not Host resources. Host incarnation changes clear the old selection/control;
old commands are never replayed against a replacement Host.

The toolkit-independent session crate adapts pinned MIT-licensed `vt100` 0.18.1
into an incremental screen. Its upstream source is [doy/vt100-rust](https://github.com/doy/vt100-rust). It accepts raw bytes with exact workspace/terminal/PID
identity and monotonic range checks. Bounds are 60 rows, 200 columns and 1,000
scrollback lines; each Host response is at most 64 KiB. Disable the parser's
`std` default feature so `vte` uses its fixed-capacity OSC buffer rather than an
unbounded OSC vector. This does not make the entire application allocation-free.
Lost prefixes reset UTF-8/escape state and accumulate a visible incomplete-screen
warning. Resizing adjusts cells without reflowing old wrapped lines.

Only the selected screen is retained. The native view renders styles/cursor,
shows actual exit code/signal separately from output EOF, provides history paging and visible-screen copy, and uses an explicit multiline
UTF-8 composer with separate Send text, Send + Enter and control-byte buttons.
The textarea is not a raw terminal keyboard. Drafts stay with their terminal
UUID within this panel. Control keys preserve unsent text; successful sends clear
only the exact submitted draft.

Background reads and explicit operations have separate activity state. At most
one user action waits behind an active read; another action cannot overwrite it.
The read completes before that action is dispatched. Older selection output is
discarded without moving the new screen's cursor. Pending/uncertain mutations
retain their UUID and disable further mutations; Check operation only reads the
receipt. Host/controller fencing remains authoritative if ownership changes
between observation and dispatch. There is no automatic command retry.

## Limits

The current panel lacks raw key forwarding, native selection, IME acceptance,
mouse reporting, terminal query replies, automatic geometry, full terminal
compatibility and persistent panel drafts/selection. Pending UI state is transient;
Host operation receipts remain durable and CLI-discoverable. A lost Host output
prefix cannot reconstruct a complete full-screen application. Host updates still
terminate its PTYs; resource transfer/retirement is separate work. No performance
or complete replacement claim follows from this development-build acceptance.

See [native verification](../verification/2026-10-21-native-terminals.md).
