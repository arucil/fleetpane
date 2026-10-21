# Supplied web-search items — 2026-10-22

Covered shared projections and Linux native fixture interactions are verified
locally. The installed Codex 0.163.0 JSON schema establishes the item/action
contract; no external search or model request was performed.

The not-unknown regression fails at `c5a48dc` and passes with this change. Shared
tests cover exact Unicode/whitespace, optional fields, opaque results, future
actions and malformed known fields. The desktop regression checks raw extension
Copy and visible invalid-item presentation without an invented status.

`cargo test --workspace --locked` passes 266 tests; the separate desktop suite
passes 34. Both formatters and strict Clippy pass. Windows GNU engine Clippy is
recorded separately from native acceptance. The native fixture used the debug
binary whose digest is retained in the receipt.

The isolated `fleetpane-search-20261022` workspace exercises six separate
synthetic histories: search, open page, find in page, unknown action, missing
metadata and malformed known action. Screenshots cover readable query/action
details, literal HTML in result JSON, URL/pattern presentation, zero versus
unavailable results, unsupported raw action and the red invalid-item error.
The public render fixture adds these cases after the existing eight samples,
preserving earlier fixture identities.

Native Copy reads back the exact 318-byte search fields and 174-byte future
action fields. Copy is the original item fields, not a reconstructed item with
`id`/`type`. A 37-byte Chinese multiline draft survives session switching and
window replacement with exact clipboard readback. Replacement reloads the search
history and requires explicit takeover. Original Host UUID, Host/provider PIDs,
zero mutation journal, admission, activity and pending requests remain unchanged;
controller identity/generation changes with that explicit replacement takeover.
The replacement window closed, then the owned workspace/processes were stopped.

At 900×560 the composer remains reachable, but fixed session headers and
interaction panels leave too little conversation height for satisfactory search
reading. Minimum-size conversation layout remains open; this receipt does not
claim acceptance at that size. No external model, remote URL, non-Linux native,
IME or whole-application performance acceptance is claimed.

[Receipt and screenshots](assets/web-search/receipt.json) bind source hashes,
schema, binaries, test logs, clipboard evidence and the unchanged runtime fields.
