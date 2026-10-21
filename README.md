# Fleetpane

A native Rust foundation for managing fleets of coding-agent sessions.

Fleetpane is an independent project. The first implementation includes a GPUI Kit
session directory, a read-only Paseo CLI, and Host-backed Codex/DeepSeek ACP conversations.
The target is donut-paseo's useful workflows with OpenCodex and DeepSeek Harness,
not an identical port or every provider. Kimi/GLM are outside the current scope;
[the parity inventory](docs/parity.md)
records what remains. This prototype is not a replacement release.
The Unix Session Host keeps its provider connection alive across control-client replacement. These are
foundations, not a usable replacement release. No performance gain is claimed.

See the [product requirements](docs/prd.md), [behavior status](docs/status.md), and
[Session Host architecture](docs/adr/0002-preserve-runtimes-across-control-daemon-upgrades.md).
The [Paseo chronological review](docs/reference/paseo-replay.md) follows both
lineages from their first commit, records each inspected batch and maps the
evolution to Fleetpane's missing workflows. Recent deltas are supplemental.

Provider-reported command actions preserve all reads, listings and searches with
explicit bounded file navigation and complete original Copy. At compact window
heights, **Session options** holds optional controls while pending requests and
the composer remain reachable. [Verification](docs/verification/2026-10-22-command-actions.md).

Supplied Codex web-search items show the exact query and expandable action/results
without fetching URLs. Future fields remain copyable; missing results stay
unavailable. [Verification](docs/verification/2026-10-22-web-search.md).

**MCP servers** exposes explicit Codex provider inventory discovery, paged server
selection, auth/runtime values, tool discovery errors and complete loaded schema
Copy. Discovery may start configured servers and requires control; it is never
triggered by ordinary session reads. CLI:
`fleetpane-host session SOCKET --take-control mcp [--cursor CURSOR]`.
Inventory describes Host provider configuration; active conversation connections,
config editing and OAuth are separate. [Verification](docs/verification/2026-10-22-mcp-discovery.md).

**Hosts** checks each configured local endpoint without taking control. Cards
show the last check time, protocol, process/controller identity, drain/fault state
and activity counts, with **Check**, **Check all** and **Copy diagnostics**.
Checks omit provider initialization data and request payloads; local paths, process
identities and error messages remain in copied JSON. CLI:
`fleetpane-host host-diagnostics CONFIG.json [HOST_NAME]`. Results describe an
observation, not permission to stop a Host. [Verification](docs/verification/2026-10-22-host-diagnostics.md).

**Attention** filters pending requests by Host and kind. With the queue focused,
Up/Down and Home/End select, Enter opens the original source, Ctrl+C copies the
original request and Ctrl+R refreshes. A resolved or changed selection clears
without choosing another request. Opening retains explicit takeover and original
review controls. [Local verification](docs/verification/2026-10-21-attention-navigation.md).

**All sessions** can search titles across configured Hosts and filter by exact
workspace directory or **Sessions / Archived** provider membership. CLI uses
`fleetpane-host sessions CONFIG.json --archived [--cwd DIRECTORY] [--search TITLE]`.
Opening archived history retains its original Host and read-only state until
explicitly restored. DeepSeek's unsupported provider archive listing stays
visible. [Verification](docs/verification/2026-10-21-directory-filters.md).

Native conversation identities now keep turn status and item cards distinct,
including provider IDs containing separators. Synthetic before/after pagination,
independent details and exact clipboard checks have
[separate evidence](docs/verification/2026-10-21-row-identity.md).

**Hide sessions / Show sessions** toggles the current Host’s directory with Ctrl+B (Cmd+B on macOS). The conversation, drafts and pending requests stay open. Visibility is local to the window and Host; [Linux acceptance](docs/verification/2026-10-22-sidebar-toggle.md).

**Switch session** opens a bounded cross-Host search palette with Ctrl+K
(Cmd+K on macOS). Search title, Host, cwd or ID, use arrows and Enter to open
saved history, and Esc to return to the previous input. Omitted rows and missing
pages are explicit; **All sessions** provides paging and archived history.
Selection preserves explicit takeover and drafts. [Local verification](docs/verification/2026-10-22-session-switcher.md).

## Try the native desktop

On Unix, **Workspaces → Import from Paseo** previews a local Paseo home, saves a
selected workspace and its session identities, then **Copy workspace history**
copies supported Codex/DeepSeek Harness transcripts. **Sessions → View history**
browses independent private snapshots with Markdown, tools and supplied images.
DeepSeek histories support v3/v4, plain or Zstandard, and select the latest
canonical generation. Unsupported/corrupt newer generations and mixed encodings
show an error; older generations are not substituted. Existing imported snapshots
retain their original version. [v4 verification](docs/verification/2026-10-21-paseo-dsh-v4.md).
The destination Host can be offline. Missing directories and unsupported histories
remain explicit; repeated imports keep the first snapshot. Complete saved Codex legacy histories can also create an independent context
fork; original running agents retain their ownership. See [migration behavior and limits](docs/adr/0059-offline-paseo-workspace-history-import.md).

For multiple workspaces, preview, filter by name/path and select individual rows
or **Select matching**, then **Migrate selected + history**. Progress and each
workspace's Details show failures and warnings. **Stop after current copy** keeps
completed work; repeat the selection to continue. Selection includes hidden rows
until cleared. [Batch behavior and limits](docs/adr/0060-bulk-paseo-workspace-history-migration.md).

**Saved migration attempts** retains progress and workspace diagnostics across
app restarts. Open/refresh reports without copying; **Retry saved selection**
creates a new attempt after checking the saved source revisions. Existing
identities and complete snapshots are reused. An interrupted attempt stays
unconfirmed; another active worker prevents retry. Selection check completion
does not imply every history succeeded. [Recovery contract](docs/adr/0064-durable-paseo-batch-receipts.md).

CLI `batch` and `retry-batch` handle Ctrl+C/SIGINT and SIGTERM as **Stop after
current copy**. They finish the current unit, save a stopped report and print one
complete JSON result (`cancelled: true`), then exit successfully. Repeated signals
keep waiting for the same copy. SIGKILL, power loss and storage failures still
require interrupted-report inspection. This signal handling does not apply to
standalone history-copy commands. [Signal verification](docs/verification/2026-10-21-paseo-signals.md).

After an interrupted copy, **Check temporary files** shows the unused file count
and bytes; **Clear unused temporary files** removes that version's scratch files.
Both actions refuse an active history copy or context export. Source transcripts,
published histories and migration reports are retained. Older temporary files
have no shared ownership lock and are reported without deletion. Cleanup is
explicit; opening the app never starts it. [Cleanup contract and verification](docs/verification/2026-10-21-paseo-scratch.md).

For supported Codex histories, choose **Continue in Codex…**, take control of the
target Host, then **Fork saved context**. This uses a private raw snapshot to
create a new thread with the original context; it never resumes the Paseo thread.
**Check saved fork status** recovers its result after reopening the app. Dispatch
is allowed once per saved import, even after an uncertain result. DeepSeek,
incomplete snapshots and Codex paginated histories with a separate thread store
remain unsupported for continuation. A prepared binding is pinned to its Host and
admission generation; stale preparations are refused. Use **Reprepare unsent context**
to explicitly replace an unsent binding after its Host changes. This does not
submit a fork. CLI: `paseo-import reprepare-fork STORE SESSION_UUID EXPECTED_OPERATION_UUID`.
Already attempted bindings, including unknown results, cannot be replaced. [Contract](docs/adr/0061-independent-codex-history-continuation.md).

The same engine is available from `fleetpane-host paseo-import`:

```sh
fleetpane-host paseo-import preview /absolute/paseo-home
# Use the selected candidate's revision and source.workspaceId from preview:
fleetpane-host paseo-import workspace /absolute/controller /absolute/paseo-home WORKSPACE_PREVIEW_REVISION /absolute/hosts.json HOST_NAME SOURCE_WORKSPACE_ID
fleetpane-host paseo-import list /absolute/controller
fleetpane-host paseo-import copy-workspace-history /absolute/controller IMPORTED_WORKSPACE_UUID /absolute/codex-home /absolute/dsh-home
fleetpane-host paseo-import sessions /absolute/controller IMPORTED_WORKSPACE_UUID
fleetpane-host paseo-import history /absolute/controller IMPORTED_SESSION_UUID 0
fleetpane-host paseo-import prepare-fork /absolute/controller IMPORTED_SESSION_UUID
fleetpane-host paseo-import fork /absolute/controller IMPORTED_SESSION_UUID --take-control
fleetpane-host paseo-import fork-status /absolute/controller IMPORTED_SESSION_UUID
# Omit IDs to plan all workspaces; otherwise list the selected source IDs.
umask 077
fleetpane-host paseo-import plan /absolute/paseo-home /absolute/hosts.json HOST_NAME /absolute/codex-home /absolute/dsh-home > /absolute/plan.json
fleetpane-host paseo-import batch /absolute/controller /absolute/plan.json > /absolute/result.json
fleetpane-host paseo-import batches /absolute/controller
fleetpane-host paseo-import batch-status /absolute/controller BATCH_UUID
fleetpane-host paseo-import batch-results /absolute/controller BATCH_UUID
fleetpane-host paseo-import retry-batch /absolute/controller BATCH_UUID
fleetpane-host paseo-import scratch-status /absolute/controller
fleetpane-host paseo-import clean-scratch /absolute/controller
```

`history` returns a `next` cursor for the next page. `source-record STORE SESSION_UUID
RECORD_NUMBER` reads the preserved JSON record (numbering starts at 1). Default
desktop roots respect `PASEO_HOME`, `CODEX_HOME` and `DSH_HOME`; edit the path fields
for another local export. Paths and full transcripts are private data; keep CLI
output outside public logs. [Verification](docs/verification/2026-10-21-paseo-import.md).

On Linux with Vulkan, fontconfig, FreeType, xkbcommon, and X11 or Wayland development
libraries installed:

```sh
cargo run --manifest-path apps/desktop/Cargo.toml --locked -- --demo
cargo run --manifest-path apps/desktop/Cargo.toml --locked -- 127.0.0.1:6767
cargo run --manifest-path apps/desktop/Cargo.toml --locked -- --host /tmp/fleetpane-example/control.sock
```

`--demo` uses 10,000 synthetic sessions without connecting to a daemon. The live
mode reads the existing Paseo directory one page at a time, with an explicit
5,000-session cap. Select with the mouse or ↑/↓, Home/End; Ctrl+R refreshes the
live directory; Ctrl+C copies the selected session ID. Kit buttons provide
refresh, pagination and copy actions.

On Unix, **Appearance** selects System, Light or Dark for this device. System
follows OS appearance changes; a choice applies after its local save succeeds.
Storage failures and stale writes show an error with **Reload appearance**.
The default preferences directory is `$XDG_CONFIG_HOME/fleetpane/preferences`
or `$HOME/.config/fleetpane/preferences` on Linux, and
`$HOME/Library/Application Support/Fleetpane/preferences` on macOS.
Use `--preferences-dir /absolute/private/directory` for an isolated profile.
These settings are independent of the selected Host; normal close waits for
pending theme and draft saves. [Verified behavior and limits](docs/verification/2026-10-21-appearance.md).

The shared CLI can inspect or change a preference without connecting to a Host:

```sh
fleetpane-host appearance /absolute/private/directory get
fleetpane-host appearance /absolute/private/directory set 0 dark
```

The set command requires the revision returned by get. Another writer's change
requires an explicit reload; it is never silently overwritten.

The conversation composer keeps Enter/Shift+Enter for newlines; Ctrl+Enter
(Cmd+Enter on macOS) invokes Send, or Steer for a running Codex turn, with the
same availability checks as the buttons. [Local keyboard evidence](docs/verification/2026-10-22-composer-shortcut.md).

`--host` connects to an independently running Fleetpane Host (see below). Enter an
absolute workspace directory, create a session, and send a multiline text prompt.
It supports bounded conversation history with rendered Markdown, command/MCP
cards, file diffs and image previews, streamed assistant text,
Stop turn, durable per-session text drafts, provider task lists, and explicit command/file
approval decisions. Structured Codex questions have a paged answer form with
single choice, optional notes, free text and masked sensitive input. Answers stay
with their request across session switches and reconnects while the window stays
open; **Submit answers** requires an answer to every question. Reopening the
desktop requires **Take control** when the Host already has an owner; this fences the previous controller. Closing the desktop
leaves the Host and its provider running. Reconnect reloads the selected history;
expired event cursors require an explicit reload. Uncertain sends retain their
operation UUID and block further commands until their status can be reconciled.

New threads request Codex `historyMode: paginated` so nested code-mode command
items survive history reloads. An isolated OpenCodex turn retained all three
commands, including their IDs, output and exit status, after native reload and
after restarting its completed Host/provider. Existing legacy threads keep their
mode and show a warning: command items already discarded by Codex cannot be
restored by this change. A new empty thread is not persisted until its first user
turn; reading it earlier can return `list_turns is not supported yet`. That empty
thread response was previously mistaken for missing paginated support.
See the [history decision](docs/adr/0010-durable-codex-tool-history.md) and
[verification](docs/verification/2026-10-19-durable-tool-history.md).

For paginated threads the desktop reads metadata first, then the latest turn and
up to 16 of its entries. **Load earlier messages** continues within that turn and
then into earlier turns, retaining the current scroll position. The button shows
the loaded count and the beginning/limit state. History reads and live text use
their response boundaries to avoid duplicate deltas or reordered older items.
[History pagination verification](docs/verification/2026-10-19-history-pagination.md)
covers a 1,200-command conversation, scroll anchors and simultaneous streaming.
**All sessions** observes all configured Hosts without taking control. It groups
metadata by Host, supports title search (Search / Refresh or Enter), Host filters
and per-Host Load more. Failed Hosts remain visible; stale rows cannot open.
DeepSeek ACP does not support server-side title search and reports that limitation.
Open session selects its exact Host; take control explicitly if needed, then
Open selected session to load history. Existing drafts stay with their original
Host/session. No context resumes or messages are sent by this navigation.
[Cross-Host verification and limits](docs/verification/2026-10-21-fleet-directory.md).

```sh
fleetpane-host sessions /absolute/hosts.json --search TITLE
fleetpane-host sessions /absolute/hosts.json --cwd /absolute/workspace
# Continue a report's page with the same filters and original Host identity:
fleetpane-host sessions /absolute/hosts.json --host NAME --cursor CURSOR --host-id UUID
```

The CLI emits per-Host errors in its JSON report and does not claim control.
Hosts need an existing controller lease for provider queries.

The Host session directory loads 100 metadata rows per page. **Load more sessions**
continues up to 5,000 sessions / 8 MiB. Title and exact workspace filters run on
the provider's complete index; Apply or Enter replaces the result set only after
success. Refresh and Clear leave the selected conversation and its draft open.
**Rename session** supports Save name or Enter, and Cancel; after success the
current query refreshes so renamed sessions enter or leave matching results.
[Session directory verification](docs/verification/2026-10-19-session-directory.md)
records the native and real Codex checks.

**Next message** offers the provider's models and their supported reasoning
efforts. Choosing a model selects its declared default effort; another effort can
be selected from its menu. Choices stay with each session across switches and
reconnects while the window stays open. **Reload models** refreshes the paged
catalog. Removed choices stay visible and block sending until corrected.
**Use session settings** omits both overrides and inherits the provider's current
configuration; it does not undo settings applied by earlier turns. Catalog
failure leaves session defaults available and does not discard drafts. Capability
lists do not establish account access to a model.
[Model-option verification](docs/verification/2026-10-19-model-options.md) records
native interactions and real Codex parameter readback using a local model fixture.
[Live OpenCodex verification](docs/verification/2026-10-19-opencodex-live.md)
adds three successful external model/tool turns through OpenCodex 2.73.0,
native model/effort selection, Markdown/diff rendering and desktop replacement
during command execution. DeepSeek Harness ACP has its own bounded adapter,
[decision](docs/adr/0013-deepseek-acp-provider.md) and
[verification](docs/verification/2026-10-20-deepseek-acp.md).

ACP v1 sessions that advertise modes now expose **Mode** beside their existing
configuration options. Choices come from that session's provider; mode changes
are allowed during a reply or tool permission wait. Rejected changes keep the
confirmed mode visible. The CLI equivalent is `session SOCKET --take-control
mode THREAD MODE_ID`. Archived sessions must resume first. This is separate from
profiles and permission policy presets; see [the contract and limits](docs/adr/0039-acp-v1-session-modes.md).

Fleetpane saves observed ACP messages and tools in its private Host journal.
After restarting that Host, select an **archived** session to browse its saved
history; **Resume context** must succeed before sending. Unfinished turns show
**unknown**, with no automatic prompt or permission replay. Existing DSH messages
that Fleetpane never observed are not replayed into the Host journal. The separate
Paseo importer can copy available Codex/DSH logs into its archive browser. See the
[decision](docs/adr/0031-durable-observed-acp-history.md) and
[verification](docs/verification/2026-10-21-acp-archive.md).

ACP agent title updates now reach the directory and open conversation even while
idle or waiting for a tool permission. Session information is saved before it is
published, without adding conversation items; null clears the supplied title.
[Verified behavior and limits](docs/verification/2026-10-21-acp-info.md).

ACP tool calls default to pending when the agent omits status. Expanded cards
show tool names, kinds and file locations; incremental updates retain the card
and replace its details. [Verification](docs/verification/2026-10-21-acp-tools.md).

ACP/MCP embedded resources and resource links have cards with supplied URI,
metadata and bounded text previews. Markdown MIME uses the Kit renderer;
Copy text, Copy URI and Copy base64 retain complete supplied data. Empty and
malformed content are explicit. Resource URIs are not fetched or opened.
[Local acceptance](docs/verification/2026-10-21-resources.md) covers native cards,
exact clipboard output and ACP archive reload with synthetic providers.

Stop cancels outstanding ACP permission requests immediately, including requests
that arrive while cancellation is pending. The turn remains active and the UI
shows **Stopping…** until the agent confirms settlement. Replacing the window
preserves this waiting state and its draft without sending another cancellation.
[Verification](docs/verification/2026-10-21-acp-cancel.md).

Stop progress now belongs to the Session Host for both Codex and ACP. An interrupt
acknowledgement and the exact original turn's terminal notification are separate:
Send and follow-ups stay blocked until both arrive. The desktop remains usable
for other sessions while waiting. Rejection shows **Retry Stop**; retries are
explicit, and a lost provider connection shows unknown. Older Hosts without
progress support refuse Stop. This does not prove background resources exited or
implement live Host replacement. [Verification](docs/verification/2026-10-21-stop-progress.md).

The composer now shows observed Agent activity independently of pending UI
actions: working, awaiting your response, stopping, synchronizing or unavailable.
It remains visible while details scroll. No running turn observed does not prove
background resources have exited. [Local verification and limits](docs/verification/2026-10-21-agent-progress.md).

New session remembers up to three recent directories per Host endpoint. The menu
prefills the workspace input; **New session** remains an explicit action. Suggestions
persist across windows, support Reload/Forget and report save failures separately
from a successful session creation. [Verification](docs/verification/2026-10-21-recent-directories.md).

**Loaded session directories** also suggests up to 24 exact directories from the
selected Host's currently loaded, filtered sessions. Choosing a path fills the
input; **New session** creates it explicitly. The menu states its loaded-page
scope and keeps persistent recents separate.
[Verification](docs/verification/2026-10-22-loaded-directories.md).

Tool permissions preview the operation carried by the request, including standalone
requests without a prior tool event. Request details take precedence over an older
tool card; raw input, content and file locations can be reviewed and the complete
request copied. A bounded interaction area keeps the draft and Stop/Send accessible
in small windows. [Verification](docs/verification/2026-10-21-acp-review.md).

Complete terminal interaction/IME, remote attachment upload and broader
provider integration acceptance remain pending. The Host
view retains at most 500 items, 500 turns and 2 MiB; reaching that bound stops
older loads, preserves current content and offers Reload for recent history.
Legacy and explicit CLI full reads remain bounded. A single item or a 16-entry
page can still exceed the transport's 256 KiB provider-frame cap. Item previews
limit text sections visibly to 16 KiB; Copy retains the full item within those
transport bounds. Byte-aware pages and a history window with eviction remain
necessary for complete traversal of arbitrarily large conversations.

Codex reasoning now updates during generation from indexed summary and raw-text
events. **Show details** expands literal text; **Copy** retains the complete
reasoning fields. Empty items stay hidden until content arrives. History page
boundaries avoid appending already-snapshotted deltas twice.
[Reasoning verification](docs/verification/2026-10-21-streamed-reasoning.md)
covers Linux native live updates, exact copy/reload, Stop and session isolation.

[Rich-rendering checks](docs/verification/2026-10-19-rich-rendering.md) cover native
Markdown, command status/output, MCP text/image results, supplied file diffs, local
and data URL images, failure states and empty reasoning. The final build also
reopened the real saved conversation used in the earlier diagnosis. Tool details
scroll within a bounded area; long commands use a first-line summary. Remote
images and a full-size image viewer remain unverified or unimplemented.

The desktop pins GPUI Kit 0.7.3 and its matching GPUI engine in a separate manifest
and lockfile; use `--locked` to preserve the tested native dependency versions.
The [Component patch](apps/desktop/vendor/README.md) fixes diff-list overdraw in
that version without changing the engine.
See [the toolkit decision](docs/adr/0004-gpui-kit-desktop.md).
Linux rendered-window verification is recorded; macOS/Windows desktop behavior,
IME, accessibility and complete raw-terminal interaction remain unverified.

## Try the Session Host

This Unix prototype starts a **new** provider runtime; it cannot take over running
Paseo sessions. Run it independently of any replaceable control client:

```sh
cargo run -p fleetpane-host -- serve /tmp/fleetpane-example/control.sock "$PWD"
```

The parent directory is created privately (0700); an existing public directory or
socket is rejected. The provider defaults to `codex app-server --stdio`, inheriting
the host's environment and Codex configuration. Ctrl+C stops the host and provider.
Use a separate `CODEX_HOME` for experiments.

A different terminal can read its state without controlling the provider:

```sh
printf '%s\n' '{"command":"snapshot"}' | cargo run -p fleetpane-host -- request /tmp/fleetpane-example/control.sock
```

Claims and mutations use the versioned protocol in `fleetpane-runtime`. The
shared `fleetpane-session` adapter is also available from the CLI:

```sh
cargo run -p fleetpane-host -- session /tmp/fleetpane-example/control.sock create "$PWD"
cargo run -p fleetpane-host -- session /tmp/fleetpane-example/control.sock --take-control list
cargo run -p fleetpane-host -- session /tmp/fleetpane-example/control.sock --take-control open THREAD_ID
cargo run -p fleetpane-host -- session /tmp/fleetpane-example/control.sock --take-control inspect THREAD_ID --resume
cargo run -p fleetpane-host -- session /tmp/fleetpane-example/control.sock --take-control history-turns THREAD_ID
cargo run -p fleetpane-host -- session /tmp/fleetpane-example/control.sock --take-control history-items THREAD_ID TURN_ID
cargo run -p fleetpane-host -- session /tmp/fleetpane-example/control.sock --take-control send THREAD_ID "Hello"
cargo run -p fleetpane-host -- session /tmp/fleetpane-example/control.sock --take-control models
# Choose MODEL and EFFORT from the returned provider capabilities:
cargo run -p fleetpane-host -- session /tmp/fleetpane-example/control.sock --take-control send THREAD_ID "Hello" --model MODEL --effort EFFORT
```

To observe an exact turn, use **wait** with its session and turn IDs:

```sh
cargo run -p fleetpane-host -- session /tmp/fleetpane-example/control.sock wait THREAD_ID TURN_ID --timeout-ms 30000
python3 scripts/wait-smoke.py --real-codex
```

Wait returns `finished` (including its actual status/error/stop reason), `unknown`,
`attention` (exact pending requests) or `timed_out`. It reads an existing controller
lease and keeps that controller; omit `--take-control`. Timeout/Ctrl+C stops only
observation. It never answers, sends or stops a turn. Host replacement/fault and
expired cursors are explicit errors. [Contract and limits](docs/adr/0047-observe-an-exact-turn.md),
[local protocol evidence](docs/verification/2026-10-21-wait-turn.md).

Session commands that acquire control use a separate controller;
`--take-control` explicitly fences the previous owner. Do not use it against a
Host another window should control.
The [process-level smoke test](scripts/runtime-smoke.py) is a complete runnable
handoff example with a synthetic provider. Adding `--codex` exercises a real,
isolated Codex `command/exec` without sending model requests or using saved sessions.
Directory, model and history reads use a separate observational channel with at
most 32 requests in flight; repeated reads do not consume mutation admission.
Mutation UUIDs and RPC results now persist in `SOCKET.journal/operations.sqlite3`.
Settled records leave the bounded RAM cache without losing their deduplication
identity: the former 128-operation lifetime cap is removed. Read-only
`fleetpane-host operations SOCKET [--cursor CURSOR]` lists 32 receipt summaries
per page; `fleetpane-host operation SOCKET UUID` reads the exact request/result.
On restart, incomplete admissions become unknown and are never automatically
replayed. The native window can reconnect, check an old UUID, and explicitly
**Continue without resending** before doing other work. Protocol v3 requires
updating the disposable Host and clients together. Storage has a 256 MiB page
budget and fails closed; there is no automatic receipt deletion. This does not
restore an in-flight provider/tool. Retained observed ACP history has its own
[archive contract](docs/adr/0031-durable-observed-acp-history.md). See
[durable receipt semantics](docs/adr/0015-durable-operation-receipts.md) and
[local verification](docs/verification/2026-10-20-durable-journal.md).

An owning window can select **Drain Host** to pause new mutations on that
endpoint while existing turns, permission answers, history reads and Stop
continue. The banner shows observed active turns, pending answers, in-flight RPCs
and uncertain receipts. Switch to another configured Host for new work; select
**Accept new work** to reopen the old endpoint explicitly. The CLI equivalent is:

```sh
fleetpane-host session SOCKET --take-control drain
fleetpane-host session SOCKET --take-control ready
```

Drain uses a durable generation fence, separate from controller ownership. It
survives client replacement and Host restart. Journal schema 1 upgrades to 2;
an older Host cannot reopen the upgraded journal. Missing saved admission state
rejects startup. This is the first part of
[overlapping Host versions and drain](docs/adr/0002-preserve-runtimes-across-control-daemon-upgrades.md#updating-the-session-host-itself).
Automatic routing, supervision, provider restoration and resource-complete
retirement remain future work. Zero observed turns does not prove tools or PTYs
have exited, so draining never automatically terminates a Host. See
[admission semantics](docs/adr/0016-persistent-host-admission-drain.md) and
[native/process verification](docs/verification/2026-10-20-host-drain.md).

## Try DeepSeek Harness ACP

With `dsh` installed and its credentials configured, start an independent Host:

```sh
cargo run -p fleetpane-host -- serve-acp /tmp/fleetpane-deepseek/control.sock "$PWD"
cargo run --manifest-path apps/desktop/Cargo.toml --locked -- --host /tmp/fleetpane-deepseek/control.sock
```

The default provider is `dsh acp`. It inherits the launching environment, including
`DSH_HOME`. Use a separate home and disposable workspace for experiments; configure
credentials there through DSH or its supported environment variables. Existing
DSH processes and sessions are not adopted or restarted.

Connections open read-only. Use **Take control**, then create or select a session
and choose the provider's model/reasoning options under
**Session configuration**, and send text. These are persistent session options,
not next-message overrides. Text/thought/tool updates, Markdown, tool details,
Stop and exact permission choices share the Host lifecycle. A replacement window
uses **Take control** to resume control of loaded sessions and pending requests.
The directory combines Host resident sessions and DSH's inactive saved sessions.
Title filtering and Rename are unavailable for this provider.

```sh
cargo run -p fleetpane-host -- session /tmp/fleetpane-deepseek/control.sock --take-control list
cargo run -p fleetpane-host -- session /tmp/fleetpane-deepseek/control.sock --take-control config SESSION_ID reasoning_effort low
# Choose OPTION_ID from the pending request; JSON_ID is a JSON number or quoted string.
cargo run -p fleetpane-host -- session /tmp/fleetpane-deepseek/control.sock --take-control permission JSON_ID OPTION_ID
```

DSH 0.2.0-rc.2 resumes model context without replaying old messages. The desktop
warns that it shows retained activity observed by Fleetpane, independently of
provider transcript replay. Host restart restores the observed archive; unfinished
turns remain unknown and are not resent. Metadata-only open/resume and **Load earlier messages** page
through retained Host observations with a 256 KiB encoded reply budget. This is
independent of DSH's missing transcript replay. The adapter does not provide
elicitation or account setup UI. Local image send is enabled only for providers
advertising ACP v1 image support; installed DSH 0.2.0-rc.2 reports false. Existing frame/cache/journal
limits apply; an individual oversized item cannot be paged.
See [ACP semantics and limits](docs/adr/0013-deepseek-acp-provider.md).

When reading earlier content, **Jump to latest** returns to the loaded tail and
resumes following a growing reply. It is local navigation and preserves drafts and
pending permissions. Existing history limits still apply.
[Native verification](docs/verification/2026-10-21-history-navigation.md).

**Attention** lists pending approvals, questions and other provider requests from
every configured Host, including unopened views. Filter by request type, copy the
original JSON or open its checked source. Read-only viewing does not take control;
use **Take control**, then **Open pending session** to review in the original
conversation. External decisions update read-only views automatically. The queue
refreshes every two seconds while open and keeps unavailable Hosts visible.
**Turn results** shows explicit completed, failed and interrupted turns with ACP
stop reasons. Each Host retains at most 64 results/64 KiB for its current run;
eviction and unavailable or older Hosts remain visible. **Mark reviewed** and
**Mark shown results reviewed** save exact local receipts and preserve the
original outcome and pending requests. **Open source** checks the original Host,
then uses explicit control and bounded history inspection. Older turns may need
**Load earlier messages**. Reviews survive desktop replacement in controller-store
schema 12; the Host result feed does not survive Host restart. OS notifications,
mute/tray settings and request age remain open.

```sh
cargo run -p fleetpane-host -- attention /path/to/hosts.json
cargo run -p fleetpane-host -- attention /path/to/hosts.json /path/to/controller-store
cargo run -p fleetpane-host -- attention-review /path/to/controller-store /path/to/exact-results.json
python3 scripts/attention-smoke.py --evidence-dir /tmp/fleetpane-attention-proof
python3 scripts/completion-smoke.py
```

The smoke check holds synthetic requests for 125 seconds by default and verifies
exact provider answers, controller fencing and cross-Host isolation.
[Local acceptance and limits](docs/verification/2026-10-21-attention-queue.md).
[Turn-result acceptance and limits](docs/verification/2026-10-22-turn-results.md).

Codex conversations show **Provider session settings** with reported model,
provider and effort plus exact **Copy settings**. Updates follow provider events;
missing metadata stays unavailable. These are current/persisted settings, while
**Next message** chooses options for the next send. Individual turns may execute
with another model. [Verification](docs/verification/2026-10-22-session-settings.md).

Conversation details (tasks, approvals, questions, model options and attachments)
now scroll within at most half the space remaining above the fixed editor.
At 900 × 560, long drafts and pending interactions leave messages visible;
Send/Stop stay reachable. [Before/after and native interaction evidence](docs/verification/2026-10-21-conversation-space.md).

Native **Attach images** imports up to four PNG/JPEG/WebP files (8 MiB each),
previews/removes them and supports image-only Send. Saved image identities share
the text draft's revision and one-attempt receipt recovery. The Host keeps original
bytes in `SOCKET.attachments`; missing/corrupt files and unsupported providers
refuse sending without clearing the draft. Codex receives local image paths;
supported ACP receives image content while history keeps local references.
Clipboard image paste, remote upload and full-size media remain open.
[Architecture](docs/adr/0042-local-image-attachments.md),
[native and provider evidence](docs/verification/2026-10-21-image-attachments.md).

```sh
# Local file import does not contact a provider or send a message.
cargo run -p fleetpane-host -- image /tmp/fleetpane/control.sock import /absolute/screenshot.png
# Use the exact key/revision returned by the draft commands; attachment edits are CAS writes.
cargo run -p fleetpane-host -- draft STORE attach KEY_JSON REVISION /absolute/screenshot.png
cargo run -p fleetpane-host -- draft STORE remove-image KEY_JSON REVISION 0
python3 scripts/images-smoke.py
# Requires installed Codex; uses a private CODEX_HOME and a local SSE model stub.
python3 scripts/codex-images-smoke.py
```

## Use several local Hosts in one window

Start each Host independently with its intended provider/home/workspace, then
create a configuration such as `hosts.json` (absolute socket paths):

```json
{
  "version": 1,
  "hosts": [
    { "name": "OpenCodex", "socket": "/tmp/fleetpane-codex/control.sock" },
    { "name": "DeepSeek", "socket": "/tmp/fleetpane-deepseek/control.sock" }
  ]
}
```

```sh
cargo run -p fleetpane-host -- hosts hosts.json
cargo run --manifest-path apps/desktop/Cargo.toml --locked -- --hosts hosts.json
```

The first command reports read-only snapshots/errors for every endpoint without
taking control. The desktop initializes other Hosts when selected. **Take
control** is explicit for each existing controller; tab switching preserves
that Host's selected session, drafts, options and history without transferring
ownership. Opened background Hosts continue receiving events and show pending
requests in their labels. Unavailable endpoints have their own error view.

The configuration accepts 1–8 uniquely named endpoints and at most 16 KiB.
`--host SOCKET` still works for one Host. This does not launch Hosts, isolate
accounts, migrate sessions or implement remote connections. Text drafts now survive
window exit; see [session drafts](#saved-session-drafts). See [the architecture](docs/adr/0014-local-multi-host-workbench.md)
and [local verification](docs/verification/2026-10-20-multi-host.md).

To recommend a ready Host for new sessions, add explicit ordered groups to that
configuration. Group membership declares the endpoints you intend to use for
that workflow; names do not establish provider/account compatibility. For example,
after adding a `New OpenCodex` endpoint to `hosts`:

```json
"routes": [
  { "name": "Codex", "hosts": ["New OpenCodex", "OpenCodex"] },
  { "name": "DeepSeek", "hosts": ["DeepSeek"] }
]
```

Use **New via group**, enter the workspace directory, then **Find ready Host**.
The first ready member wins; drained, offline and faulted members show their
status. **Prepare on NAME** rechecks the recommendation, saves its creation UUID
and Host binding, then opens that Host's tab and fills its workspace input.
Use its **New session** button, taking control
explicitly if another controller owns it. Existing threads, drafts and approvals
stay on their original Host. If no member is ready, no session is created and
selection stays inside the group. Recommendations are not reservations or
automatic failover; stale/uncertain creations are never replayed on another Host.

`fleetpane-host route hosts.json Codex` reports the same recommendation read-only,
without claiming control or creating anything; no ready target returns a nonzero
exit status. Configurations without `routes` retain manual selection. The file is
reloaded when opening a window. Session selections remain memory-only; saved text
drafts restore when their original session is reopened.
Group creation bindings persist in `hosts.creations/creations.sqlite3` for a
`hosts.json` configuration. **Saved group creations** restores prepared work and
checks attempted creations against their original Host, including after a window
crash. Each saved intent allows one dispatch attempt; an unknown or missing
receipt never causes another send. A successful check shows the recovered session
ID; select its Host to open it from the session list. Keep the same store if the
configuration is renamed. Direct per-Host creation keeps its previous behavior.

The CLI uses the same engine, an explicit store and a caller-generated UUID:

```sh
fleetpane-host creation /absolute/hosts.creations plan hosts.json Codex /absolute/work UUID
fleetpane-host creation /absolute/hosts.creations send UUID # --take-control if already owned
fleetpane-host creation /absolute/hosts.creations check UUID # read-only; never resends
fleetpane-host creation /absolute/hosts.creations list # 32 newest; append oldest position for next page
```

Local controller store jobs share a queue per canonical directory before opening
SQLite. Cloned handles and ancestor aliases use the same queue, and a spawned job
keeps its slot even if its caller closes. Other stores and Host streams remain
independent; cross-process SQLite lock errors remain explicit.
[Decision and limits](docs/adr/0043-serialize-local-controller-store-jobs.md),
[local verification](docs/verification/2026-10-21-controller-store-queue.md).

The private store is bounded to 64 MiB and has no retention UI. Storage failures
stop creation. This is an initial binding, not complete session routing or
provider recovery. See [ADR 0018](docs/adr/0018-durable-local-creation-intents.md).

Older clients reject the optional configuration field. No Host protocol or journal upgrade is
needed for this slice. See [the decision](docs/adr/0017-explicit-ready-host-selection.md)
and [local verification](docs/verification/2026-10-20-host-selection.md).

## Saved projects and workspaces

**Workspaces** registers an existing ordinary folder or Git working directory on
a named local Host. Git details show branch/HEAD, changes (including staged
renames), and main-checkout/linked-worktree membership. Registration and Refresh
only inspect local directories; they do not claim a Host or start a provider.
**Prepare on NAME** selects the saved Host and directory for **New session**;
taking control remains explicit when another controller owns that Host.

Projects group Git workspaces by their common Git directory. Project/workspace
UUIDs survive branch and remote changes and window replacement; canonical path,
filesystem identity and Host endpoint keep distinct checkouts separate. Missing
or replaced directories/repositories fail visibly. The engine rechecks identity
before creation, but does not lock directories against concurrent replacement.

```sh
fleetpane-host workspace /absolute/hosts.creations add hosts.json Codex git /absolute/work
fleetpane-host workspace /absolute/hosts.creations add hosts.json Codex folder /absolute/folder
fleetpane-host workspace /absolute/hosts.creations list # 32 newest; optional oldest position
fleetpane-host workspace /absolute/hosts.creations inspect UUID
fleetpane-host workspace /absolute/hosts.creations diff UUID staged src/main.rs
fleetpane-host workspace /absolute/hosts.creations diff UUID unstaged src/main.rs
fleetpane-host workspace /absolute/hosts.creations create UUID # --take-control if needed
```

The catalog shares the private 64 MiB controller store with creation intents.
For `--hosts hosts.json`, keep `hosts.creations/creations.sqlite3`; `--host
/absolute/control.sock` uses `/absolute/control.controller/creations.sqlite3`.
Local-store schemas 1–4 upgrade transactionally to 5 and preserve creation
bindings, attempt flags, workspace IDs and draft revisions. Older controller binaries reject
schema 5. Host protocol and journal remain v3/schema 2. Worktree removal,
workspace setup scripts, file editor and complete terminal interaction remain open. Session creation follows direct
per-Host command receipts; this catalog does not add durable session ownership
routing or ADR 0018 group creation bindings. Git observations have a 10-second
deadline per subprocess, 256 KiB per output stream and 500 changed-path limit;
exceeding a bound reports an error. Non-UTF-8 paths are unsupported.
Choose **Staged** or **Unstaged**, then select a changed file to open its unified
Diff. **Refresh diff**, **Back to changes** and **Copy patch** retain the explicit
comparison. Paths are relative to the Git worktree root, even when a saved
workspace is a subdirectory. Adds, deletes, renames, mode changes and untracked
files/symlinks are supported. Binary files and unresolved conflicts have explicit
messages; untracked directories cannot yet be expanded. External diff and
textconv are disabled, and status/diff consistently use 50% rename detection.
Patch output is limited to 256 KiB, 8000 lines and 16 KiB per line; untracked
input is limited to 1 MiB. Exceeding a bound does not silently truncate a patch.
These are live observations, not an atomic index/worktree snapshot or a filesystem
sandbox. Commit/base-ref comparisons, stage, commit, push and conflict resolution
remain open. See [ADR 0021](docs/adr/0021-read-only-workspace-diffs.md) and
[Diff verification](docs/verification/2026-10-20-workspace-diffs.md).

See [ADR 0019](docs/adr/0019-existing-workspace-catalog.md) and
[local verification](docs/verification/2026-10-20-workspaces.md).

Select a saved Git workspace, enter **New branch** and **Base ref or commit**,
then choose **Preview worktree**. The preview saves the resolved commit and shows
the new branch, directory and session cwd. **Create this worktree** creates that
branch and checkout in `STORE/worktrees/OPERATION_UUID`, preserving a selected
project subdirectory. It registers the result without starting a session. Open
the result and prepare a session separately. Uncommitted source files are not
copied; moving the baseline ref after preview does not change the pinned commit.

Choose **Existing local branch** to create a checkout for a local branch already
in the repository. Preview records its expected tip; creation refuses a changed,
deleted or occupied branch. No remote branch is guessed/fetched, and no branch is
reset or forced. The base input applies only to **New branch**.

```sh
fleetpane-host worktree /absolute/hosts.creations plan SOURCE_WORKSPACE_UUID feature/task main OPERATION_UUID
fleetpane-host worktree /absolute/hosts.creations plan-existing SOURCE_WORKSPACE_UUID feature/existing OPERATION_UUID
fleetpane-host worktree /absolute/hosts.creations apply OPERATION_UUID
fleetpane-host worktree /absolute/hosts.creations check OPERATION_UUID
fleetpane-host worktree /absolute/hosts.creations list # optional oldest position for next page
```

The saved result distinguishes prepared, attempted, completed and failed work.
Only a prepared plan can execute, once; repeated/concurrent attempts are refused.
Reopen Workspaces to see the 32 most recent creation records. **Check saved
status** reads the controller database without invoking Git or claiming a Host.
An interrupted attempt remains unresolved; it is not replayed or automatically
treated as success. Git errors report the failed step and possible residual
directory/branch. Existing targets are refused; partial results are retained for
inspection. There is no automatic rollback or deletion. Git hooks and recursive
submodule checkout are disabled; no setup script runs. Configured checkout
filters can still execute. Creation has a 120-second process-group deadline;
a hard-killed controller can leave Git running, so inspect an unresolved attempt
before further repository operations. Worktree disk usage is separate from the
64 MiB database limit. See [ADR 0020](docs/adr/0020-planned-worktree-creation.md)
and [verification](docs/verification/2026-10-20-worktree-creation.md).

Fleetpane creation operations share a repository lock across linked worktrees,
windows and controller stores. A busy repository is refused before an attempt.
An admitted checkout continues to completion/deadline if its async caller goes
away while the controller process lives. Exiting the program or hard-killing the
controller still requires manual inspection. External Git
commands do not honor this guard. See [ADR 0045](docs/adr/0045-existing-branch-worktrees.md)
and [existing-branch evidence](docs/verification/2026-10-21-existing-worktrees.md).

## Saved session drafts

Paste PNG/JPEG/WebP images into the message composer, or use **Attach images**.
Pasted images join the saved text/image draft and wait for explicit Send.
Provider capabilities and the four-image / 8 MiB-per-image bounds still apply.
See [clipboard input evidence](docs/verification/2026-10-21-clipboard-images.md)
for native Linux coverage and remaining platform/media limits.

Text in an opened Host-backed session saves to the same private controller store.
**Draft saved locally** confirms the save; closing the window waits for outstanding
saves. A save failure keeps the window open with the text available to copy.
Multiple windows use revision checks: a stale save cannot overwrite another
window, and reloading after a conflict explicitly discards local edits.

Draft identity includes the exact socket, Host incarnation, session ID and working
directory. Reopen that session to restore its draft after a desktop restart.
Restarting the Host changes its incarnation; earlier drafts can be recovered with
the paginated CLI listing, without attaching them to a reused session ID.

```sh
fleetpane-host draft /absolute/hosts.creations list # 32 rows, optional oldest position
fleetpane-host draft /absolute/hosts.creations get KEY_JSON
fleetpane-host draft /absolute/hosts.creations edit KEY_JSON REVISION TEXT
fleetpane-host draft /absolute/hosts.creations check KEY_JSON
fleetpane-host draft /absolute/hosts.creations send KEY_JSON REVISION # --take-control if needed
```

`KEY_JSON` contains `socket`, `host_id`, `thread_id` and `cwd`, as returned by
`list`. A send saves its operation UUID and exact provider request before dispatch.
Success clears the draft only after a matching receipt; rejection restores editable
text. After interruption, **Check send status** only reads the original Host's
receipt. Pending/unknown attempts remain read-only and available to copy; they
are never automatically resent. A cleared draft retains its revision so stale
windows cannot restore already-sent text.

Each draft is limited to 64 KiB of UTF-8 text within the shared 64 MiB database.
The desktop keeps one saved record, one in-flight save and one latest pending
text per opened Host. Forced termination restores the last confirmed save;
new-session input before creation, model options, attachments, question answers
and selection/layout persistence are outside this slice. See [ADR 0022](docs/adr/0022-durable-session-drafts.md)
and [local verification](docs/verification/2026-10-20-session-drafts.md).

## Browse workspace files

PNG/JPEG/WebP filenames offer **Previous image**, **Next image** and position/count
within the current directory. Each step refreshes the listing and reads one image;
corrupt files show their own preview error. Direct path opens offer **Load image
list**. Failed neighbor reads keep the prior preview for explicit retry. See
[native verification and limits](docs/verification/2026-10-21-image-navigation.md).

Inspect a saved workspace and choose **Browse files**. Navigate one directory at
a time with **Root**, **Up** or a file/directory path and **Go** or **Enter**.
Relative, `./` and absolute paths within the saved root are accepted. **Refresh folder**
reloads its contents; **Reload file** observes external edits. This works without
Host control and never starts a provider or writes files.

**Back** and **Forward** revisit up to 32 locations within the open file view,
re-reading files and restoring the top directory row when it still exists.
Refreshing keeps the forward branch; opening a new location after Back replaces
it. Missing or replaced paths show an error without advancing the history cursor.
Closing the view discards this history. [Verification and limits](docs/verification/2026-10-21-file-history.md).

Explicit Markdown file links and structured tool/change paths open the same
read-only view over the retained conversation. **Back to conversation** restores
the composer and draft. Markdown `#L2`, `#L2-L3` and `:2` anchors start the preview
at that one-based line; tool line metadata remains display-only. Unsupported
schemes and outside-root paths show errors. Inline code is not guessed as a path.

UTF-8 text files up to 256 KiB have a selectable read-only preview. The display
shows at most 16 KiB from the selected source line with a notice; **Copy full text** preserves all
loaded text, including line endings and whitespace. Binary, empty and oversized
files have explicit messages. Listings are limited to 500 entries; symlinks and
special files are displayed but not opened. Parent traversal and replaced saved
directories are refused. At most two filesystem workers run per process; calls
return an error after five seconds, while blocked workers retain their slots.

```sh
fleetpane-host workspace /absolute/hosts.creations files WORKSPACE_UUID
fleetpane-host workspace /absolute/hosts.creations files WORKSPACE_UUID src
fleetpane-host workspace /absolute/hosts.creations read WORKSPACE_UUID src/main.rs
fleetpane-host workspace /absolute/hosts.creations open WORKSPACE_UUID ./src/main.rs
```

PNG, JPEG and WebP files have a fitted image preview, type and pixel dimensions.
Images are validated from their bytes, bounded to 8 MiB, 8192 pixels per dimension
and 16 Mi-pixels. Reload observes external changes; corrupt/unsupported/oversized
images show explicit notices. The shared CLI result includes the original bytes
as base64 plus SHA-256 and metadata. Images are never imported as attachments.

Editing, recursive trees, watching/search, thumbnail grids/zoom, embedded Markdown-file images,
other media and large-file streaming remain open. See [the decision](docs/adr/0026-read-only-workspace-files.md)
and [engine verification](docs/verification/2026-10-20-workspace-files.md), plus
[conversation navigation acceptance](docs/verification/2026-10-21-file-navigation.md) and
[image preview acceptance](docs/verification/2026-10-21-file-images.md).

## Pin frequently used workspaces

Choose **Pin** on a saved workspace to keep it in the separate **Pinned** group,
even when viewing another catalog page. Up to 32 pins survive window replacement;
the newest pin appears first. **Unpin** preserves the saved workspace, drafts and
running sessions. Pins belong to the local controller store and require no Host
control or provider connection. Same-path entries on different Hosts are distinct.

**Use workspace** selects the saved Host and prepares its directory for a new
session, after checking directory identity. It does not create a session or take
control. Use **Newest** to reload preference changes from another window.

```sh
fleetpane-host workspace /absolute/hosts.creations pins
fleetpane-host workspace /absolute/hosts.creations pin WORKSPACE_UUID on
fleetpane-host workspace /absolute/hosts.creations pin WORKSPACE_UUID off
```

This adds local quick access; workspace tabs, drag ordering, keyboard pin shortcuts
and selection restoration remain open. See [the decision](docs/adr/0025-local-workspace-pins.md)
and [verification](docs/verification/2026-10-20-workspace-pins.md).

## Follow up during a Codex turn

Type in the composer while Codex runs and choose **Send to active turn**.
The message is bound to that turn; if the turn ends or changes, the provider
rejects it and the text is retained. This action uses the current turn's model
and does not approve pending tools or start another turn.

```sh
fleetpane-host session /absolute/control.sock --take-control steer SESSION TURN TEXT
fleetpane-host draft /absolute/hosts.creations steer KEY_JSON REVISION TURN --take-control
```

The saved-draft command persists the exact request before dispatch, like normal
Send. Pending or uncertain outcomes require a receipt check and are never
resent automatically. DeepSeek ACP currently has no live follow-up operation;
the UI explains this and offers Stop before a new ordinary send. Input queues
remain unimplemented. See [the decision](docs/adr/0024-active-turn-follow-ups.md)
and [verification](docs/verification/2026-10-20-active-turn-follow-ups.md).

## Agent task lists

Host-backed conversations display provider-reported steps with status, optional
priority and explanation, plus refresh, copy and collapse controls. Codex task
notifications, standard ACP plans and successfully completed DeepSeek
`todo_write` tools share this panel. A new report replaces the entire list;
stopping or finishing a turn does not mark unfinished tasks complete. Markdown
plan-mode proposals remain separate conversation items.

```sh
fleetpane-host session /absolute/control.sock tasks SESSION_ID
```

This command reads the current Host cache without taking control or sending a
provider request. It requires an established controller lease. The same-Host
cache survives desktop replacement and event expiry, retaining at most 64
recently updated sessions and 1 MiB total. Each plan is limited to 128 entries
and 32 KiB. Host restart and cache eviction can leave no retained plan; earlier
plan history is not reconstructed. See [ADR 0023](docs/adr/0023-agent-task-plans.md)
and [verification](docs/verification/2026-10-20-agent-tasks.md).

## Try the directory client

With an existing Paseo daemon listening on loopback:

```sh
cargo run -p fleetpane-cli -- 127.0.0.1:6767
```

The CLI prints one page of up to 200 active sessions as JSON. If `pageInfo.hasMore`
is true, pass the returned `nextCursor` as the second argument for another page.
This is a one-shot snapshot. It requests neither timeline streams nor live
subscriptions, and does not start, stop or modify agents.

This prototype supports a directly accessible, unauthenticated loopback WebSocket
daemon only. TLS, relay connections and authentication are not implemented.
Output contains session metadata; use synthetic examples when sharing output.

Workspace files can **Save a copy…** to a new absolute local filename (up to
64 MiB). Export preserves current bytes, refuses existing targets and reports
SHA-256. The same operation is available as
`fleetpane-host workspace STORE export UUID RELATIVE_SOURCE ABSOLUTE_DESTINATION`.
It requires no running Host. [Evidence and limits](docs/verification/2026-10-22-file-export.md).

Drop local PNG/JPEG/WebP files onto an existing Host conversation to add them to
its saved draft. Picker, paste and drop keep the same four-image limits. Invalid
batches preserve the draft; sending remains explicit.
[Linux native verification](docs/verification/2026-10-22-image-drop.md).

## Workspace

| Package | Responsibility |
| --- | --- |
| `fleetpane-protocol` | Typed, forward-compatible directory projection and request encoding |
| `fleetpane-client` | Bounded WebSocket handshake and correlated directory request |
| `fleetpane-cli` | Inspect the same directory client used by the desktop |
| `fleetpane-runtime` | Host state, fencing, bounded replay and Unix provider transport |
| `fleetpane-host` | Standalone host and replaceable request client |
| `fleetpane-session` | Toolkit-independent Codex/ACP conversation projection and shared Host commands |
| `fleetpane-desktop` | GPUI Kit directory, composer, timeline, approvals and questions (separate manifest) |

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 scripts/stop-progress-smoke.py
cargo build -p fleetpane-host --locked
python3 scripts/runtime-smoke.py                 # Unix; synthetic provider
python3 scripts/runtime-smoke.py --codex         # optional; installed Codex required
python3 scripts/runtime-smoke.py --codex --drain # optional; preserve a real command while draining
python3 scripts/session-smoke.py                 # Unix; synthetic conversation workflows
python3 scripts/creation-directory-smoke.py      # Unix; Host directory checks before fresh session admission
python3 scripts/directory-smoke.py               # Unix; paged queries, renaming and failures
python3 scripts/models-smoke.py                  # Unix; paged model capabilities and validated turn options
python3 scripts/mcp-smoke.py                     # Unix; explicit journaled MCP discovery, paging and fencing
python3 scripts/codex-mcp-smoke.py               # optional; installed Codex, synthetic local MCP, no model calls
python3 scripts/history-smoke.py                 # Unix; large synthetic history and bounded item/turn pages
python3 scripts/query-smoke.py                   # Unix; repeated reads, fencing and send deduplication
python3 scripts/acp-smoke.py                     # Unix; ACP lifecycle, config, permissions, fencing and resume
python3 scripts/acp-history-smoke.py             # Unix; byte-bounded observed history, scoped stable cursors
python3 scripts/acp-archive-smoke.py             # Unix; crash recovery and archive storage faults
python3 scripts/acp-stop-smoke.py                # Unix; admission, quiet prompts and explicit stop reasons
python3 scripts/acp-order-smoke.py               # Unix; text/tool ordering, message identity and exact restart history
python3 scripts/acp-info-smoke.py                 # Unix; idle/live session info, archive and commit-failure fencing
python3 scripts/acp-tools-smoke.py                # Unix; minimal tool defaults, exact patches and archived snapshots
python3 scripts/resources-smoke.py                # Unix; supplied ACP/MCP resources, exact reload and ACP archive restart
python3 scripts/acp-cancel-smoke.py               # Unix; pending/late permissions, cancellation settlement and isolation
python3 scripts/acp-review-smoke.py               # Unix; exact request details, choices, controller/session isolation
python3 scripts/recent-directories-smoke.py       # Unix; device-local exact MRU, Host isolation, corruption/forget
python3 scripts/appearance-smoke.py              # Unix; device preference persistence, migration, conflicts and failures
python3 scripts/paseo-receipts-smoke.py          # Unix; process death, durable reports and explicit retry
python3 scripts/paseo-scratch-smoke.py           # Unix; paused/killed copies and explicit scratch collection
python3 scripts/paseo-signals-smoke.py           # Unix; SIGINT/SIGTERM finish the current copy and save Stop
python3 scripts/paseo-fork-smoke.py              # Unix; saved context fork, stale binding and interrupted-operation recovery
python3 scripts/paseo-fork-smoke.py --real       # optional; real Codex app-server, local rejecting model, no account
python3 scripts/paseo-fork-smoke.py --opencodex  # optional; one real model turn through existing loopback proxy
python3 scripts/concurrency-smoke.py             # Unix; 20 synthetic ACP streams, reads, cancel and handoff
python3 scripts/host-diagnostics-smoke.py         # Unix; eight endpoints, timeout/protocol/fault and unchanged snapshots
python3 scripts/multihost-smoke.py               # Unix; colliding IDs, permissions, read-only probes and one-Host loss
python3 scripts/journal-smoke.py                 # Unix; durable receipts, crash unknowns and exact deduplication
python3 scripts/drain-smoke.py                   # Unix; old/new Hosts, durable admission drain and answers/Stop
python3 scripts/routing-smoke.py                 # Unix; explicit groups, ready recommendations, no cross-Host replay
python3 scripts/creations-smoke.py               # Unix; durable binding, killed controller, Host restart, no second dispatch
python3 scripts/workspaces-smoke.py              # Unix; real Git identity/status, linked worktrees, bounded reads, Codex/ACP cwd
python3 scripts/file-navigation-smoke.py          # Unix; offline file/directory paths, exact bytes and root confinement
python3 scripts/file-export-smoke.py              # Unix; exact raw copy, concurrent no-overwrite and path/size boundaries
python3 scripts/diffs-smoke.py                   # Unix; real staged/unstaged/untracked Git diffs, bounds, deadline and read-only proof
python3 scripts/worktrees-smoke.py               # Unix; pinned worktree creation, concurrent attempt, crash/residual/deadline checks (~2 minutes)
python3 scripts/existing-worktrees-smoke.py       # Unix; local branches, tip/occupancy checks, shared guard and late-change residual
python3 scripts/codex-session-smoke.py           # optional; real Codex, local rejecting HTTP fixture
python3 scripts/codex-questions-smoke.py         # optional; real Codex question, local SSE model fixture
python3 scripts/codex-history-smoke.py           # optional; real Codex nested tool history, local SSE model fixture
cargo fmt --manifest-path apps/desktop/Cargo.toml --check
cargo test --manifest-path apps/desktop/Cargo.toml --locked
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked
```

Default tests use synthetic WebSocket/stdio providers and need no provider accounts.
The Unix process tests require Python 3. The optional Codex checks use an empty
`CODEX_HOME`; the conversation check sends one turn to a local HTTP fixture that
returns 400 and verifies the failed turn and persisted history. The question check
uses a local SSE model to trigger a real Codex question and validates the answer
in the next model request. The history check runs a nested command, reloads its
saved items, restarts the completed Host/provider and checks the saved command
again through full and paged reads without reexecuting it. These optional checks make no external LLM calls. See the
[question verification](docs/verification/2026-10-19-questions.md).
See [current behavior and gaps](docs/status.md) and the
[architecture decision](docs/adr/0001-independent-native-client.md).

## Rewrite workflow

The repository includes a [Rust native rewrite skill](.agents/skills/rust-native-rewrite/SKILL.md)
derived from publicly documented ArtCraft engineering practices. It is locally
authored and is not an official ArtCraft skill. Its reference notes pin the exact
public source revisions and distinguish borrowed ideas from our adaptations.

The main lessons are to keep the engine independent of the UI, verify actual
workflows through shared entry points, and compare complete release builds under
the same workload. Interface coverage alone does not establish replacement
readiness. [PhotoCraft's public guide](https://github.com/storytold/photocraft/blob/8d2e615ab93d424c4ec25ab15b4edb5368d1f9a6/AGENTS.md)
and [parity assessment](https://github.com/storytold/photocraft/blob/8d2e615ab93d424c4ec25ab15b4edb5368d1f9a6/docs/target-app-parity.md)
describe these practices.

Apache-2.0. Fleetpane is not affiliated with Paseo or ArtCraft.

### Host-owned terminals (Linux prototype)

`fleetpane-host terminal STORE WORKSPACE_UUID create LABEL` starts an owned POSIX
shell in a saved workspace. Add `--take-control` before the verb when a controller
already owns the Host. `list` and `output ID AFTER_BYTE` only read; `input ID
EXPECTED_INPUT TEXT` writes exact bytes without Enter. `key ID EXPECTED_INPUT
enter|interrupt|eof|tab|escape`, `resize ID ROWS COLS`, `rename ID LABEL` and
`close ID` use the same typed engine and durable operation receipts. See `--help`
and the [acceptance limits](docs/verification/2026-10-20-host-terminals.md).
Provider loss leaves terminal operations and their pending receipts available; storage/Host faults fence all mutations. See [fault isolation evidence](docs/verification/2026-10-21-terminal-fault-domains.md).
In the desktop, open **Workspaces**, select a saved directory, then **Terminals**.
The panel lists existing shells without claiming control. **Take control** transfers
control of the entire Host; then create, rename, resize or close a shell. Closing
the panel/window keeps the Host and shell running. Its ANSI screen handles
incremental UTF-8, styles, cursor movement, alternate screens and bounded
scrollback; **Copy screen** copies the displayed text without escape sequences.

The composer sends exact UTF-8 with **Send text**, or appends CR with **Send + Enter**.
Separate Enter, Ctrl-C, Ctrl-D and Tab buttons send control bytes. Unsent drafts
stay with each terminal during panel selection. Background reads do not disable
input, and an explicit operation waits behind an in-flight read once. Uncertain
receipts expose **Check operation** without resending a command.

**Type in terminal** enables direct keyboard input for the focused screen. Arrows,
control/function keys and Tab use terminal encodings; **Ctrl+Shift+V** preserves
text with the observed bracketed-paste mode, and **Ctrl+Shift+C** copies the screen.
A bounded 16 KiB queue sends ordered 4 KiB packets through the same fenced Host
commands. Errors pause queued input for explicit inspection; uncertain packets
are never automatically resent. Normal window close waits for accepted input.
See [keyboard acceptance and limits](docs/verification/2026-10-21-terminal-keyboard.md).

Native text selection, IME acceptance, terminal replies/mouse reporting, automatic geometry and Host
update recovery remain open. One selected screen is retained (up to 60×200 cells
and 1,000 scrollback lines); dropped Host output clears incomplete parser state
and remains visibly flagged. Resize does not reflow prior lines. Terminal drafts
and the panel's pending-operation selection are transient; durable operation
receipts remain discoverable through the CLI. See
[native acceptance and limits](docs/verification/2026-10-21-native-terminals.md).

### Restore existing Codex archives

Choose **Archived** beside the session filters to read provider-archived history.
**Restore to sessions** restores the selected original ID; it preserves the loaded
history/draft and does not start a turn, resume a context or restore descendants.
For an unloaded context, use **Resume context** before sending. Failures keep the
archive/draft visible. ACP provider archive listing is unsupported.

The shared CLI exposes `session SOCKET [--take-control] list --archived` (also
accepting title/cwd/cursor flags) and `session SOCKET [--take-control] restore THREAD`.
Restoration is a fenced mutation and unavailable during drain. Safe archive
mutation/tree impact is still separate work. See
[verification and limits](docs/verification/2026-10-21-archive-restoration.md).

### Inspect a Codex session tree

Choose **Session tree** in an open conversation, or run:

```bash
cargo run -p fleetpane-host -- session /path/to/control.sock tree THREAD_ID
python3 scripts/session-tree-smoke.py
```

Inspection uses the existing controller lease without taking control. It reads
normal/archived metadata and loaded contexts, includes spawned descendants and
shows pending requests, observed activity and incomplete ancestry. Forks remain
independent. Use Refresh tree, Copy tree JSON or Copy ID; switching conversation
closes the inspector. Failures retain explicitly stale observations. **Open session**
rechecks the observed identity, ancestry and archive membership before opening
history; **Back to source session** returns to the session whose tree was opened.
Both preserve saved drafts and require explicit control. Unknown archive membership
remains inspection-only; opening never resumes or restores a context. See
[navigation evidence](docs/verification/2026-10-22-tree-navigation.md).

Reads are bounded to 5,000 nodes, 8 MiB metadata, depth 64 and 30 seconds. The
result is non-atomic and does not enumerate background resources or authorize
archive/stop. ACP does not advertise this capability. See
[verification](docs/verification/2026-10-21-session-tree.md) and
[ADR 0056](docs/adr/0056-observe-session-ancestry.md).

The native workbench remembers the selected local Host in the configuration's
controller store (introduced in schema 11, current schema 13). Restore uses the exact socket path across Host
rename/reorder. Offline targets stay selected; absent targets show an explicit
notice and preserve the saved choice. Switching and restoring never take control
or resume a session. Reload choice reads current preferences without navigating;
Remember current Host explicitly saves after a conflict. Keep previous choice
retains the old record after a failed save. Normal titlebar close waits for saves.

```sh
fleetpane-host host-choice /absolute/controller-store get
fleetpane-host host-choice /absolute/controller-store set 0 /absolute/hosts.json "OpenCodex"
python3 scripts/host-choice-smoke.py
```

The CLI requires the last observed revision. This selection is scoped to the
controller store; moving the configuration may change its default store location.
Host choice does not open a session. Older binaries cannot open an upgraded
store; the current schema is 13. See [local evidence](docs/verification/2026-10-21-preferred-host.md).

Each Host view also remembers its last inspected session in that controller
store. **Open last session** requires explicit control, checks exact Host UUID,
session ID, cwd and archive membership, then opens saved history without context
resume or Send. Startup does not automatically inspect or take control. The check
is bounded to 800 metadata rows / 10 seconds; unavailable or changed sources
require a fresh explicit selection. Conflicting writes retain the old record;
**Reload choice**, **Remember current** and **Keep previous choice** expose the
recovery choices. Normal close waits for local saves.

```sh
fleetpane-host last-session /absolute/controller-store /absolute/host.sock get
fleetpane-host last-session /absolute/controller-store /absolute/host.sock check
python3 scripts/last-session-smoke.py
python3 scripts/last-session-observation-smoke.py
```

`get` and revisioned `set REVISION TARGET_JSON` use only local state. `check`
observes the existing Host without claiming/resuming. See
[local evidence](docs/verification/2026-10-22-last-session.md).

All sessions defaults to **Activity**: fresh active sessions first, then provider
update time. **By Host** retains provider page order. Ordering covers loaded rows
only; each Host has independent Load more controls in its summary, and unavailable
rows say Last observed. Filtering, sorting and opening never resume or send.

Turn headers offer **Copy loaded answer** for assistant text currently loaded in that turn. Earlier pages are never fetched by Copy; the notice reports copied bytes and excluded non-text blocks. [Verification and scope](docs/verification/2026-10-22-turn-answer-copy.md).

ACP sessions can advertise a **Commands** menu in the scrollable session options
region. Selection fills an empty draft or replaces a slash prefix; **Send** runs
the ordinary prompt. Existing arguments/prose stay intact. **Copy commands**
preserves the complete current list. Unavailable and explicitly empty lists are
distinct; a restarted/resumed provider must advertise again.
[Verification and limits](docs/verification/2026-10-22-acp-commands.md).
