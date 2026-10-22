# Workspace alert scopes from observed session metadata

Accepted 2026-10-22. PRD ATT-04 requires workspace mute to cover future sessions
without changing the true pending queue. Extend [local alert policy](0111-device-local-alert-policy.md)
with `{ socket, directory }`: the configured Host endpoint and exact absolute
provider directory. Labels, the currently selected tab and request payload cwd
are not workspace identity. A scope does not include child directories or the
same path on another endpoint. Endpoint rules survive Host replacement; exact
session rules retain their existing incarnation-bound identity.

The Host retains at most 64 provider-observed session-directory records within
128 KiB. Validated thread responses and `thread/started` provide the association;
ordinary deltas and arbitrary request fields do not. Replacement Host state
starts empty. This cache survives event-ring expiry without fetching transcripts,
claiming control, scanning files or adding provider RPCs to alert collection.
Its eviction count remains inspectable in snapshots. `session_directories: null`
or an absent field identifies an older Host without this observation support.

Join pending requests and terminal results by their original thread ID within
that Host snapshot. An unknown or evicted directory stays included in alerts
unless a broader global/event/Host/session rule mutes it. If workspace scopes
exist on that endpoint, the row explicitly says its workspace is unknown. This
conservative behavior can produce an extra alert; it cannot silently hide an
unassociated request. Normal explicit session reads repopulate metadata, but
notification collection never performs them implicitly.

Preferences remain schema 15, with an optional `muted_workspaces` JSON field;
old records deserialize to an empty scope list. Limit it to 64 unique scopes
and retain the existing 512 KiB record bound, strict validation and shared GUI/CLI
compare-and-set save. Relative paths, parent traversal, NULs, duplicates and
oversized values are rejected before storage is opened. Matching does not touch
the filesystem. Neither mute nor unmute answers, reviews or changes a result.

The current workspace header provides an explicit toggle. Alerts lists saved
scopes for removal even when their tabs are closed. Use GPUI Kit's controlled
popover with bounded scrolling rather than an in-flow settings panel: native
900×560 reproduction showed the latter consuming the session input/action area.
Kit handles focus, Escape and outside dismissal; Fleetpane blocks background
Next request, tab navigation and session-switcher shortcuts while it is open.
Show directory first and expose full endpoint/directory in the toggle tooltip.

This completes the covered local workspace-policy workflow, not ATT-04/05 as a
whole. OS notifications, sound, tray/dock, durable delivery deduplication,
cross-device preferences and broader platform native acceptance remain open.
