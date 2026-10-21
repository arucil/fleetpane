# Local file export — 2026-10-22

Shared `Workspace::export_file` and `fleetpane-host workspace STORE export UUID
RELATIVE_SOURCE ABSOLUTE_DESTINATION` copy current bytes to a new path. The native
file view exposes **Save a copy…**, an exact destination field and **Save copy**.
Existing targets are kept; failures remain visible without discarding input.

Two engine tests cover raw bytes/hash, private mode, source/destination conflicts,
links, FIFO, size bounds, cleanup and replaced workspace identity. The process
smoke exports 1,048,852 bytes without a Host, verifies fresh/empty input, races two
independent CLI exporters to one destination (one success/one refusal), and checks
boundary failures plus no remaining spool. Full workspace tests pass 273 tests;
35 separate desktop tests, both formatters and strict Clippy pass.

Owned Linux X11 native verification exports an 8,216-byte binary to a Chinese
filename containing literal percent/hash characters; exact bytes/hash/mode match.
**Copy result** returns 145 exact bytes including full path and SHA-256. Repeating
the destination shows an error and preserves its bytes. Changing the source after
preview then exporting gives the current 23 bytes, also at 900×560. Returning to
conversation preserves the exact 27-byte Chinese multiline draft; Host journal
stays empty. The native window closes normally and workspace stops.

`python3 scripts/file-export-smoke.py`, `file-navigation-smoke.py` and
`files-smoke.py` pass. [Receipts and raw evidence](assets/file-export/receipt.json)
pin source and binaries. Native runtime is Linux with synthetic provider only.
Long-blocked export/close behavior, filesystem crash/hostile-writer failures,
force-termination cleanup and non-Linux native behavior remain unaccepted. No
system save picker/share sheet, remote download, overwrite or editable save.
