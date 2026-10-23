# Native terminal words, lines and Shift extension — 2026-10-23

The fixed terminal viewport now supports word/line gestures and Shift extension.
The [contract](../adr/0123-terminal-selection-units.md) defines boundaries and
input isolation; the [receipt](assets/terminal-selection-units/receipt.json)
records source/binary hashes, local checks, native apps and evidence identities.

## Local checks

- `cargo test --workspace --locked`: 387 passed. Eight shared selection tests
  cover existing snapshots plus words, paths/flags, separators, wide/combining
  cells, forward/reverse unit ranges, logical soft wraps, hard lines, printed
  spaces, viewport clipping and bounded keyboard movement.
- `cargo test --manifest-path apps/desktop/Cargo.toml --locked`: 52 passed.
  The new no-repaint gesture regression fails with the previous focus-click clear
  operation and passes with the corrected state transition. It also checks an
  intervening emulator rewrite, triple click, Shift extension and clearing.
- Session and desktop strict Clippy, both format checks, desktop development
  build, Windows engine cross-check and `git diff --check` passed.
- `python3 scripts/terminal-smoke.py` passed real owned PTYs with private Codex
  and ACP protocol fixtures, including ownership/input receipts and fault fences.
  This is independent of the native pointer checks below.

These are local results, not new remote CI or other-platform GUI acceptance.

## Native acceptance

The hidden Linux X11 workspace `fleetpane-terminal-units-20261023` used a private
synthetic Codex-shaped provider and real shells. No existing user Host/session
was claimed, opened or restarted, and no model call was made. The initial app
`app-1509490` exposed the rapid-click race; every acceptance result below uses
the fixed final binary in `app-1548000` or replacement `app-1581263`.

1. Without taking Host control, double-click and Ctrl+Shift+C copied
   `src/main.rs` (11 bytes), `--flag=中文` (13) and `é` (3). Button Copy and
   triple-click copied the first line with its two printed trailing spaces
   (33 bytes), ANSI-colored `second row: alpha RED omega` (27), and the 88-byte
   wrapped line as one line. [Word](assets/terminal-selection-units/final-word.png),
   [line](assets/terminal-selection-units/final-line.png).
2. Forward and reverse double-click drags both copied the same 54-byte range
   through the complete styled `RED` word on the next row.
   [Word drag](assets/terminal-selection-units/final-word-drag.png).
   Shift+Right/End/Down/Home returned exact expected 12/32/60/34-byte ranges;
   Shift+click returned a 26-byte range ending at the complete Chinese parameter.
   [Keyboard](assets/terminal-selection-units/final-keyboard.png),
   [Shift+click](assets/terminal-selection-units/final-shift-click.png).
3. At 900×560 the existing 54-byte range remained available. The shell printed
   60 new lines while Copy still returned those same bytes. Full Host and shell
   snapshots stayed equal to their recorded originals, including control owner,
   mutation receipts, PIDs and input counters.
   [Small window](assets/terminal-selection-units/final-small.png),
   [clipboard ledger](assets/terminal-selection-units/clipboard.json).
   Esc exposed later output. The physically short panel displays only part of
   the emulated viewport; the fixed selection still includes its retained rows.
4. After explicitly taking control and enabling terminal keyboard input, word
   selection plus Shift+Right, `a`, Tab and Ctrl+Shift+V left input sequence at 1.
   Esc followed by Ctrl-C advanced it to 2 exactly once. The other shell remained
   at sequence 0; switching to it cleared selection and disabled direct input.
   [Blocked input](assets/terminal-selection-units/final-blocked-small.png),
   [other shell](assets/terminal-selection-units/final-other.png).
5. Normal close while selected and a replacement window retained Host PID
   1506266 and shell PIDs 1506283/1506288 with the same terminal UUIDs and input
   sequences 2/0. Selection did not return, direct input started disabled, and
   double-click Copy worked again without control in the replacement window.
   [Replacement](assets/terminal-selection-units/final-replacement.png).
   Both final apps exited 0. The owned Host, shells and socket were removed,
   and the hidden desktop was stopped.

Window-targeted workspace tools drove clicks, keys, resize and Copy. A small
workspace-owned `xdotool` helper expressed held Shift and held-button unit drags
against that same isolated window. It did not target the host desktop.

## Provenance and limits

Paseo's pinned word resolver supplied behavioral context for separators and
physical-row words. Rust code/tests are new; no source/assets copied and no
historical review cursor or upstream-freshness claim advanced. Logical-line
triple-click and Shift extension follow the recorded Fleetpane contract.

Selection remains a bounded transient snapshot. Keyboard extension needs an
existing selection; keyboard-only initiation, drag-edge autoscroll, unseen
scrollback selection, IME, accessibility, terminal mouse/query replies,
automatic geometry, other-platform GUI and active Host update transfer remain
open. No performance comparison or new OpenCodex/DeepSeek model result is claimed.
