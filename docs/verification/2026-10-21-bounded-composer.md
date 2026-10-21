# Bounded native composer — 2026-10-21

The uncommitted tree on base `c91d269` uses Kit auto-grow for message input,
with one to six rows and a three-row maximum below 680 pixels of window height.
[ADR0041](../adr/0041-bounded-native-composer.md) records layout ownership.
The [receipt](assets/bounded-composer/receipt.json) binds final source/binary hashes,
checks, native captures, provider readbacks and task-owned process cleanup.

## Source review and checks

Paseo ordinals 102–113 were inspected at the paths recorded in the
[ledger](../reference/paseo-replay.json). Full selected diffs and hashes are in the
[history receipt](assets/paseo-replay/lazy-and-composer/receipt.json). The 101-entry
prefix is unchanged; the complete reviewed SHA prefix agrees with the fixed
captured root index. This does not audit saved agent data, lockfiles or dependency
implementations, and 102 also excludes STT implementation paths. No historical
mobile/voice version was run. Ordinal 114 was prepared but not source-reviewed.

All 11 desktop tests pass. Desktop Clippy with warnings denied, locked build,
formatting and diff whitespace checks pass. All 13 history-tool tests pass.
The final layout/resize change received a fresh test, Clippy and build run.
No unit test merely mirrors the row breakpoint: acceptance exercises the real
Kit input, persistence, window resizing, scrolling and native action controls.
Root/provider code is unchanged in this slice; prior root/Windows/provider checks
are separate evidence. Remote CI and live model calls were not run.

```sh
cargo test --manifest-path apps/desktop/Cargo.toml --locked
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked
cargo fmt --manifest-path apps/desktop/Cargo.toml -- --check
python3 -m unittest discover -s scripts/tests -p '*_test.py' -v
git diff --check
```

## Native acceptance and regression

One private ACP fixture Host and two synthetic sessions ran in hidden workspace
fleetpane-composer-20261021. All prompts used the native Send control; no user
desktop, shared Host or live agent session was modified. Captures 01–22 include
intermediate candidates and failed layout experiments, not final acceptance.
Captures 23 onward use the final source-bound executable.

The initial auto-grow candidate reproduced Send below the bottom edge at the
supported 900×560 minimum size (09–11). The fixed layout gives the middle region
shrink/scroll ownership and keeps bottom controls from shrinking. A candidate
outer max-height clipped the frame without resizing Kit's text layout (22).
The final version changes the component's own row limit on window-bounds events.

The exact 20-line draft contains Chinese, doubled spaces and trailing spaces.
Its UTF-8 size is 1,019 bytes. It survives native window replacement while an
approval remains pending. At 900×560, the final textarea shows three rows;
scrolling reaches lines 18–20 (26). Native Select All/Copy returns the complete
original, matching the saved draft. Run/Skip and Stop remain reachable while the
long draft is retained (25–29). The provider records reject-one for request 10,
allow-one for request 11, and cancelled for request 12 after native Stop.
The Host then has no pending requests, no active turns and no fault.

Resizing to 1100×720 restores the six-row limit and back to 900×560 restores
three rows (30–31), keeping the same text, Host and provider. A native three-line
send with leading/trailing spaces is saved and delivered exactly, then the
composer shrinks to one row. Provider prompts contain only the intended review
prompt and that multiline send; layout, reconnect and copying do not send work.

Four native windows close normally with exit code 0. Cleanup stops only the
task-owned Host/provider and hidden workspace after recording readbacks. This
is Linux fixture acceptance, not a real DSH model, IME, other-platform or
performance acceptance claim. Attachments, session activity sorting, relative
time labels and complete Host retirement remain separate work. No minimum
timeline height is reserved: at the smallest window with a large composer,
reading the conversation may require growing the window. General pane sizing
remains open.

Final captures: [small pending request and long draft](assets/bounded-composer/25-final-small-permission.png),
[last three lines](assets/bounded-composer/26-final-small-text-end.png),
[after sending](assets/bounded-composer/35-final-sent-settled.png).
