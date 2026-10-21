# Supplied command actions and compact conversations — 2026-10-22

Shared typed projections and the covered Linux native fixture workflows pass
locally. Current Codex actions preserve every supplied read, list, search,
unclassified and future action. Missing/null actions remain unavailable; an empty
array stays empty; malformed known actions remain visible errors. Raw command,
combined output, unknown extensions and null exit status survive full Copy.

`cargo test --workspace --locked` passes 268 tests; the separate desktop suite
passes 35. Both formatters and strict Clippy pass. Windows GNU engine Clippy,
16 Python script tests and `scripts/file-navigation-smoke.py` pass. Logs and
source/binary digests are in the [receipt](assets/command-actions/receipt.json).

The owned `fleetpane-command-20261022` desktop exercises seven synthetic histories.
At 1360×900 and 900×560, read links open separate files, including a Chinese
filename, through the read-only file browser. The first file copies exactly ten
bytes. Native command Copy returns all 970 bytes of original item fields,
including six ordered actions and combined output. A 37-byte Chinese multiline
draft survives session switches and window replacement, with clipboard readback.

At 900×560, optional settings move into Session options and its scroll region.
Conversation content, composer and explicit approval/Decline/Stop controls remain
reachable. One explicit synthetic approval turn was sent and declined. The same
Host UUID/provider PIDs, ready admission and zero faults remain; the journal has
exactly that one turn. Replacement requires explicit takeover (generation 2).
The native window closed normally after verification.

This closes the previously recorded compact conversation space issue for these
fixtures. It does not establish arbitrary-content layout, IME, non-Linux native,
external-model or performance acceptance. Supplied actions describe the provider's
interpretation; link navigation reads current bytes, not historical file contents.
# Command cwd follow-up

Relative provider read links now resolve against the command's own directory.
The [separate regression and native evidence](2026-10-22-command-cwd.md) covers
same-name root/subdirectory files, literal filenames and retained drafts.

