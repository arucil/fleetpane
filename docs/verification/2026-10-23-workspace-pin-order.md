# Workspace pin order acceptance

2026-10-23. Linux x86_64, isolated X11 `:91`, GPUI Kit 0.7.3, debug build,
Mesa software renderer. Source base `708d313ec5e6d1674bdc32fb9b1156e235eea4d2`
plus this change. Accepted desktop SHA-256:
`d76fa515d502d84d746f5e59c4739c8de73d5a04b57fba0bd88806b2625ad06c`.
[Receipt and source hashes](assets/workspace-pin-order/receipt.json),
[decision](../adr/0119-local-workspace-pin-order.md).

## Native fixture

`python3 scripts/workspace-names-smoke.py --fixture /tmp/fp-pin-order-native`
starts owned synthetic Codex/ACP Hosts, original pending questions/permissions,
two independent 43-byte drafts, and three same-path workspace registrations on
separate endpoints, including one offline Host. The probe seeds saved tabs and
injects only local SQLite failures. All source data is synthetic.

- **Down**, **Up** and the dedicated drag handle persist the expected UUID order.
  A drop in empty space leaves all pins exact. The offline entry moves without
  contacting its Host. Labels and other entries retain their identities.
  [Initial catalog](assets/workspace-pin-order/catalog-before.png),
  [order receipts](assets/workspace-pin-order/down.json),
  [drag](assets/workspace-pin-order/drag.json),
  [cancelled](assets/workspace-pin-order/cancelled.json).
- An injected insert error rolls back the preceding pin deletion. The displayed
  order stays unchanged and Alt+F4 leaves the window open with the actual storage
  error. Removing the trigger and **Reload pins** clears the close warning and
  restores editing; the next explicit Down succeeds.
  [Refused close](assets/workspace-pin-order/refusal-close.png),
  [900×560 error controls](assets/workspace-pin-order/small-refusal.png),
  [recovered](assets/workspace-pin-order/recovered.json).
- A separate CLI process changes the saved order while the catalog holds the old
  list. The next native move is refused; saved order equals the external result.
  **Reload pins** shows the new order. **Keep saved pins** after another injected
  failure changes no pin row and allows a subsequent normal close.
  [Conflict](assets/workspace-pin-order/stale-refused.png),
  [external order](assets/workspace-pin-order/external-order.json),
  [kept](assets/workspace-pin-order/kept-saved.json).
- At **900×560**, the first-row Down control saves correctly; the catalog has its
  own list scrolling. Two normal close/replacement cycles preserve exact pin
  records, names and both draft rows. Native ACP composer Copy before and after
  replacement returns all **43 UTF-8 bytes**, including the real newline and two
  trailing spaces. Explicit Take control enables copying; no request is answered.
  [Small catalog](assets/workspace-pin-order/small-pins.png),
  [restored pins](assets/workspace-pin-order/restored-pins.png),
  [draft before](assets/workspace-pin-order/draft-copy-before.png),
  [draft after](assets/workspace-pin-order/draft-copy-after.png).

All settled pre-control ordering/failure/reload intervals preserve both complete
Host snapshots, draft records and name records exactly. Initial Codex history
reading advances only transport `sequence`. Later explicit control changes owner
and request-history reads advance sequence; these are distinct from ordering.
The small-window ordering interval after control still preserves complete Host
snapshots exactly. Three Alt+F4 exits have code 0. Earlier automation close calls
removed windows but left processes alive; they are not normal-close evidence and
were stopped with the isolated workspace. Owned Hosts/providers were stopped and
their sockets removed; the fixture source file remains exact.
[Cleanup](assets/workspace-pin-order/cleanup.json).

## Checks

- `cargo test --workspace --locked`: **370 passed**, including all seven pin tests.
- `cargo test --manifest-path apps/desktop/Cargo.toml --locked`: **51 passed**.
- `python3 -m unittest discover -s scripts/tests -p '*_test.py'`: **22 passed**.
- Strict all-target Clippy for both workspaces, both fmt checks, Host/desktop
  builds, Windows engine cross-check and `git diff --check` pass.
- `python3 scripts/workspace-pins-smoke.py`: independent processes retain
  reordered pins, reject malformed/oversized/duplicate/incomplete/stale lists,
  roll back failed writes and allow exactly one concurrent changed-order winner.
  Catalog, exact drafts and entire Host snapshots remain unchanged.
  [CLI receipt](assets/workspace-pin-order/pins-smoke.log).
- Engine tests additionally cover the full 32-pin list, empty and unchanged
  orders, failure on the second insertion, reopening, name preservation and
  new/re-pinned-first ordering.

[Commands and logs](assets/workspace-pin-order/checks.txt). This is local
acceptance; new exact-head remote CI is not asserted. No macOS/Windows GUI,
native Wayland, installed OpenCodex/DeepSeek model run, cross-window drag,
automatic other-window refresh, IME or release performance claim is made.
