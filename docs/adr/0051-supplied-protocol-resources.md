# 0051 — Render supplied protocol resources

Status: accepted for local rendering, 2026-10-21.

Paseo history 151–156 develops common tool events, saved-session discovery,
result previews and task lists. Its recursive property/status heuristics are not
a reliable contract for arbitrary provider JSON. Fleetpane already has explicit
ACP tool and MCP result projections, but embedded resources and resource links
fell through to raw JSON. The initial regression reproduced that gap.

## Decision

Add toolkit-independent Resource and ResourceLink parts to the session crate,
using the [ACP content contract](https://agentclientprotocol.com/protocol/v1/content)
and [MCP resource contents](https://modelcontextprotocol.io/specification/2025-11-25/server/resources).
Decode explicit `resource` and `resource_link` blocks in ACP messages, ACP tool
content and MCP results. Preserve empty text, zero sizes, exact whitespace and
both text/blob fields when supplied. Raw Item serialization and full-item Copy
retain annotations, extensions and unknown content. Missing required fields or
embedded contents produce a visible projection error; future block types remain
inspectable JSON. A completed tool with malformed display content retains its
reported status; rendering failure is not evidence of execution failure.

The native card shows URI and supplied metadata. Text previews are bounded to
8 KiB and 64 newline separators on UTF-8 boundaries, with an explicit truncation
notice. Metadata previews have a separate 512-byte bound. MIME `text/markdown`
selects the existing Kit Markdown renderer; other text stays literal. Copy text,
Copy URI and Copy base64 use complete supplied strings. Empty text is explicit.
Binary contents show their encoded length and can be copied without decoding.

Resource URIs are opaque identifiers, including provider virtual resources.
Rendering does not resolve, read or fetch them, and does not infer a workspace
file action from `file://`. Existing explicit conversation-file navigation stays
independent. Resource reads, binary previews, audio playback, attachment creation
and a complete MCP resource browser are outside this slice.

## Verification

See [resource acceptance](../verification/2026-10-21-resources.md). This includes
the failing pre-fix assertion, typed-content tests, real Host transports using
synthetic ACP/MCP providers, exact ACP archive recovery, and native rendering and
clipboard checks. No new external OpenCodex/DeepSeek model, remote transport,
cross-platform or concurrency-performance acceptance is claimed.
