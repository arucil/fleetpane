# ADR 0039: Preserve ACP v1 session modes

Status: implemented; local acceptance recorded separately.

Paseo replay ordinal 92 adds a current-mode status bar and a selector populated
from the agent's available modes. Fleetpane negotiates ACP protocol version 1
but previously retained only configOptions: dedicated modes returned by new or
resume were discarded, and current_mode_update while idle failed the generic
active-prompt check. A pre-fix adapter test reproduces the lost creation state.

Use the optional ACP v1 modes object as a separate typed session field. Preserve
exact IDs, names, descriptions and opaque metadata. Validate nonempty current ID
and unique nonempty choice IDs/names. A provider-selected ID absent from the
catalog is displayed verbatim; never invent a selectable choice. An unsolicited
update without a catalog records only that current ID, with an empty choice list.
Existing configOptions, including mode-category config options, remain independent.

The shared mode command accepts only a resident session and an advertised ID.
Use session/set_mode; a provider error preserves the last confirmed state and
returns the error. A successful empty object acknowledges the requested ID.
Serialize configuration/mode mutations per session, while leaving other sessions
and Stop available. ACP v1 expressly allows mode changes while idle or generating,
including a pending tool permission; neither starting a new prompt nor answering
the permission is implied. Ordinary config changes retain their idle-only contract.

Handle current_mode_update as metadata before the conversation activity check.
Keep an in-memory notification revision per resident session. A successful mode
ACK must not overwrite a provider notification received after that command was
sent. This tracks receive order only: the protocol supplies no independent mode
revision or rollback guarantee. Transport-uncertain commands retain the existing
operation receipt workflow and are never automatically resent.

Archive confirmed modes with session metadata before publishing the batch. Old
archive rows without modes remain readable, invalid rows refuse startup, and
archived mode controls require explicit successful resume. Resume replaces the
catalog with the provider's returned state, including absence; never retain stale
choices as permission to issue an RPC. No prompt, approval or mode command is
replayed after Host restart.

Native controls use the advertised choices and display current state only after
a provider notification/ACK. Preserve drafts, history and pending request identity.
Ownership, draining and command-receipt gates still apply. Mode UI does not
assert that the chosen mode grants any particular filesystem or tool permission.

This implements [the official ACP v1 mode contract](https://agentclientprotocol.com/protocol/v1/session-modes).
ACP v2 replaces the dedicated mode API with config options; v2 negotiation,
profiles, workspace defaults and permission policy presets are separate work.
No new claim about the installed DSH agent's advertised mode catalog is made.

[Local verification](../verification/2026-10-21-acp-modes.md).
