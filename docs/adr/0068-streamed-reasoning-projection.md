# 0068 — Preserve indexed Codex reasoning deltas

Accepted 2026-10-21 for the current bounded conversation projection.

The final reasoning item already renders, but the shared decoder ignored
`item/reasoning/summaryPartAdded`, `summaryTextDelta` and `textDelta`.
An empty started item therefore stayed invisible until its completed snapshot.
This was reproduced with two failing regressions and an unchanged native binary.

Apply the three notifications to the exact thread, turn and reasoning item.
Summary and raw content use separate indexed string arrays. Keep empty slots,
Unicode, whitespace and arrival order; adding an existing summary slot never
erases text. Negative, malformed, unrelated-item and missing-target inputs fail
explicitly. Sparse indices and the prospective serialized change are bounded by
the existing 2 MiB conversation budget before modifying the item.

History replay gives these notifications the same per-item snapshot boundaries
as other item updates. A page that already contains a delta covers that delta;
loading earlier items must not cover a later update to a previously loaded item.
The existing native row builder reveals nonempty reasoning and updates its
retained card. Details stay literal text, bounded in the existing preview, and
Copy retains the full reasoning fields. Final snapshots replace the same item.

Do not derive turn completion, runtime ownership or cancellation settlement from
reasoning text, another tool item or a permission event. Existing Host lifecycle
and exact-turn Stop behavior remain the authorities. This change adds no model
access, hidden reasoning extraction, activity export or reasoning animation.

[Verification](../verification/2026-10-21-streamed-reasoning.md) separates unit
projection, captured protocol events and Linux native fixture acceptance.
