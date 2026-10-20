# Observational Host queries — 2026-10-20

Source base: `973b0ed883c25356a9f0849681333919d4497387`, with the uncommitted
foundation and this query slice. Rust 1.98.1, GPUI Kit 0.7.3, Linux X11 with
software Vulkan. This record covers local behavior, not remote CI or a release.

## Reproduction and automated checks

Before the fix, `python3 scripts/query-smoke.py` failed during its repeated
reads with `capacity: prototype operation journal is full; command was not
submitted`. The same test now completes 160 shared directory/model/metadata/
turn/item reads without increasing the journal, then sends a prompt twice under
the same operation UUID. The provider creates exactly one turn. It also rejects
stale controllers, resume/send/command execution and unknown methods on the query
path, exposes a provider read error, and processes an abandoned caller's response
without faulting the Host.

Runtime tests additionally cover 256 reads with all 128 operation slots already
occupied, preservation of the first mutation's deduplication identity, rejection
of a new mutation at capacity, the 32-query in-flight bound, malformed replies,
provider errors/failure and distinct response namespaces. A local Unix socket
test disconnects after receiving each request: a lost read reply returns a
refreshable transport error, while a lost mutation reply retains its original
pending UUID.

`python3 scripts/codex-history-smoke.py` passed against installed Codex 0.163.0
and a local deterministic SSE model fixture. After completed provider replacement,
it performs 160 shared reads across all five allowed methods, observes an unchanged
operation-ID list, then renames the saved thread and verifies the new name. Nested
command identity/output/exit status and bounded history remain correct. The test
uses two local model requests and executes its synthetic command once; it sends
no external LLM request and is not a new live OpenCodex acceptance run.

Passed checks:

```sh
cargo fmt --all --check
cargo test --workspace --locked -j2
cargo clippy --workspace --all-targets --locked -j2 -- -D warnings
cargo build -p fleetpane-host --locked -j2
python3 scripts/query-smoke.py
python3 scripts/runtime-smoke.py --codex
python3 scripts/session-smoke.py
python3 scripts/directory-smoke.py
python3 scripts/models-smoke.py
python3 scripts/history-smoke.py
python3 scripts/codex-history-smoke.py
cargo fmt --manifest-path apps/desktop/Cargo.toml --check
cargo test --manifest-path apps/desktop/Cargo.toml --locked -j2
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -j2 -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked -j2
uvx --offline ruff check scripts/query-smoke.py scripts/codex-history-smoke.py
uvx --offline ruff format --check scripts/query-smoke.py scripts/codex-history-smoke.py
git diff --check
```

Workspace tests: 44 passed; desktop tests: 4 passed. The synthetic query check is
added to the Unix CI steps; remote CI has not been run.

## Native workflow

Hidden workspace `fleetpane-query-20261020`, desktop 1100 × 720, with a disposable
synthetic provider and three saved turns. No user desktop, credentials, existing
Paseo runtime or existing user session was used.

Clicked **Reload conversation** 55 times. Each completed reload produced three
ordered query replies, advancing the Host sequence from 5 to 170. All 165 reads
left the operation-ID list empty. Loading both earlier turns reached the beginning
with six command entries; those four additional reads also left the journal empty.

Sent `连续读取后仍能发送` from the native composer. The turn completed, the assistant
rendered `Echo: 连续读取后仍能发送`, and its Copy button returned that exact text.
The journal contained one operation. Next, applied the fixture's `[list-fail]`
filter: a visible provider error retained the existing directory/conversation.
Sending `读取失败后继续发送` still completed; Copy returned
`Echo: 读取失败后继续发送`. A final query read back five completed turns: the three
seeded turns plus exactly two sends. No pending-mutation recovery was needed for
the failed read. The journal contained exactly two operations and no Host fault.

Host/provider PIDs remained 751906/751909 through the native checks; desktop PID
757898. SHA-256 read from the running `/proc/<pid>/exe` images matched the built
binaries:

- Host: `a8adab48a4999e2433de2c6752a2eeb4afd0fb5b0fbbfb50f00b49c412b7013d`.
- Desktop: `4ca253e97208d3c3620a33681cf64e5fa06219ed87164bbd9f47688a82ed4df3`.

Evidence: [after 165 reads](assets/queries-165-reads.png),
[completed send](assets/queries-completed-send.png),
[visible read error](assets/queries-read-error.png),
[send after read error](assets/queries-recovered-send.png),
and the [synthetic receipt](assets/queries-readback.json).
Private raw snapshots, provider events and readback are under
`/tmp/fleetpane-query-20261020/`. The owned idle Host is stopped with SIGINT and the
hidden workspace is stopped after acceptance; the receipt records cleanup.

## Remaining limits

The 128-entry lifetime limit still applies to `execute` requests, including reads
sent through that older/raw API. Mutation identities remain in memory only.
Durable admission/recovery/retention is a separate requirement. Thirty-two
unanswered provider queries can exhaust their own in-flight budget; disconnected
callers' slots remain until the provider responds or faults. Frame, event-ring,
conversation memory and large-item bounds are unchanged. This work does not
establish performance gains, remote transport support or DeepSeek Harness support.

See [ADR 0012](../adr/0012-observational-host-queries.md).
