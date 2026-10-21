# 0085: Render supplied web-search items without inferring results

Accepted 2026-10-22.

The installed Codex 0.163.0 app-server schema supplies a `webSearch` item with a
required query, optional action and optional opaque result array. Fleetpane used
to show it only as an unknown JSON card. Paseo history 453 also encountered this
gap, but its alias guessing and default completed status are not the current
app-server contract.

Project the documented search, open-page, find-in-page and other actions in the
shared session crate. Preserve exact query strings, URLs and patterns. Retain
future action payloads and show unsupported actions explicitly. A malformed known
action is an inspectable invalid-item error, rather than an unknown action.

The native card shows the query and expandable action details. Results remain
opaque JSON in the existing bounded preview; markup is literal. Absent results
are unavailable, while an empty supplied array has zero entries. Original item
fields, including unknown extensions, remain available through full Copy.

Rendering does not fetch URLs, initiate searches, infer completion, or invent
result schemas. The parent turn retains its own independent status. ACP and
external search integrations retain their existing tool presentation.
