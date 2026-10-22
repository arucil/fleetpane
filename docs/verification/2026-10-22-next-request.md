# Global next pending request

2026-10-22, base `830b16a`, Linux x86_64, GPUI Kit 0.7.3, isolated X11
desktop with Mesa software rendering and debug builds. [Receipt and raw
artifacts](assets/next-request/receipt.json) bind source/binary hashes, snapshots,
clipboard readback, build/test logs, setup/gate scripts and screenshots.

**Next request** and Ctrl+Shift+J refresh all configured Hosts and cycle through
exact pending sources. Queue type/Host filters remain independent. Source
confirmation still validates socket, Host incarnation and original complete
request; takeover and answers remain explicit. See [ADR 0109](../adr/0109-global-next-pending-request.md).

Native coverage uses synthetic Codex and two ACP providers with four pending
requests: a two-page question, a Codex approval and request `10` independently
pending on both ACP Hosts. A real PTY and four saved workspace tabs share the
same controller store. Coverage includes:

- Button and shortcut navigation, initial startup without clicking, complete
  cycles and wrap; exact question/session/request banners and read-only gates.
- A filter with no matches still permits global navigation to another Host/type.
  The filter remains saved in the open window. Empty global snapshots show an
  explicit notice alongside the unavailable Host; 900×560 controls are usable.
- A snapshot-only gated endpoint proves cancellation: switch to a workspace tab
  during the held read, release it and retain that tab. A gesture arriving during
  an older queue refresh waits for a distinct post-gesture read. On the final
  binary, ordinary Down selection cancels a held global gesture without opening
  a source. Existing five-second transport timeouts are unchanged.
- The session palette remains open when the global chord is pressed. Existing
  draft focus/Copy works after closing the palette by its Close button.
- Exact 38-byte Chinese/multiline/trailing-space draft survives source cycling,
  workspace tabs and normal window replacement, verified by native clipboard and
  private store readback. Explicit takeover after replacement remains required.
- With **Type in terminal** enabled on a real shell, the shortcut leaves its
  original PID `1766153`, input sequence `0` and all terminal Info fields unchanged.
- Initial navigation preserves Host/provider PIDs, owner, operation IDs/journal,
  pending requests, admission and outcomes. Only Codex's observational sequence
  advances from history reads. Final-binary read-only cycling preserves the same
  fields independently after normal replacement.
- An explicit synthetic permission decline reaches DeepSeek-A's provider store;
  DeepSeek-B's same-number request and snapshot remain unchanged. Subsequent
  navigation omits the resolved request. Remaining synthetic requests are answered
  explicitly through existing Host RPCs before verifying the empty queue.

Native testing exposed two focus defects: the initial workbench lacked a focused
key-dispatch target, and a structured question could leave focus on the removed
composer. The initial shortcut screenshot is unchanged before the first fix;
pre-fix question screenshots show the second lost navigation. The final binary
opens directly from startup and moves from the question to the Codex approval
without an intervening click. These are native regression observations, separate
from the pure cursor tests. Original question/permission controls can require
scrolling in the existing footer; this change accepts source navigation, not a
new complete question/permission layout.

Passed commands:

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --manifest-path apps/desktop/Cargo.toml --check
cargo test --manifest-path apps/desktop/Cargo.toml --locked
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked
git diff --check
```

328 workspace tests and 42 desktop tests pass. Cursor tests cover wrap, exact
identity across refresh/reorder/replacement, missing targets and empty/end
boundaries; desktop tests cover exact modifiers. Affected desktop checks rerun
after the final guard change. Owned desktop, hidden workspace, three Hosts,
providers, gate server and PTY terminate; Host sockets are removed.

No installed-provider/model run, IME, macOS/Windows, performance or hosted-CI
acceptance. Request creation times, notifications/deduplication and complete mute
remain separate work. Shared Paseo/agent processes and existing sessions were not
restarted. No claim of seamless Session Host update or replacement readiness.
