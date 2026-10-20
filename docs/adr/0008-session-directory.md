# Paged session directory and names

Accepted 2026-10-19 for the local Codex Host.

## Problem

The Host view previously requested only the first 100 threads and discarded the
provider's next cursor. The user could not find older sessions beyond that page.
There was no title/workspace query or session rename command. Filtering only
loaded rows would make global search return misleading omissions.

## Decision

`fleetpane-session::directory` owns provider query parameters and transactional
page application. `Command::List(PageRequest)` is shared by the CLI and desktop.
The Codex 0.163.0 generated schema supplies `searchTerm`, exact `cwd`, opaque
`cursor`, `limit`, `sortKey: updated_at` and descending order. Each page carries
the same filters, 100 metadata rows maximum and no conversation turns.

Apply/Enter starts a query at its first page; Load more uses its next cursor.
Refresh restarts the successful query, and Clear removes filters. Visible query
labels describe the last successful query, even when newly edited fields or a
failed request differ. A failure preserves rows, cursor, selected conversation
and composer draft. Directory errors do not pause the unrelated history stream.
The selected conversation can remain open outside the current result set.

The native directory is virtualized and bounded to 5,000 sessions / 8 MiB of
encoded metadata. The existing transport frame cap also applies. On overflow,
retain the previous page state and request narrower filters. Reject duplicate
IDs within a page, empty IDs, malformed/oversized metadata pages, cursor cycles
and pages that do not continue the current query. Overlap between pages updates
the existing identity instead of duplicating rows. A changing updated-at index
is not a point-in-time snapshot: overlap merging cannot guarantee every row is
seen during concurrent activity. Refresh is the reconciliation action.

`Command::Rename` writes `thread/name/set`, rejecting blank names and names above
4 KiB before submission. It trims whitespace as Codex does. GUI Rename session
opens an inline editor; Save name or Enter commits, Cancel closes without a
write. Failed edits retain the input and old title. Success updates the selected
name and refreshes the current query, allowing a session to leave its old filter.
`thread/name/updated` also updates matching selected/directory metadata; outside
updates show a refresh notice for possibly stale filtered results. Status
notifications update a loaded row's state without loading its history.

All provider commands keep the Host's controller fencing and existing operation
identity semantics. No implicit ownership takeover or request retry is added.
Filter inputs and pages live in the current desktop process; application restart
does not restore them. Title filtering follows provider semantics; full-text
conversation search, pin/archive, status filters and history pagination remain
separate work.

## Verification

See [the verification record](../verification/2026-10-19-session-directory.md):
237 synthetic sessions through CLI and native UI; Unicode names, filtering,
failure/empty states, draft retention and keyboard interaction; isolated real
Codex 0.163.0 name/query readback. Real Codex pagination above 100, a native
5,000-session run, IME and non-Linux desktop acceptance remain unverified.
