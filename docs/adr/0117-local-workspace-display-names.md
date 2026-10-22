# Local workspace display names

Accepted 2026-10-22. Extend the saved workspace catalog and tabs with an explicit
device-local display name. Preserve the catalog UUID, Host endpoint, directory
identity, project, session bindings, draft keys, pins and saved layout.

Schema 18 adds `workspace_names`, keyed by the existing workspace UUID. An absent
row means revision zero and the existing default label. A custom name contains
1–256 UTF-8 bytes, without surrounding whitespace, control characters or line
separators. Preserve accepted Unicode exactly. Reset stores a new revision with
a null value rather than deleting the row, so reset cannot recreate revision
zero and admit a stale writer.

GUI and CLI share `Store::rename_workspace`. Read the observed name revision,
validate and save inside one immediate SQLite transaction. Exactly one concurrent
writer wins; a stale revision or failed write returns an explicit error and keeps
the previous name. Reject corrupt persisted names before returning a workspace;
do not silently substitute the directory label. Registration refresh and pins
join the same name record. The migration preserves all previous state; older
binaries reject the new schema. Migration fixtures must remove the new table
when constructing an earlier schema.

Name changes require only the saved UUID. They do not inspect the directory,
contact the Host, acquire control, rename a filesystem path or change a provider
session. A saved name can be changed with its Host offline and directory missing.
Different Host registrations of the same path keep independent names. A new
catalog identity at a replaced path does not inherit the old identity's name.

The native Workspaces catalog and pinned rows expose Rename. The Kit editor has
Save name, Use default name, Reload name and Close name editor. Save/reset wait
for the store; successful replies update loaded rows and matching open tabs.
Reload reads the latest revision and explicitly discards the editor text. Newest
refreshes loaded catalog/pin labels without resolving a name error or silently
rebasing an existing edit. Other-window changes are observed on explicit reload
or catalog refresh; there is no background name watcher.

Pending saves defer normal window close. A failed save keeps the editor text,
shows the actual error and blocks close until the user reloads the name or
explicitly closes the editor. An unrelated catalog refresh must preserve that
guard. Successful recovery clears the corresponding window-close error. The
editor participates in existing composition checks and has its own bounded
scroll region for small windows.

Tabs retain Host/path identity and the existing navigation, session, file and
terminal entities. A label update does not recreate those views or write layout.
The original path stays visible. This does not add project renaming, sync,
workspace move/archive/delete, provider-session renaming or a real-model claim.
