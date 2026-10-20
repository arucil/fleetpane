# 0013: DeepSeek Harness through ACP v1

Status: implemented, 2026-10-20. This is a bounded local provider slice.

## Contract and process ownership

Installed DeepSeek Harness (DSH) 0.2.0-rc.2 exposes `dsh acp`, a JSONL/stdio ACP
v1 server. Fleetpane's Host starts this process directly and owns its stdin,
stdout and lifetime. The adapter runs inside the Host; there is no shell bridge
or intermediate adapter process. A desktop or CLI replacement does not close the
provider connection. This does not adopt existing DSH/Paseo processes.

`serve-acp` selects this transport explicitly. Initialize advertises no client
filesystem/terminal capabilities, checks protocol version 1 and session list/resume
capabilities, and sends no Codex `initialized` notification. DSH executes its own
tools and resolves credentials through its own configuration. The existing
Codex transport remains separately selected by `serve`.

The toolkit-independent adapter maps ACP into the shared conversation/command
projection. Internal names such as `thread/read` and `turn/start` are Fleetpane
commands, not claims that DSH implements Codex RPCs. Both GUI and CLI use the same
commands. Unavailable operations return explicit errors.

## Sessions, directory and configuration

Map create/list/resume to `session/new`, `session/list` and `session/resume`.
**DSH lists inactive sessions only**, excluding its resident sessions, even after a
completed prompt. Fleetpane merges Host-owned resident metadata with inactive
provider pages, deduplicates IDs, and returns at most 100 rows per page. Resident
rows come first on a refresh. Overflow uses an opaque cursor containing remaining
metadata IDs and the upstream continuation, bound to this Host generation and
workspace query. Remaining rows precede the next upstream page; nothing is lost
when 64 resident rows meet a full 100-row provider page. Metadata remains bounded.
Provider cursors are not passed directly to clients. Exact workspace filters are
supported; title filtering and renaming are unavailable in this DSH slice.

Use the complete `configOptions` snapshots returned by DSH. Select menus preserve
its opaque values, including grouped model routes. Setting an option uses
`session/set_config_option`, affects the session, and must settle before another
configuration change or prompt. This differs from Codex's next-turn overrides.
Unknown option types remain disabled; unsupported values fail explicitly.

## Activity, settlement and permission

A `session/prompt` response arrives only when the prompt settles. Waiting for it
before completing the shared Send command would block the GUI's poll/cancel and
permission controls. After writing the request, the Host therefore returns an
explicit `admission: host-dispatched` receipt. This establishes dispatch, not
provider acceptance or model success. The later response determines completed,
failed or interrupted state. The mutation UUID is retained to prevent an uncertain
send from being repeated by a replacement controller.

Observe `session/update` messages: assistant text/content parts, thoughts, tool
calls and partial tool updates. DSH emits committed semantic chunks, so this is
incremental activity rather than a claim of raw token streaming. Keep message IDs,
merge text chunks and partial tool fields, and retain unknown activity as
inspectable items. Tool text and images use existing detail renderers; terminal
and diff references remain inspectable raw blocks. Settlement emits metadata only,
keeping previously emitted items rather than duplicating a potentially large turn.

Cancel sends the `session/cancel` notification. A Stop receipt does not manufacture
an interrupted result; the provider's response settles the turn. Exact pending
permission requests retain their ACP IDs and offered options. GUI/CLI answers
select an offered option ID; no approval is inferred from its label. Controller
leases fence stale answers. Pending requests survive controller replacement and
are cleared when explicitly answered or their prompt settles. DSH does not expose
Codex-style elicitation through this ACP surface.

## History and limits

DSH's resume restores model context but does not replay transcript updates.
Fleetpane retains activity observed by the running Host and labels the view
`acp-observed`, with a native warning. A new Host can list and resume saved DSH
sessions, but begins with an empty observed transcript. It never parses DSH's
private session files to manufacture replay. Closing the GUI retains Host history;
restarting the Host does not. This follows the installed protocol, also documented
in [DSH's ACP reference](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/acp/acp/README.md).

The initial adapter retains at most 64 resident sessions, 5,000 metadata summaries,
160 pending RPCs, 64 permission requests, 500 entries/turns and 2 MiB per observed
conversation, with an 8 MiB combined conversation/metadata budget. Overflow faults
explicitly; it never silently discards history. Existing Host frame/event bounds,
32-query budget and 128-mutation lifetime journal still apply.

The 2026-10-20 history extension separates replay availability from paging:
`historyMode: acp-observed` remains truthful, while `historyPaging: true` allows
the common desktop to load observations incrementally. `thread/read` honors
`includeTurns`; resident resume honors `excludeTurns`. Turn pages return metadata
only, and item pages return at most 16 entries within the exact 256 KiB encoded
provider-reply budget, including identity and continuation. Cursors bind Host
generation, session, turn scope and immutable item/turn anchors. New appends do
not shift the continuation. Invalid flags, scopes and cursors fail explicitly.

A full read exceeding that budget returns a command error without faulting the
healthy Host. A single accumulated item can still exceed the frame budget;
windowing and durable observations are not implemented. Snapshot watermarks keep
older pages from reverting newer live ACP item replacements. See the
[history verification](../verification/2026-10-20-acp-history.md).
Capacity exhaustion is a prototype limitation, not an instruction to restart an
active Host. Durable recovery, Host version drain/routing, account setup UI,
attachment send, remote transports and arbitrary ACP-agent compatibility remain
future work.

See [verification](../verification/2026-10-20-deepseek-acp.md).
