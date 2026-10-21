# Loaded session directories — 2026-10-22

Added an explicit directory suggestion menu derived from the selected Host's
loaded metadata. Exact deduplication and provider order, filtered/archived
replacement, 24-entry bound and invalid-path refusal are covered by shared tests.
`cargo test --workspace --locked` passed 261 tests; desktop tests passed 33.
Strict engine/desktop Clippy, Windows GNU engine Clippy, formatting and native
build passed. The existing recent-directory and session-tree process smoke checks
passed. [Receipt](assets/loaded-directories/receipt.json) binds source hashes,
historical source material and acceptance limits.

In task-owned hidden workspace `fleetpane-directories-20261022`:

- Three sessions produce two exact deduplicated suggestions; selecting the
  Chinese path copies exactly 37 bytes into the input. Every snapshot field stays
  unchanged except read sequence. [Menu](assets/loaded-directories/menu.png).
- Title filter `Fixture 1` leaves only `beta space`; another Host independently
  suggests `other-host`. [Filtered](assets/loaded-directories/filtered-menu.png),
  [other Host](assets/loaded-directories/other-host.png).
- Explicit **New session** creates `fixture-thread-4` at the selected non-Git
  directory. Query readback verifies cwd, idle status and no turns. Host 0's
  journal increases once; Host 1 has no mutation.
  [Created](assets/loaded-directories/created.png),
  [readback](assets/loaded-directories/created-readback.json).
- A 43-byte Chinese/English draft survives directory filtering, Host switching,
  another creation and normal window replacement, with exact clipboard readback.
  [Restored](assets/loaded-directories/restored-root.png). The 900×560 menu stays
  reachable and the composer remains visible.
  [Small window](assets/loaded-directories/small-menu.png).
- Kit TextView already enables mouse selection by default. Dragging existing
  Markdown and Ctrl+C returns the exact 19-byte `Selection evidence.` text.
  [Selection](assets/loaded-directories/selection.png). No selection implementation
  or mobile Markdown renderer was added; narrow-list wrapping is not newly accepted.

Replacement changes the explicitly claimed controller generation; Host/provider
IDs and PIDs, admission, activity, faults and the single creation receipt stay
unchanged. Both desktop instances closed normally, then owned workspace/fixtures
were stopped. Initial fixture setup used 0755 socket parents and was correctly
refused; changing only those task-owned directories to 0700 fixed setup.

History inspection advances continuously from ordinal 376 through 402, with
selected paths/hunks recorded individually. Bootstrap/MCP test plans are not
runtime proof. MCP agent creation/injection and remote discovery remain open.
No external model or macOS/Windows native acceptance was performed.
