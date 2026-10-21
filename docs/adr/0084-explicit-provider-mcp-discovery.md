# Explicit provider MCP discovery

Codex `mcpServerStatus/list` can connect to or launch configured MCP servers.
Treat discovery as a fenced, journaled command through the existing Session Host,
including during pagination. Do not poll it as an observational query or run it
automatically when opening a session. Drain refuses new discovery; stale leases
cannot admit it. DeepSeek ACP has no corresponding inventory contract here.

The shared catalog validates bounded pages, server identity, total size and
cursor continuity before replacing the view. Invalid replies retain the previous
catalog. Preserve unknown auth/runtime states and provider metadata, qualified
tool keys, complete schemas and tool discovery errors. Missing runtime status is
unavailable, not connected. The native catalog belongs to the current Host;
switching Hosts clears it, and a reply from an earlier Host UUID is refused.

The native view shares the existing scrollable interaction area so the message
editor remains reachable. It provides explicit discovery, pagination, server
selection and full loaded-inventory Copy. Visible tool names are bounded; Copy
retains all loaded schemas. Inventory describes provider configuration and does
not prove a particular conversation is connected to each server.

Configuration editing, MCP OAuth, enable/disable, resource fetching, injected
agent orchestration and external-model execution are separate future workflows.
No provider-specific credentials or user configuration enter public fixtures.
