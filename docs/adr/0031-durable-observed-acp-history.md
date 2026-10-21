# ADR 0031: Persist Host-observed ACP activity separately from live context

Accepted 2026-10-21.

Paseo root-history commit 43 adds file-backed conversations and ACP agents.
Fleetpane already keeps provider runtimes independent of control clients, but its
ACP adapter kept observed messages only in memory. A Host/provider restart could
resume DSH context while showing an empty conversation. ACP resume does not import
the provider transcript. Reading DSH private files would couple this guarantee to
a separate format and is outside this decision.

Use the existing private, exclusive-owner Host SQLite journal. Schema 3 adds
identity-keyed session metadata, turn metadata and individual item rows. Upgrade
v1/v2 transactionally while retaining admission fencing and operation receipts.
A current schema with a missing archive is corruption. Save affected rows in one
transaction before writing translated requests or publishing resulting events.
A streaming update changes one item rather than rewriting previous turns. Bound
stored rows/bytes and the reconstructed adapter independently; never silently
truncate. Count UTF-8 bytes and update accounting only after commit.

Restart reconstructs observed history in original insertion order. Archived
sessions are explicitly `archived`, can be read/paged without resuming, and cannot
accept prompts/configuration. The desktop offers **Resume context**. Only an
explicit, successful `session/resume` changes residency, refreshes configuration,
and allows new prompts; retain old observed turns. Metadata-only resume respects
`excludeTurns`; explicit full reads keep existing frame-limit refusal.

An unsettled archived turn is `unknown` with an explanation. Never infer success,
resend its prompt, restore pending provider RPCs, or reuse a prior permission.
History cursors belong to a Host incarnation and fail after restart. Repeated
resume while pending is rejected. A workspace mismatch from provider listing is
an error, not permission to relabel saved history.

Archive write failure fences Host mutations, including terminals, before the
unsaved event is published. Malformed/corrupt/oversize archive prevents startup;
no empty-history fallback. This is distinct from a provider-only failure, which
continues to leave independent terminals available.

Limits: only observed activity from this implementation onward is retained.
Plans remain in the separate bounded, transient plan cache. No transcript import,
archive deletion/retention UI, unbounded history, cross-socket migration or live
process takeover is added. Small committed observations may not have reached the
client before a crash; restored history may contain more than the last rendered
view. SQLite acknowledgement depends on the existing EXTRA-synchronous journal
and underlying filesystem. Performance under many persistent streams needs its
own measurement; Rust alone does not establish a concurrency improvement.
