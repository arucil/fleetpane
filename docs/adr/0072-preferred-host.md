# Remember the selected local Host

Accepted 2026-10-21.

Paseo chronological entry 203 separates primary/background session accessors and
persists the active daemon. Fleetpane already retains one lazy conversation view
per endpoint, but startup always initialized the first configured Host. Remember
the chosen **socket path** in the configuration's existing controller store.
Names, indices, provider IDs and Host incarnations are not preference identity.

Store schema 11 adds a private singleton with a positive revision and bounded
JSON-encoded absolute socket path. Reads reject malformed or oversized data;
`save_host_selection(expected_revision, socket)` uses an immediate transaction.
A concurrent window must reload after a revision conflict. Failed transactions
retain the previous record. Older-schema upgrades preserve drafts, workspaces,
Paseo migration receipts and device appearance. Earlier binaries that only know
schema 10 cannot open the upgraded controller store.

The GUI asynchronously loads the preference before initializing its first Host
view. Explicit navigation made while loading takes precedence. Restoration never
claims control, opens/resumes a provider session or changes runtime ownership.
Each later explicit Host selection records the same endpoint even when it is
offline. An absent endpoint shows the first configured Host with an explanation;
it never silently overwrites the saved endpoint. Rename/reorder keeps the exact
socket binding. Replacing the Host at that socket still requires normal fresh
incarnation and lease validation; the preference grants no authority.

Rapid selections coalesce to the latest desired endpoint while one save runs.
The current view stays usable after a write failure, but the preference error is
visible. Reload choice only refreshes the local revision, without navigating or
implicitly retrying the failed write. Remember current Host explicitly saves the
visible selection. Keep previous choice discards the unsaved preference attempt
and retains the database record. Normal titlebar close waits for pending reads
and saves; errors keep the window open until reload or explicit discard, alongside
the existing draft/terminal guards. Forced process termination cannot guarantee
an unsaved choice.

CLI `host-choice STORE get | set EXPECTED_REVISION CONFIG HOST` uses the same
store/domain command. CLI set validates configured membership and does no Host
RPC. The choice belongs to the supplied controller store, not the global theme
preference store. Moving or renaming the configuration file changes its default
controller-store path. Session/scroll restoration, editable Host registry, remote
connections, background model prefetch and telemetry remain separate work.
