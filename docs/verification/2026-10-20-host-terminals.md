# Host-owned PTY foundation verification

2026-10-20, Linux. Local base remains
`c91d269247207a08898e264c81083df4fbd4e745` plus the earlier uncommitted slices
and this implementation. No commit, push, remote CI or release was performed.

## Implemented boundary

Following the root-history review, added actual Host-owned PTYs and a shared
Session terminal API/CLI. Terminal results have their own typed command domain;
they are not decoded as conversation results. Creation binds the saved workspace
and verifies its directory inode at both boundaries. Linux spawn uses a held
directory FD for cwd. The Host, not a short-lived CLI or provider, owns the shell.

Create, exact input, explicit keys, resize, rename and close use the existing
durable receipt journal. Input sequences reject duplicates even under a fresh
operation UUID; reuse of a saved UUID returns its original result. A write
timeout/partial write is explicit and fences later input. Output is a raw 64 KiB
ring with byte cursors, exact drop counts, EOF and read faults. Shell wait status
supplies code/signal. Limits: 16 unclosed terminals, 4 queued controls each,
4 KiB input each. Drain rejects creation and preserves existing terminal actions;
Host activity includes live terminal IDs.

## Local evidence

All passed on the final Rust source:

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build -p fleetpane-host --locked
cargo test --manifest-path apps/desktop/Cargo.toml --locked
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo fmt --all --check
cargo fmt --manifest-path apps/desktop/Cargo.toml --check
python3 scripts/terminal-smoke.py
python3 scripts/session-smoke.py
python3 scripts/drain-smoke.py
python3 scripts/acp-smoke.py
python3 -m unittest discover -s scripts/tests -p '*_test.py'
uvx --offline ruff check scripts
uvx --offline ruff format --check scripts/terminal-smoke.py
git diff --check
```

101 engine/workspace Rust tests and 7 desktop Rust tests passed; the two new
unit checks cover raw byte retention/cursors and input/label/size/drain bounds.
All 13 history-reader Python tests also pass. Initial desktop integration errors
were corrected before the final tests/Clippy; earlier failed runs are not passes.

The new smoke uses **real `/bin/sh` PTYs** with independently owned Codex and
DeepSeek ACP **protocol fixture providers**, no real model request. It verifies:

- Saved real Unicode cwd, exact Chinese/literal-shell-syntax text, explicit Enter
  (no command runs before it), multiline input and `stty` dimensions.
- Renaming keeps terminal ID/PID; repeated CLI exits and explicit controller
  takeovers keep the same Host/provider/shell. Stale leases are fenced. Other
  workspace/Host identities cannot read the terminal; reads do not claim control.
- A separate shell's foreground `sleep` is stopped on Close; the original shell
  remains usable. Explicit Ctrl-C interrupts a foreground wait. The intentionally
  spawned background job is explicitly cleaned up by the test.
- The 16-terminal limit rejects creation and closing frees slots. Drain rejects
  new PTYs and preserves existing input. Host activity retains live IDs.
- A non-reading process fills the input queue: timeout fences more input while a
  concurrent Host snapshot still returns. The 0.75-second test deadline is a local
  liveness assertion, not a measured release latency budget.
- A 100,000-byte flood retains exactly 65,536 bytes and reports loss. Reading at
  the current cursor returns no bytes. Invalid UTF-8/whitespace retention is also
  covered by the unit check.
- A deliberately lost input reply is recovered from the durable receipt;
  submitting the exact operation UUID again produces only one file append.
- `exit 7` reports the actual code, reaches output EOF, and reaps the shell.
  Replacing the saved workspace directory refuses creation.
- Provider loss leaves terminal output/shell readable but currently rejects
  terminal mutations through the shared provider fault fence. Host teardown
  terminates its remaining owned shell; this limitation is reproduced explicitly.

The [receipt](assets/terminals/receipt.json) binds source/binary/log hashes and
separately records cleanup. Raw outputs are retained beside it. All resources
belong to the smoke's temporary private directory; shared Paseo/Codex/DSH
processes were not adopted or restarted.

## History and acceptance limits

Continuous **selected-path inspection** advances from ordinals 19 to 27,
`65c6f81605347ee4a77a7e90e5b9772314eadbde`; next is ordinal 28,
`14230c681f8e1cf3d738392dbb6222df4a533a97`. Ledger SHAs/ordinals were checked
against the pinned 6,187-entry root index. The ledger names paths actually read;
this is not a full audit of every changed file, prompt, dependency or old test.
The later per-conversation commit `37a363da6` was inspected for design context,
but has not been reached by continuous review.

**Partial workflow, not terminal parity.** No native emulator was added/launched
or visually accepted this turn. Alternate screen, ANSI screen state, selection,
scroll/copy, IME, configurable shells and conversation attachment remain missing.
Desktop compilation/tests do not establish those behaviors. Terminal state is
not recovered across a Host restart/update; receipts never recreate a lost shell.
Provider-fault-independent terminal mutation/cleanup is also unfinished. Close
targets the current foreground process group and owned shell, not a complete
detached process tree. macOS/Windows, real-provider coexistence, large-fleet
performance and long-running soak were not verified for this slice.

See [ADR 0028](../adr/0028-host-owned-pty-foundation.md). Next work continues
at commit 28 and completes the terminal interaction/lifecycle slices.
