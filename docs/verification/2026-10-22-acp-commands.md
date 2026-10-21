# ACP command advertisements — 2026-10-22

Covered shared, Host-process and Linux X11 native fixture workflows pass locally.
The ACP idle-update regression was reproduced before implementation: the Host
treated `available_commands_update` as conversation activity without a prompt.
Commands now replace live metadata independently of text, tool and permission
streams. Invalid, duplicate or oversized lists fail atomically; current lists
retain exact names, hints, metadata and future fields. Archive/restart/resume
forgets availability until the provider advertises it again.

`cargo test --workspace --locked` passes 293 tests; the separate desktop suite
passes 36. Both formatters and strict Clippy pass. Python Ruff checks/formatting,
`acp-commands-smoke.py`, `acp-info-smoke.py` and `acp-smoke.py` pass. The command
process smoke covers idle and pending-permission updates, exact extensions,
empty replacement, another-session isolation, no implicit prompt/history entry
and restart/resume expiry. A desktop regression verifies that advertisements
leave content-row inputs unchanged. [Logs and digests](assets/acp-commands/receipt.json).

The owned `fleetpane-commands-20261022` desktop used two synthetic resident
sessions and a private metadata inbox. The rendered menu displayed descriptions,
input hints and Chinese command names. Selection prepared `/检验`, read back as
exactly seven bytes. The fixture recorded one explicitly sent ordinary prompt,
`/web query 中文  ` with its two trailing spaces. Dynamic empty/nonempty
advertisements and separate session availability were inspected.

At 900×560, the menu remains reachable by scrolling the options region; it can
initially be below that region's viewport after window replacement. A multiline
Chinese draft, including two spaces before a newline, survived normal close,
replacement, explicit takeover and reopening, with exact 32-byte clipboard
readback. Full command Copy returned 283 bytes and retained opaque extensions.
Before/after snapshots retain the same Host UUID, identical activity records and
no fault; the provider's prompt log still contains only the one explicit send.
The owned desktop and fixture Host were stopped after verification.

No installed-provider command advertisement/execution, images combined with a
command, IME, other-platform native, remote CI or performance acceptance is
claimed. The menu prepares protocol text; the provider defines what a command
does. [Protocol source](https://agentclientprotocol.com/protocol/v1/slash-commands),
[decision](../adr/0094-live-acp-command-advertisements.md).
