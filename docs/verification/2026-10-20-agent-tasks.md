# Provider task lists — 2026-10-20

Local development builds based on `973b0ed883c25356a9f0849681333919d4497387`
plus the uncommitted foundation on `feat/session-host-foundation`. Rust 1.98.1,
GPUI Kit 0.7.3, Linux X11/software Vulkan, hidden 1100 × 720 native window.
This is covered-workflow evidence, not replacement readiness or remote CI.

## Behavior and regression proof

The shared runtime/session engine projects Codex task notifications and standard
ACP full-list plans into the native panel. Step status, optional priority,
explanation, exact Chinese multiline text and future statuses remain inspectable.
An empty report clears the list. Replanning replaces all old steps; neither a
normal turn ending nor Stop completes tasks implicitly. Existing Markdown
plan-mode proposal rendering is separate.

A real DSH 0.2.0-rc.2 turn actually executed two `todo_write` calls but left the
first implementation's task cache empty. Readback contained both generic tool
cards and their exact `rawInput.todos`. Inspection of the installed DSH ACP
bridge confirmed it emits tool start/result events rather than plan updates.
The new regression test
`deepseek_completed_todos_report_plans_without_inventing_priority` failed before
the compatibility fix (zero plans instead of one) and passes afterward. The
adapter now projects only successfully completed `todo_write` input, keeps the
tool card, ignores pending/failed calls and avoids replaying an old completed
call on a later content-only update. No priority is invented.

Passed commands on the final source:

```sh
cargo fmt --all --check
cargo test --workspace --locked -j2
cargo clippy --workspace --all-targets --locked -j2 -- -D warnings
cargo build -p fleetpane-host --locked -j2
cargo fmt --manifest-path apps/desktop/Cargo.toml --check
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -j2 -- -D warnings
cargo test --manifest-path apps/desktop/Cargo.toml --locked -j2
cargo build --manifest-path apps/desktop/Cargo.toml --locked -j2
python3 scripts/tasks-smoke.py
python3 scripts/acp-smoke.py
python3 scripts/drafts-smoke.py
uvx --offline ruff check scripts/
uvx --offline ruff format --check scripts/tasks-smoke.py scripts/acp-fixture.py scripts/provider-fixture.py
git diff --check
```

Root tests: 89 passed; desktop tests: 4 passed, 93 total. Strict Clippy passes for
both manifests. `tasks-smoke.py` is included in Unix CI configuration; remote CI
was not run. The process smoke exercises real private Hosts with deterministic
Codex/ACP providers: full replacement, clear, future status, same-cwd session
isolation, 140-event ring expiry, repeated reads with an identical complete Host
snapshot, stale lease/invalid input rejection, Stop, drain, provider fault and
Host restart. DSH-shaped successful/failed/late-update tool events have their own
fixture checks. Reads neither claim ownership nor dispatch provider work or
consume journal/event entries. A Host restart deliberately yields no cached plan.
ACP and durable-draft process regressions pass after the final runtime change.

## Native synthetic acceptance

Using the agent-workspace-linux skill, the isolated native desktop showed both
provider task panels, including priority, explanation and Chinese multiline
steps. Copy returned exact status/priority/content text. Refresh left the entire
Host snapshot unchanged. The desktop was forcibly terminated and reopened;
both original Host/provider PID pairs and complete snapshots stayed unchanged
before takeover. After explicit Take control and session selection, the same
lists and clipboard content returned:
[Codex](assets/agent-tasks/codex-recovered.png),
[ACP](assets/agent-tasks/acp-recovered.png).

Native Stop left an in-progress task unfinished. Starting a different Codex turn
without a plan displayed
[Tasks from previous turn](assets/agent-tasks/previous-turn.png).
A new session with the same cwd had
[no retained plan](assets/agent-tasks/session-isolation.png).
An ACP future status stayed visible as
[awaiting_review](assets/agent-tasks/future-status.png), rather than pending.
Sending a clear report displayed
[Agent cleared the task list](assets/agent-tasks/cleared.png);
[collapse/show](assets/agent-tasks/collapsed.png) preserved the list.
These checks preceded the DSH bridge addition; the native panel code did not
change afterward. Final-build real DSH acceptance follows.

## Real installed harnesses

A new private DSH home, containing only a temporary 0600 credential copy, started
a task-owned Host with the final runtime binary. The workspace and prompts were
synthetic; no existing session or runtime was adopted. DSH's DeepSeek-v4-flash
actually called `todo_write` first with in_progress/pending entries, then with
both completed. Exact-turn readback confirmed successful completion and both
original tool calls. The shared task read returned exactly the two Chinese steps
with completed statuses and null priorities. The final GPUI Kit build showed
[2/2 completed](assets/agent-tasks/live-dsh-tasks.png).

The real Host/provider PIDs remained 146545/146547 when the final desktop closed
normally and reopened. Its entire Host snapshot remained identical before
Take control. The replacement window [recovered the same task list](assets/agent-tasks/live-dsh-recovered.png). Clipboard
readback was checked while the application still owned the X11 clipboard; no
clipboard persistence after application exit is claimed.

Two isolated OpenCodex/Codex 0.163.0 turns completed through the existing local
OpenCodex endpoint, using a private CODEX_HOME and a public model catalog. Neither
turn made a plan-tool call or emitted a plan event, and task readback was null.
The model reported the tool unavailable, including after a Code Mode-specific
prompt; its assertion is not independent proof of tool configuration. Real
OpenCodex task-plan acceptance therefore remains open. Codex notification parsing,
cache and native rendering are verified with the deterministic provider only.
The existing OpenCodex proxy and user runtimes were not restarted or modified.

## Evidence and limits

Private raw logs, regression-before/after results, snapshots, native screenshots
and one consolidated receipt live under `/tmp/fleetpane-tasks-20261020` and
`/tmp/fleetpane-tasks-*.log`. The public
[receipt](assets/agent-tasks/receipt.json) records source/artifact identities,
covered behaviors and cleanup, without credentials or user session contents.
All task-owned Hosts/providers and the isolated desktop are stopped after checks;
temporary DSH credential copies are deleted. No commit or push was performed.

Plans are memory-only, bounded to 64 recently updated sessions and 1 MiB, with
128 entries/32 KiB per plan. Cache eviction/Host restart can leave no retained
plan; historical plan replay and editable checklists are unimplemented. Old Host
query refusal is surfaced by the panel, but was not exercised in a native old-Host
compatibility run. Remote CI, macOS/Windows native behavior, real multi-agent
capacity, hostile transport and long soak remain unverified. No comparative
performance claim is made. Further Git-diff work stays deferred by user request.
