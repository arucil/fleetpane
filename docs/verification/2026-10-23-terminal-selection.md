# Native terminal selection — 2026-10-23

Mouse drags now select a fixed displayed terminal viewport. Output keeps flowing
to the existing emulator; Copy cannot silently move to later output. The
[contract](../adr/0122-bounded-terminal-selection.md) describes text extraction,
input gating and remaining limits. [Receipt](assets/terminal-selection/receipt.json)
binds source/binary identities, checks and native evidence.

## Verification

- `cargo test --workspace --locked`: 383 tests passed, including four new shared
  selection cases. They cover forward/reverse wide/combining cells, printed spaces
  versus unoccupied padding, hard lines, soft wraps, scrolled views and stable
  snapshots after new output, alternate-screen changes and resize.
- `cargo test --manifest-path apps/desktop/Cargo.toml --locked`: 51 passed.
- Session and desktop strict Clippy, both format checks, desktop development build,
  Windows engine cross-check and `git diff --check` passed. These are local checks;
  no new remote CI or other-platform GUI acceptance is claimed.
- `python3 scripts/terminal-smoke.py` passed with real owned PTYs under private
  Codex/ACP protocol fixtures. This preserves the existing ownership, input,
  lost-reply receipt, fault/storage and shell isolation contract independently of
  the native mouse checks.

The hidden Linux X11 workspace `fleetpane-terminal-selection-20261023` ran real
shells under a synthetic Codex-shaped provider. All data is synthetic. No existing
user Host/session was opened, restarted or claimed. The provider made no model calls.

1. Forward selection copied exactly `A中文 é  \nsecond row` (24 UTF-8 bytes).
   Reverse selection starting within a Chinese cell copied
   `中文 é  \nsecond row` (23 bytes), including its two printed trailing spaces.
   Both mouse button and Ctrl+Shift+C paths worked with Host control absent.
   [Final reverse selection](assets/terminal-selection/final-reverse-settled.png).
2. While that selection was retained, the shell printed 60 new numbered lines.
   Copy still returned the original 23 bytes, the complete Host snapshot was
   unchanged, and terminal identities/input counters were unchanged. Esc exposed
   the latest `AFTER_OUTPUT` screen. This was repeated on the final binary at
   900×560. [Fixed selection](assets/terminal-selection/final-frozen-small.png),
   [returned live output](assets/terminal-selection/final-live.png).
3. Page up selected scrollback, with button Copy returning
   `BACKGROUND-18\nBACKGROUND-19`. The original 88-byte wrapped line copied as one
   line without an inserted newline. ANSI-colored text copied exactly
   `alpha RED omega` (15 bytes), without escape sequences.
   [Soft wrap](assets/terminal-selection/final-soft-wrap.png),
   [styled selection](assets/terminal-selection/final-ansi.png).
4. A drag released left of the terminal content completed without retaining the
   drag gesture; Copy returned the intended two scrollback rows.
   [Outside release](assets/terminal-selection/final-outside.png). With direct
   keyboard input enabled, `a` and Tab during selection did not advance input
   sequence 3. After Esc, native Ctrl-C advanced it exactly once. Switching to the
   other shell cleared selection and left that shell at input sequence 0.
   [Other shell](assets/terminal-selection/final-other-shell.png).
5. Normal close and subsequent window replacement retained terminal UUIDs and
   shell PIDs 1415222/1415257 under Host PID 1415087. Input started disabled, and
   the old selection did not reappear. Selection/Copy worked again after reopening.
   All three native apps exited 0. The owned Host was then stopped; its socket and
   both shell processes disappeared. The hidden desktop was stopped.

The initial native trial used a candidate before the explicit mouse-up endpoint
update. Final apps `app-1635405` and `app-1663765` used the recorded final binary.
Reverse/Unicode, small-window Copy, background-output stability, input gating,
outside release, scrollback/wrap, styled Copy and close/reopen were rechecked on
that binary. The clipboard ledger distinguishes initial and final trials. The
full workspace suite was rerun after the final additional whitespace test; no
production code changed afterward.

## Provenance and limits

Read Paseo's `terminal-selection.ts` and its tests at the recorded reference
revision; no source/assets copied and no historical review cursor advanced.
Paseo history review progress remains separate from feature completeness.

Selection is a bounded, transient snapshot. This is not selection across unseen
scrollback or automatic drag-edge scrolling. Word selection, keyboard extension,
IME, accessibility, terminal mouse/query replies, automatic geometry, broader
terminal compatibility, other-platform GUI, active Host update transfer and
performance acceptance remain open. Alternate-screen/resize snapshot invariants
have shared engine tests, not new native acceptance. No new OpenCodex/DeepSeek
model turn was needed or claimed for this local PTY feature.
