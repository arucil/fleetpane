# ADR 0034: ACP session information is independent of prompt activity

Status: implemented for owned resident ACP sessions; verified locally with a
private fixture and Linux native UI. Provider-driven title updates are supported;
this does not add a provider rename command.

ACP v1 defines `session_info_update` for optional title, last-activity timestamp
and opaque `_meta`. The adapter previously required a running Host prompt for
this update. An idle update failed the provider boundary; an active update was
inserted as unknown conversation content and split anonymous text segments.
Both failures were reproduced by regression tests before the production change.

Handle information before the prompt-only branch. Apply only supplied fields;
`null` clears title and timestamp. Validate all field types before changing either
metadata index. Store the latest supplied `_meta` object intact, without merging
or interpreting vendor keys; an omitted `_meta` leaves the previous object, and
explicit null removes it. Timestamp strings remain provider-owned ISO 8601 text;
Fleetpane does not convert them to Codex epoch values or use them for sorting.

Publish a complete `thread/acpSessionInfo/updated` snapshot through the shared
projection. The selected conversation and loaded directory rows receive the same
title. A changed name marks directory results for explicit refresh. There is no
new history item, prompt, permission answer or anonymous-stream boundary. A list
response cannot overwrite a resident session's live information; provider list
metadata remains authoritative for inactive sessions.

Persist the session row in the existing journal transaction before publishing
the event. Old rows without the new optional fields remain readable without a
schema change. Commit failure fences the Host and publishes no information event.
Restored sessions remain archived; unfinished turns remain unknown and explicit
context resume is required. Updates targeting unowned or inactive archived
sessions still fail. Notifications preceding creation/resume ownership, provider
transcript import and process takeover need separate protocol contracts.

References: [ACP v1 schema](https://agentclientprotocol.com/protocol/v1/schema),
[local verification](../verification/2026-10-21-acp-info.md). Paseo replay ordinal
67 motivates keeping lifecycle/status updates distinct from conversation output;
its deletion delay and loose runtime casts are not reused.
