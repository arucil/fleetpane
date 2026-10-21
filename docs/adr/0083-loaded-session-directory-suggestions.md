# ADR 0083: Suggest directories from loaded sessions

Status: implemented; covered Linux acceptance recorded separately.

Paseo history ordinal 389 replaces device recents with directories from sessions
on the selected Host. Fleetpane retains useful persistent recents and adds a
separate **Loaded session directories** menu above them.

The shared Directory projection returns exact absolute UTF-8 cwd values in loaded
provider order, deduplicated without trimming, canonicalization or filesystem I/O.
It validates every row, returns the full distinct count, and limits the menu to
24 paths. Invalid metadata yields a visible error rather than skipping a row.
Current Host, accepted filters, archive membership and loaded pages define scope;
no full-index, freshness or directory-existence claim is made.

The desktop computes and caches suggestions when directory replies or matching
selected metadata are applied, outside rendering. Selection only fills the new
session input. It does not change the selected conversation, save MRU, take
control, create a session, or send a draft. Prepared creation/workspace bindings
disable replacement, with the binding rechecked inside the callback. Ordinary
creation retains its existing path validation and successful-result MRU update.
Menus do not mix configured Hosts. Window replacement reloads provider metadata.

This adds no provider commands, discovery scan or persistent preference schema.
Persistent three-item MRU, saved workspaces and pins retain their existing roles.
MCP agent spawning, injected tools, provider changes and remote discovery remain
separate work.

[Local verification](../verification/2026-10-22-loaded-directories.md).
