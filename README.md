# Fleetpane

A native Rust foundation for managing fleets of coding-agent sessions.

Fleetpane is an independent project. The first implementation includes a GPUI Kit
session directory, a read-only Paseo CLI, and Host-backed Codex/DeepSeek ACP conversations.
The target is donut-paseo functional completeness; [the parity inventory](docs/parity.md)
records what remains. This prototype is not a replacement release.
The Unix Session Host keeps its provider connection alive across control-client replacement. These are
foundations, not a usable replacement release. No performance gain is claimed.

See the [product requirements](docs/prd.md), [behavior status](docs/status.md), and
[Session Host architecture](docs/adr/0002-preserve-runtimes-across-control-daemon-upgrades.md).

## Try the native desktop

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

Terminal emulation, attachment upload/send and broader
provider integration acceptance remain pending. The Host
view retains at most 500 items, 500 turns and 2 MiB; reaching that bound stops
older loads, preserves current content and offers Reload for recent history.
Legacy and explicit CLI full reads remain bounded. A single item or a 16-entry
page can still exceed the transport's 256 KiB provider-frame cap. Item previews
limit text sections visibly to 16 KiB; Copy retains the full item within those
transport bounds. Byte-aware pages and a history window with eviction remain
necessary for complete traversal of arbitrarily large conversations.

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
IME, accessibility and terminal integration remain unverified.

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

Each CLI invocation is a separate controller; `--take-control` explicitly fences
the previous owner. Do not use it against a Host another window should control.
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
restore an in-flight provider/tool or ACP observed history. See
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

Create or select a session, choose the provider's model/reasoning options under
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
warns that it shows activity observed by this Host; a Host restart starts a fresh
observed transcript. Metadata-only open/resume and **Load earlier messages** page
through retained Host observations with a 256 KiB encoded reply budget. This is
independent of DSH's missing transcript replay. The adapter does not provide
elicitation, attachment send or account setup UI. Existing frame/cache/journal
limits apply; an individual oversized item cannot be paged.
See [ACP semantics and limits](docs/adr/0013-deepseek-acp-provider.md).

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
Local-store schemas 1–3 upgrade transactionally to 4 and preserve creation
bindings, attempt flags and workspace IDs. Older controller binaries reject
schema 4. Host protocol and journal remain v3/schema 2. Worktree removal,
workspace setup scripts, file editor and terminal remain open. Session creation follows direct
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

```sh
fleetpane-host worktree /absolute/hosts.creations plan SOURCE_WORKSPACE_UUID feature/task main OPERATION_UUID
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

## Saved session drafts

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
cargo build -p fleetpane-host --locked
python3 scripts/runtime-smoke.py                 # Unix; synthetic provider
python3 scripts/runtime-smoke.py --codex         # optional; installed Codex required
python3 scripts/runtime-smoke.py --codex --drain # optional; preserve a real command while draining
python3 scripts/session-smoke.py                 # Unix; synthetic conversation workflows
python3 scripts/directory-smoke.py               # Unix; paged queries, renaming and failures
python3 scripts/models-smoke.py                  # Unix; paged model capabilities and validated turn options
python3 scripts/history-smoke.py                 # Unix; large synthetic history and bounded item/turn pages
python3 scripts/query-smoke.py                   # Unix; repeated reads, fencing and send deduplication
python3 scripts/acp-smoke.py                     # Unix; ACP lifecycle, config, permissions, fencing and resume
python3 scripts/acp-history-smoke.py             # Unix; byte-bounded observed history, scoped stable cursors
python3 scripts/concurrency-smoke.py             # Unix; 20 synthetic ACP streams, reads, cancel and handoff
python3 scripts/multihost-smoke.py               # Unix; colliding IDs, permissions, read-only probes and one-Host loss
python3 scripts/journal-smoke.py                 # Unix; durable receipts, crash unknowns and exact deduplication
python3 scripts/drain-smoke.py                   # Unix; old/new Hosts, durable admission drain and answers/Stop
python3 scripts/routing-smoke.py                 # Unix; explicit groups, ready recommendations, no cross-Host replay
python3 scripts/creations-smoke.py               # Unix; durable binding, killed controller, Host restart, no second dispatch
python3 scripts/workspaces-smoke.py              # Unix; real Git identity/status, linked worktrees, bounded reads, Codex/ACP cwd
python3 scripts/diffs-smoke.py                   # Unix; real staged/unstaged/untracked Git diffs, bounds, deadline and read-only proof
python3 scripts/worktrees-smoke.py               # Unix; pinned worktree creation, concurrent attempt, crash/residual/deadline checks (~2 minutes)
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
