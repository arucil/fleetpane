# 0012: Separate observational queries from mutation admission

Status: implemented, 2026-10-20.

## Problem

The original Host put every provider RPC in a lifetime journal of 128 operation
UUIDs. Opening history, loading older pages, refreshing the directory and reading
model capabilities all consumed permanent mutation admission slots. Repeated
reads could therefore prevent the next send. Evicting that journal would allow
an uncertain mutation to execute twice; increasing its size only delays failure.

## Decision

Add an observational `query` request alongside `execute`. Both require the current
controller lease. The Host allows only `thread/list`, `model/list`, `thread/read`,
`thread/turns/list` and `thread/items/list` through this path. Resume, send,
interrupt, rename, arbitrary commands and unknown methods still require operation
admission. The same shared command adapter selects this path for desktop and CLI.

Queries have no mutation UUID or operation-status reconciliation. The Host assigns
a provider ID in a separate `query:<uuid>` namespace, with at most 32 outstanding
queries. A provider result or error releases the slot and returns the raw reply
with its ordered response sequence to the original caller. The Host continues
handling claims, operations and provider notifications while awaiting results.
Query replies also participate in the existing bounded event ring, preserving
history watermark semantics.

Disconnecting a caller does not release an outstanding provider identity: its
eventual response must still be recognized and ordered. It remains bounded by the
32-slot limit until the provider replies or faults. Results are not retained in
the operation journal. Provider loss fails waiting queries and retains the
existing unknown-outcome behavior for admitted mutations. The local transport's
five-second deadline still applies; a read failure is visible and can be refreshed
explicitly. Neither timeouts nor read errors create a pending mutation or trigger
automatic retries.

The `execute` contract remains unchanged, including when a raw/older client uses
it for a read: every admitted operation keeps its UUID for the Host's lifetime.
The 128-operation limit therefore remains for that path. Snapshot operation IDs
continue to identify admitted operations only.

## Compatibility and limits

This is an additive protocol-v1 request/response. Older Hosts reject `query` as an
invalid request; new clients display the error and do not silently fall back to
consuming journal slots. Update the disposable prototype Host and client together.
Existing shared provider sessions are not migrated or restarted by this change.

This removes read-induced admission exhaustion. It does not make the mutation
journal durable or remove its lifetime limit. Durable admission, recovery and a
retention/acknowledgement contract remain necessary before removing mutation
records. Frame, event-ring and retained conversation bounds are unchanged.

See the [verification record](../verification/2026-10-20-host-queries.md).
