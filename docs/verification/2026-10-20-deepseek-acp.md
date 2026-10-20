# DeepSeek Harness ACP — 2026-10-20

Local implementation and acceptance, based on
`973b0ed883c25356a9f0849681333919d4497387` plus the uncommitted foundation.
Rust 1.98.1, GPUI Kit 0.7.3; Linux X11 with software Vulkan, hidden desktop
1100 × 720. No remote CI, release, performance or replacement-readiness claim.

## Protocol and regression checks

Installed DSH 0.2.0-rc.2 was inspected and probed with an independent `DSH_HOME`.
It negotiates ACP v1, supports new/list/resume/config/prompt/cancel and exact
permission options, and does not replay old transcript updates on resume.
The Host starts `dsh acp` directly; no shell/adapter-process bridge is involved.

A real controller-replacement check found that DSH's `session/list` intentionally
excludes loaded sessions. A newly opened desktop therefore showed zero sessions
while its Host still owned a running bash tool. The synthetic provider was changed
to model that exact inactive-only behavior: before the directory merge,
`python3 scripts/acp-smoke.py` failed at the first list readback (`IndexError`).
After the fix the same process test passes. A domain test checks one resident
plus 100 inactive rows: 100 first-page rows, one overflow row, then the original
upstream continuation, with no loss. Wrong-workspace and old-Host cursors fail.
Native refresh and controller replacement retain the real loaded session.

Other domain checks cover per-message chunk merging, partial tool updates retaining
input/title, media preservation, grouped configuration values, serialization of
configuration changes before prompts, exact permission validation, cancellation
settlement, explicit dispatch capacity and malformed known activity. A large-turn
regression confirms settlement emits bounded metadata rather than duplicating
all previously streamed items. Tool content decodes text/images and keeps unknown
terminal/diff references inspectable.

`acp-smoke.py` covers shared CLI create/list/read/resume, both configuration
options, unsupported command/value errors, one prompt execution for a duplicate
operation UUID, thoughts/text/tool updates, explicit permission across controller
replacement, stale/invalid answer rejection, cancellation, failure and completed
Host/provider restart. The restarted Host restores session settings and model
context while exposing an empty observed transcript, then accepts a new prompt.

Passed commands:

```sh
cargo fmt --all --check
cargo test --workspace --locked -j2
cargo clippy --workspace --all-targets --locked -j2 -- -D warnings
cargo build -p fleetpane-host --locked -j2
python3 scripts/acp-smoke.py
python3 scripts/runtime-smoke.py
python3 scripts/session-smoke.py
python3 scripts/query-smoke.py
python3 scripts/models-smoke.py
python3 scripts/directory-smoke.py
python3 scripts/history-smoke.py
python3 scripts/runtime-smoke.py --codex
python3 scripts/codex-history-smoke.py
cargo fmt --manifest-path apps/desktop/Cargo.toml --check
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -j2 -- -D warnings
cargo test --manifest-path apps/desktop/Cargo.toml --locked -j2
cargo build --manifest-path apps/desktop/Cargo.toml --locked -j2
uvx --offline ruff check scripts/acp-fixture.py scripts/acp-smoke.py
uvx --offline ruff format --check scripts/acp-fixture.py scripts/acp-smoke.py
git diff --check
```

52 workspace tests and 4 desktop tests passed. Codex transport/history regression
uses installed Codex 0.163.0 and a local SSE fixture, not an external model. ACP
process smoke is included in Unix CI steps; remote CI has not run. The final
desktop checks and native permission exercise were repeated after the permission
presentation change.

## Real DeepSeek and native workflow

A private disposable DSH home received a temporary copy of the existing credential
store. No user profile/plugins, existing sessions or running provider were adopted.
The only workspace file was synthetic `sample.txt`, containing
`Fleetpane ACP acceptance: compass=west`. DSH's default DeepSeek-v4-flash completed
an external model turn at Low effort: real `read` tool output, thought content and
Chinese Markdown with a heading/table. Reading this result through shared commands
confirmed the actual tool call and `stopReason: end_turn`.

After stopping this completed, owned Host, a new Host listed and resumed the saved
session. The desktop displayed the no-replay warning and no invented old items.
Using its native menu changed effort from Low to High. A native send asked to read
the same file again and compare with the preceding context. The real tool ran,
and the assistant rendered a Chinese heading, inline code/emphasis and a table
with `west`. Copy returned the exact original Markdown. The expanded read card
rendered its input and multiline output. Evidence:
[assistant/table](assets/acp-live-output.png), [tool details](assets/acp-live-tool.png),
[resume warning](assets/acp-resume-warning.png).

A real bash tool (`sleep 60`, followed by a synthetic marker) was then running
when its desktop closed. A replacement desktop required **Take control**, listed
the loaded session, and showed the same active bash turn. Host/provider PIDs
3473785/3473786 stayed unchanged. **Stop turn** sent cancellation; DSH settled with
`stopReason: cancelled`, the turn became `interrupted`, and the tool became
`failed`. Evidence: [active after handoff](assets/acp-real-handoff.png),
[cancelled](assets/acp-real-cancelled.png). This verifies provider settlement and
continuity across desktop replacement, not a completed control-daemon upgrade or
Host version migration.

A separate synthetic ACP provider exercised native create and model selection
(Synthetic A → B), and permission interaction. **Skip this tool** read back exactly
`reject-one`. A second pending request (ID 11) survived closing/reopening the
desktop and explicit takeover. **Run this tool** read back exactly `allow-one`;
the pending list cleared. The final UI shows the tool title/input and actual
option names rather than opaque protocol identities. Host/provider PIDs
3473796/3473814 remained unchanged. [Permission after handoff](assets/acp-permission-handoff.png).
This permission acceptance uses a deterministic provider; no real external-model
permission escalation was forced or approved.

The [public synthetic receipt](assets/acp-readback.json) records source/binary
identities, PID continuity, model/config readback and scope limits. Different
native stages used successive development builds; real model/tool/cancel checks
precede the final permission-description-only UI change. Private raw evidence is
under `/tmp/fleetpane-acp-20261020/`. Owned idle Hosts and the hidden desktop are
stopped after verification, their sockets removed, and the temporary credential
copy deleted. Existing user runtimes and the user display remain untouched.

## Limits

The native view retains only this Host's observed ACP activity. DSH context resume
does not replay old messages. No ACP history paging, elicitation, image upload,
account setup UI, remote connection or arbitrary ACP-agent acceptance is claimed.
Individual accumulated items/full observed reads can exceed the 256 KiB frame
bound before reaching the 2 MiB conversation cache bound. The 128-mutation lifetime
journal, in-memory recovery and other Host bounds remain. No performance comparison
was performed. See [ADR 0013](../adr/0013-deepseek-acp-provider.md).
