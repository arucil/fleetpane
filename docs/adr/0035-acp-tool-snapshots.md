# ADR 0035: ACP tool snapshots and incremental presentation

Status: implemented; locally verified with a deterministic ACP provider and Linux
native UI. Paseo replay ordinal 68 merges tool calls and subsequent updates in
one card. Fleetpane already retained tool row identity, but rejected legal initial
calls without status and accepted malformed patches into visible state.

Validate and merge at the runtime boundary, independent of GPUI. A new tool call
defaults to pending status, other kind and empty content/locations. A supplied
initial null for those non-null fields is invalid. Title is required and may be
empty. Updates preserve omitted/null fields and replace supplied values in full,
including raw JSON and content/locations collections. Do not spread JSON objects,
discard false/zero/empty strings or infer tool execution from a progress update.
Unknown fields and unknown string status/kind values remain inspectable.

Validate the complete candidate before replacing or creating a card. Required
text, optional name, extension metadata, collections and location types must be
valid; location paths must be absolute. An update without a previously observed
tool call remains an explicit error. Existing item/turn/session byte and count
bounds apply. These are provider reports, not authorization or an execution API.

The common projection includes optional tool name, kind and file locations.
Native expanded details display them as text, retaining provider line values.
An empty title displays the programmatic name when available, otherwise Tool call;
copy/archive retain the original empty title. Location display does not open or
execute a file. Filesystem navigation, terminal embedding and richer diff content
remain separate work; Git Diff expansion remains deferred.

Keep the same card identity and its expansion state as patches replace details.
The existing Host journal persists the merged item before publishing it; restart
reads the same snapshot without replaying the tool. No archive schema change or
rewrite of historical items is required. Old tool snapshots without optional
metadata still decode in the common projection.

References: [ACP tool-call contract](https://agentclientprotocol.com/protocol/v1/tool-calls),
[verification](../verification/2026-10-21-acp-tools.md). The old Host binary failed
the new process scenario with `ACP status must contain a string`; the corrected
binary accepts the same ordinary initial report.
