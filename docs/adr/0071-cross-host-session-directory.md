# Cross-Host session directory

Accepted 2026-10-21.

Paseo replay entry 202 introduces per-daemon accessors and a grouped session
directory. Fleetpane already retains separate local Host views, but finding a
session requires selecting each Host and taking control before loading its list.
Add **All sessions** as an on-demand observational directory in the workbench.

The shared `fleet_directory` engine reads one metadata page from each configured
Host concurrently, in configuration order. It borrows the existing controller's
query fence and never claims control. An unowned Host, disconnected endpoint,
provider fault, unsupported query or changed controller is an explicit per-Host
error. Each Host read has a 10-second deadline; collection waits for these bounded
reads. No provider or hidden conversation view is started. Query replies advance
the Host's event sequence, without adding mutation receipts.

The normal typed `Command::List` and `directory::Directory` contracts remain the
authority: at most 100 metadata rows per page, 5,000 rows / 8 MiB per Host,
metadata-only turns, exact query/cursor continuation, overlapping-row deduplication
and cursor-cycle refusal. At most eight configured Hosts give an aggregate bound
of 40,000 rows / 64 MiB of encoded metadata; transport frames impose additional
per-page bounds. Nothing preloads full histories. One refresh or page action runs
at a time, and rendering only consumes loaded metadata in a virtualized list.

Native title search applies to each provider's complete index. Host filter buttons
only choose which groups are visible; Search / Refresh still checks every Host.
DeepSeek ACP's unsupported server-side title search remains an explicit error.
Empty input clears the title filter. CLI also supports an exact workspace filter;
native workspace and provider archive filters remain available in the Host view.

Groups retain the last successful rows on a failed read of the same query, mark
them stale and disable source opening. A failed different query clears that
group's old rows, so unrelated results cannot masquerade as matches. Other groups
remain usable. Load more uses the original Host incarnation and opaque cursor;
host replacement requires refresh. The directory has no automatic polling or
cross-Host deduplication by provider thread ID.

Opening targets the exact configured endpoint, Host incarnation and thread ID.
It selects the existing retained Host view, then uses the normal metadata/history
inspect command with `resume: false`. If the view is connecting, busy, saving a
draft or controlled elsewhere, it retains a source banner with an explicit Open
selected session action. Take control remains separate. A changed Host disables
opening the stale source. Source preparation never resumes a context, sends a
message, answers a request or discards a draft. Successful inspection clears the
source banner and uses the existing history pager and durable draft identity.

CLI `sessions CONFIG [--search TITLE] [--cwd DIRECTORY]` returns per-Host reports.
`--host NAME --cursor CURSOR --host-id UUID` continues one observed Host with the
same filters. Report errors are data, not a claim that all Hosts were available;
invalid arguments fail the command. This is a local metadata observation, not a
durable global ownership table, live activity order or remote connection layer.
Registry editing, authentication/relay, unread state, background subscriptions,
cross-Host activity sorting and Host runtime migration remain separate work.
