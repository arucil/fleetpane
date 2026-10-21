# ACP permissions during cancellation — 2026-10-21

Implemented in the uncommitted working tree on base `c91d269`. The private
[receipt](assets/acp-cancel/receipt.json) binds source and binary hashes, failing
before checks, automated logs, native screenshots/readbacks and process cleanup.
Paseo scoped-path review now reaches ordinal 75; prior receipts remain unchanged.

## Root cause and change

The old adapter sent session/cancel but left outstanding permission requests
unanswered. Its regression failed with one wire frame instead of three. The
old built Host also timed out against the new deterministic provider, which
cannot settle until permission replies arrive. Neither failure used a real model.

The adapter now replies cancelled to all pending requests for that session,
removes their actionable Host requests, and immediately cancels requests arriving
while cancellation is pending. The session stays active until the provider prompt
response. Reads and status updates expose cancelRequested; repeated Stop produces
no duplicate wire frames, and stale approval responses are rejected. Other-session
permissions remain usable. Existing write/storage fences and no-retry behavior
remain in force. [ADR0036](../adr/0036-acp-permission-cancellation.md) documents
the current [ACP contract](https://agentclientprotocol.com/protocol/v1/cancellation).

## Automated checks

130 root tests (4 + 4 + 48 + 74), 9 desktop tests and 13 Python history tests
passed. Root/desktop Clippy with warnings denied, Windows GNU cross-target Clippy,
both builds, formatting and diff whitespace checks passed. Cross-compilation is
not native Windows acceptance; these are local checks, not remote CI results.

```sh
cargo test --workspace
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --locked --target x86_64-pc-windows-gnu -- -D warnings
cargo build --workspace --locked
cargo test --manifest-path apps/desktop/Cargo.toml
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked
python3 -m unittest discover -s scripts/tests -p '*_test.py'
python3 scripts/acp-cancel-smoke.py
```

The new process smoke covers two pending requests, a third in-flight request,
provider settlement held behind a gate, duplicate Stop, stale approval, independent
permissions on another session, archived confirmed cancellation and restart during
cancellation. That restart yields unknown rather than a fabricated cancelled
outcome, with no prompt/permission/cancel replay. The test is wired into CI.
Existing ACP lifecycle, stop, tools, chronology, metadata/commit failure, archive
and 20-stream concurrency process smokes also passed.

## Native acceptance

Used hidden Linux workspace `fleetpane-acp-cancel-20261021`, private fixture Host
and device store under `/tmp/fleetpane-acp-cancel-qa`. No shared user processes
or desktop were changed, and no model request was made.

- The first window showed two pending tool permissions. Typed and copied the
  exact Chinese/English multiline draft.
- Clicking Stop removed pending cards and changed the button to disabled
  Stopping… with an explicit waiting message. Turn stayed inProgress, Send stayed
  disabled and the draft was unchanged. Clicking the disabled Stop added no wire
  frames. Provider audit contained one cancel notification plus three cancelled
  replies, including the late request.
- Normal close exited 0 while the same Host/provider stayed alive. A new window
  and explicit Reload conversation recovered active/cancelRequested with no
  pending permissions, identical Host/provider/turn identities, the same exact
  copied draft and no additional cancellation frames.
- Releasing the provider settlement gate rendered its final cleanup message,
  interrupted/Cancelled and enabled Send. Native turn copy retained the exact
  cancelled stopReason. No draft was sent. The second normal close exited 0.

Host restarts and other-session isolation are process-test evidence; native
acceptance here covers window replacement against a surviving Host. Runtime logs
contain only the hidden X server's Mesa DRI3 warnings. Software-rendered checks
prove covered interactions, not performance. The task-owned Host/provider and
both windows are gone and the workspace is stopped.

This does not force termination of an unresponsive agent, implement Host process
takeover, add provider RPC cancellation beyond permissions, or establish new
real DeepSeek/OpenCodex behavior. Those remain separate acceptance work.
