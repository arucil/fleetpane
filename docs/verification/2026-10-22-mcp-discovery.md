# Explicit MCP discovery — 2026-10-22

Shared engine/CLI and covered Linux native paths are verified locally. The new
`session SOCKET --take-control mcp [--cursor CURSOR]` command uses the same fenced,
journaled Host admission as the native **Discover servers** action. Discovery
may launch/connect configured servers; ordinary session reads never discover.

`cargo test --workspace --locked` passed 263 tests and the desktop suite passed
33. Engine/desktop formatting and strict Clippy passed, together with Windows
GNU engine Clippy. Sixteen Python tests passed. `scripts/mcp-smoke.py` verifies
32 + 5 paging, journal admission, stale-controller rejection, drain, explicit
provider failure, malformed replies and an empty inventory. Shared catalog tests
verify atomic view retention on invalid pages, cursor loops, bounded size,
unknown states and metadata preservation.

`scripts/codex-mcp-smoke.py` ran installed Codex 0.163.0 with a temporary private
CODEX_HOME and the checked-in synthetic stdio MCP server. The actual provider
returned its `echo` tool and schema, `authStatus: unsupported`, and no runtime
status. It made zero model calls. This validates installed-provider discovery,
not OAuth or execution of a real user's configured tools.

Task-owned isolated workspace `fleetpane-mcp-20261022` exercises:

- Explicit discovery, two servers then 32 + 5 paging, independent server menus,
  unknown auth states and a visible tool discovery error. Complete native Copy
  contains 37 servers in 5,997 UTF-8 bytes, preserved qualified tool name/schema,
  unknown metadata, `nextCursor: null` and `complete: true`.
- Discovery failure keeps earlier inventory. Drain/ready controls remain
  independent. Switching Hosts resets inventory instead of exposing another
  provider's catalog. Window replacement requires fresh explicit discovery.
- The exact 41-byte `MCP 不发送这段草稿\n exact 中文  ` draft survives discovery,
  paging, Host switching and window replacement, including clipboard readback.
  Conversation Markdown remains visible alongside discovery controls.
- At 900×560, the existing interaction region scrolls while the composer remains
  visible. The server controls can be reached by scrolling; the whole catalog
  does not have to fit in the window simultaneously.
- Both original Host UUIDs and provider PIDs remain unchanged and fault-free.
  Host 1 retains the same pending approval and two admissions throughout the
  remaining Host 0 work. Final Host 0 journal count is seven (all explicit
  discovery attempts); no draft was sent. The final desktop closed normally,
  then the owned workspace and fixture processes were stopped.

[Receipt](assets/mcp-discovery/receipt.json) binds source hashes, checks and
screenshots. No macOS/Windows native, external model, OAuth, config editing,
agent orchestration or per-session connection acceptance is claimed.
