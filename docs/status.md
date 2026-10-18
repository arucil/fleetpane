# Behavior status

Updated 2026-10-18. This is an initial foundation, not a replacement release.

| Workflow | Status | Evidence / remaining gap |
| --- | --- | --- |
| Hello before requesting the directory | verified in fixture | Real local WebSocket integration test |
| Correlated, paged directory response | verified in fixture | Client test; up to 200 entries and explicit next cursor |
| Request directory without live/timeline subscriptions | verified in fixture | Request encoding and local daemon tests |
| Unknown future notification | verified in fixture | Protocol regression test with an object payload |
| Malformed known response and daemon rejection | verified in fixture | Decode/error tests; no fallback empty directory |
| Loopback-only connection policy | verified in fixture | Non-loopback rejection test |
| Current live daemon compatibility | verified locally | Read-only CLI smoke test completed against one local daemon; exact serving source revision was not attested |
| Absolute request deadline and frame/memory bounds | partial | I/O timeouts and 4 MiB message/frame limits; no hostile-transport timing test yet |
| Native desktop, virtualized session list | missing | Planned after the protocol slice |
| Timeline, sending prompts, permissions, terminals | missing | Requires behavioral contracts and replay fixtures |
| Authenticated/remote/relay connections | missing | Current transport is local plaintext WebSocket only |
| Full runtime migration and session recovery | missing | Existing daemon remains required |
| Responsiveness and total memory under load | unmeasured | Same-machine release comparison required |

The first performance scenarios should cover cold startup, idle memory, directory
loading, selecting a session, streaming output while typing, and recovery after a
disconnect. Scale workload sizes to actual use; record revisions, build profiles,
all required processes, raw measurements and p50/p95/p99 where useful. Budgets
remain unset until there is a trustworthy baseline and a concrete requirement.
