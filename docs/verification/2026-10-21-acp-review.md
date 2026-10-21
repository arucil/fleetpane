# Review ACP permission details — 2026-10-21

Implemented in the uncommitted working tree on base `c91d269`. The private
[receipt](assets/acp-review/receipt.json) binds source/binary hashes, logs,
native screenshots, exact clipboard/provider readbacks and process cleanup.
Scoped-path Paseo review now reaches ordinal 77; earlier receipts stay unchanged.

## Root cause and change

The old native permission area preferred the observed tool title/input and ignored
the request's content/locations. A private reproduction showed Old operation and
old-command although the pending request proposed Review settings 中文 and new
file content. The retained before screenshot and Host request record this mismatch.

The common Rust session crate now decodes the permission ToolCallUpdate and
projects its supplied fields first. Missing/null fields may inherit only from the
matching tool in that session's active turn. Arbitrary JSON and collections replace
whole values, including false, zero and empty arrays. Standalone requests render
without an earlier tool event. Identity, content and locations are validated;
invalid native previews show an error and disable choices. Original request JSON
remains available through Copy. The preview does not modify observed tool history.

The native preview uses the existing retained tool renderer, starts expanded and
keeps its entity while the request is unchanged. Its detail viewport and the
surrounding interaction area have window-relative limits. Native testing found
that intrinsic content height must be retained inside the scroll area; otherwise
the review or its choices were clipped. The final small-window check exposes all
choices while leaving the composer and Stop accessible. [ADR0037](../adr/0037-acp-permission-review.md)
records the [ACP tool/permission contract](https://agentclientprotocol.com/protocol/v1/tool-calls).

## Automated checks

132 root tests (4 + 4 + 48 + 76), 9 desktop tests and 13 Python history tests pass.
Root/desktop Clippy with warnings denied, Windows GNU cross-target Clippy, both
builds, formatting and diff whitespace checks pass. These are local checks;
Windows compilation is not native Windows acceptance and remote CI was not run.

```sh
cargo test --workspace
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --locked --target x86_64-pc-windows-gnu -- -D warnings
cargo build --workspace --locked
cargo test --manifest-path apps/desktop/Cargo.toml
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked
python3 -m unittest discover -s scripts/tests -p '*_test.py'
python3 scripts/acp-review-smoke.py
```

The new process smoke covers request-only details, 180-line standalone content,
falsy input, exact CLI choice replies, stale-controller fencing, late answers,
another session's independent permission, unchanged observed tools, archive
round-trip and no prompt/request replay. It is wired into CI. Existing Codex
session/question, ACP lifecycle, cancellation, tools, metadata and ordering
process smokes also pass. The two new Rust tests cover the actual preview
projection, including replacement/clear semantics and active-turn boundaries.

## Native acceptance

Used hidden Linux workspace `fleetpane-acp-review-20261021`, private fixture Hosts
under `/tmp/fleetpane-acp-review-{before,qa,question-qa}` and isolated device stores.
No shared user Host, provider or desktop was changed; no real model was called.

- The final default-size window supports scrolling through the requested settings
  to exact choice buttons while the draft, Send and Stop remain visible.
- The maximized settings preview shows the new title, input, false/zero, proposed
  content and location. Native Copy returns the complete original request; Skip
  returns selected/reject-one. The old timeline tool remains unchanged.
- The next request has no earlier tool event. Its plan and content appear;
  independent scrolling reaches Review detail 179 中文. Copy preserves all 180
  lines. Switching to another session shows only that session's request and draft;
  switching back restores the main request and exact Chinese/English draft.
- Normal close exits 0 while Host/provider stay alive. A replacement window
  recovers the same pending requests and exact copied draft. Host, provider,
  observed turn/items and request JSON are unchanged. Native Run this tool then
  returns selected/allow-one to the private provider.
- The third request renders Input false and copies it without coercion. Stop
  returns cancelled, settles interrupted/Cancelled and preserves the unsent draft.
  The other session's permission remains pending and can then be skipped normally.
  The fixture never writes the proposed file; only the two intended prompts ran.
- A separate private Codex fixture checks the shared interaction layout: native
  Rust choice, Next, Chinese multiline answer, exact copy and Submit answers.
  Provider echo confirms both answers, including the existing user_note prefix;
  the request disappears and the turn completes.

All three final windows (main, replacement and question) close normally with
exit 0. Earlier reproduction/layout windows also stopped. The receipt confirms
all 16 task-owned process IDs are absent and the workspace is stopped. Runtime
logs contain only the hidden X server's Mesa DRI3 warnings. Software rendering
proves these interactions, not performance.

This does not implement durable permission audit, vendor-specific plan/file
execution, Git Diff expansion, file navigation, new permission policy, Host
process takeover or new real DeepSeek/OpenCodex acceptance. Full parity is open.
