# Cross-Host archive membership and workspace filters

Accepted 2026-10-21. Extends [ADR 0071](0071-cross-host-session-directory.md).

All sessions already queries title metadata across configured Hosts, while the
shared directory query supports exact workspace and provider archive membership.
Expose those filters in the global native view and `sessions --archived` CLI.
Use the existing query, validation, bounds and opaque cursor; never filter just
the loaded first page or scan every history. Sessions means unarchived provider
membership, not an assertion that every listed runtime is active.

Search / Refresh and Enter apply the title/workspace inputs together with the
current membership. Sessions / Archived applies both inputs with the chosen
membership. The applied summary remains separate from edited inputs. Invalid
input retains the applied query and rows. A failed different query clears each
affected group's previous rows/cursor; a failed same query retains stale rows
with source opening disabled. Host buttons only choose visible groups.

An observed source now includes Host incarnation, exact thread ID and provider
archive membership. Only a fresh group's existing row may become a source.
The retained view verifies the original Host and opens history with `resume:
false`; its archive flag comes from this source rather than the runtime status
or a different view's directory query. Archived history remains read-only until
the existing explicit Restore to sessions action succeeds. Inspection, selection
and replacement never restore a session, send a turn or answer a request.

DeepSeek ACP does not advertise provider archive listing. Its durable observed
history and nonresident contexts are a different concept and stay in the normal
directory; an archive query reports the existing explicit per-Host refusal.
No provider is adapted merely because Paseo supports it. Current provider scope
is OpenCodex and DeepSeek Harness; Kimi/GLM are excluded by the user.

This change does not add persistent filters, external archive-change watching,
full-text search, atomic cross-Host observations or automatic context resume.
An externally changed archive membership can require an explicit fresh query.
See [verification](../verification/2026-10-21-directory-filters.md).
