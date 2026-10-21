# 0049 — Explicit conversation file navigation

Status: accepted for the local read-only slice, 2026-10-21.

Paseo history ordinal 144 joins assistant paths, file routes and previews. Fleetpane
already has a bounded file engine and saved root identities. Conversation links
should enter that same engine without reading through a provider, claiming a Host
or replacing the selected conversation and its draft.

## Decision

Keep `Target { path, line }`, explicit link decoding and file/directory
classification in the session crate. GUI path input and CLI `workspace … open`
use `Workspace::open_path`. It classifies through descriptor-relative, no-follow
metadata and then uses the existing bounded listing/read implementation. A known
file does not require listing its entire parent directory first.

Explicit Markdown links support workspace-relative paths, absolute paths under
the saved root and local file URLs. Percent decoding preserves UTF-8 names and
encoded filename characters. `#L2`, `#L2-L3` and `:2` use positive one-based line
numbers; a range starts the preview at its first line, without highlighting the
range. Malformed anchors, remote file authorities and unsupported schemes return
visible errors. HTTP(S) retains the existing external-link action. Ordinary inline
code is not guessed to be a filename. Raw path input does not interpret colon
suffixes as line numbers.

Structured tool locations and supplied changed-file paths emit the same typed
navigation request. Tool location line metadata remains displayed as received;
this slice does not infer its indexing convention. Existing patch cards remain
available, while the user-deferred Git Diff expansion is unchanged in scope.

An item emits its cwd and target; the Host view adds the selected thread identity.
Workbench checks the selected Host/thread/cwd before starting a controller job and
again before displaying its reply. An independent request token invalidates work
when switching Hosts or screens. Rendering performs no filesystem scan. Exact
endpoint/cwd lookup reuses a saved workspace's root identity; a first explicit
click can register an otherwise unsaved local folder. Errors do not silently
re-register a replaced saved root. Aliases of an existing cwd are not exhaustively
covered by this exact-match lookup.

The file view overlays the retained conversation view. **Back to conversation**
restores composer focus. Direct file open shows its parent path, selected preview
and an explicit **Refresh folder** prompt; it does not issue an unneeded parent
listing. Folder refresh and file reload retain the current line anchor. A bounded
16 KiB UTF-8 preview starts at the requested line; **Copy full text** still copies
the entire loaded file. Missing source lines are explicit errors. A flexible
preview container leaves file metadata and controls visible within the window.
The earlier window-local location cache stores paths, not line anchors or scroll.

## Related first-open correction

Native verification exposed an existing nonpaginated history problem: metadata
selection starts draft loading, then the automatic full read was rejected by the
same guard that protects navigation away from an unfinished draft. Allow only a
non-resuming `Open` of the already selected thread through that draft guard. It
still requires the existing ownership/busy/admission checks; other-thread reads,
Inspect, resume, Send and Steer keep their protections. A changed cwd in the reply
is rejected before replacing the current thread. Existing draft-load state avoids
overwriting dirty/loading/saving text. No timer or retry is used.

## Verification and limits

See [local engine, CLI and native evidence](../verification/2026-10-21-file-navigation.md).
Root identity, worker/entry/byte/deadline limits remain authoritative. Native file
navigation preserves complete test Host snapshots and the exact draft. The token
guard is implemented, but a deliberately delayed stale reply was not forced in
this acceptance run.

OS-level `fleetpane://` routing, inline-code path chips, range highlighting,
Back/Forward, persisted anchors/scroll, file editing, media preview, recursive
watch/search and native acceptance on other platforms remain separate work.
Historical ACP flags, SDK dependencies, unbounded rollout scans and inferred
persistence capabilities are not copied into Fleetpane's negotiated provider
adapters or Host archive.
