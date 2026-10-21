# Loaded turn answer copy

Status: accepted, 2026-10-22.

Paseo replay 633 adds copying assistant messages from a conversation turn.
Fleetpane keeps the action bound to exact Host view, selected session and turn
IDs. The shared session projection returns only observed assistant text in loaded
item order, preserving whitespace and Markdown source; messages are separated by
two newlines, ACP text blocks by one. User input, tools, plans, reasoning and
non-text ACP blocks are excluded. Unknown blocks are counted, never fetched.

The turn header says **Copy loaded answer**. It computes on click against current
loaded history, so streaming and paging do not cache stale aggregate answers or
rebuild unrelated cards. The notice reports bytes, assistant-message count and
excluded blocks, and says earlier pages were not fetched. Missing/malformed/no
assistant text produces a visible error and leaves the clipboard unchanged.
Copy requires no Host/provider mutation, approval or context resume. It remains
available for failed/interrupted/archived turns when text is loaded.

This is not a complete transcript or media export. Paging exhaustion is not
inferred from the absence of a cursor: a history limit may also stop traversal.
No automatic history reads, OS clipboard persistence after process termination,
new external-model or non-Linux acceptance is claimed.
