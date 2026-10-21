# ADR 0032: Device appearance is independent of session runtimes

Accepted 2026-10-21.

Paseo root-history commit 51 introduces mobile settings and conversation
selection. Its inspected settings hook updates state before persistence and can
hide failed reads behind defaults. Fleetpane's PRD UX-02 also requires native
light/dark appearance. Implement this as a device preference, independent of Host
configuration, controller leases, provider options and conversation state.

Keep the typed System/Light/Dark contract in the session crate and use the same
revisioned Store API from CLI and GPUI Kit. Controller-store schema 6 adds a
singleton appearance record; upgrading preserves existing catalog, creation,
pin and draft data. The default device directory is separate from every
socket-derived controller store. Reuse the existing private SQLite storage
boundary rather than introducing a second persistence mechanism.

An absent record means System at revision zero. Invalid JSON, unknown fields or
themes, invalid identity/revision, oversized records and missing current-schema
tables are explicit errors. Never overwrite them with defaults. Save with an
immediate transaction and expected-revision comparison; stale writers must reload
and choose explicitly. Apply a requested theme only after commit succeeds.

The native Frame retains the existing Screen and its Host views. Theme changes
update Kit's theme and refresh windows without reconnecting or recreating
provider sessions, requests, drafts, terminals or cards. System follows actual
window appearance notifications; fixed Light/Dark ignore them. Initial System
preview is allowed while loading, with an explicit unavailable state on failure.
No periodic preference polling or automatic write retries are added. Separate
app processes see another writer on Reload or through the next save conflict.

The Frame owns appearance I/O; the Workbench owns draft I/O. A window has one
close callback, so the Frame must delegate to the Workbench's existing guard.
Wait for pending appearance I/O, then ask the draft guard before closing. Keep
the window open on save failure and disable appearance actions during the
Workbench's close sequence. Forced process termination remains outside the
normal-close guarantee.

Linux resolves XDG_CONFIG_HOME, then HOME/.config; macOS resolves
HOME/Library/Application Support. An explicit --preferences-dir supports
isolated use. Empty/relative environment paths are errors. Windows preference
support and macOS native acceptance remain open. Font, language, shortcuts,
notifications and layout preferences are separate remaining work.
