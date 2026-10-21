# Provider session model settings

Accepted 2026-10-22.

Paseo history 318–334 separates requested model choices from reported runtime
information. Its older SDK reads full rollout files and falls back to requested
configuration. Fleetpane's installed Codex 0.163.0 protocol already supplies
`Thread.model`, `modelProvider`, `reasoningEffort` and
`thread/settings/updated`. Its schema explicitly describes these as current or
persisted configuration, not per-turn execution telemetry.

Retain those optional fields in the shared Thread projection. Decode settings
events at the selected-thread boundary, validate the complete update before
mutation, and clear effort when it is null or absent. Foreign-thread events
cannot change the selection. Future effort names remain opaque strings. Bound
model, provider and effort strings to 1024, 512 and 128 bytes respectively.
Unsupported older metadata stays unavailable instead of borrowing a requested
next-message choice or catalog default.

The desktop displays and copies these provider settings above the conversation.
Next-message options remain a separate existing control. Metadata events update
the display without rebuilding retained conversation cards. ACP keeps its
advertised configuration controls and does not receive invented Codex fields.
No rollout scanning, model-account mutation or provider restart is required.

Per-turn model telemetry and reroute history are separate work. This feature
does not claim that a configured model proves which model executed a turn.
