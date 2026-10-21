# Native terminal keyboard input — 2026-10-21

The terminal screen now accepts direct keyboard input through the existing fenced
Host input commands. [ADR 0054](../adr/0054-terminal-keyboard-input.md) describes
the bounded queue and error/close behavior. Source/artifact identities, raw checks
and native readbacks are in the [receipt](assets/terminal-keyboard/receipt.json).

## Verification

- `cargo test --workspace --locked`: 167 tests passed. The two new input cases
  check ordered 4 KiB packets, split UTF-8, atomic overflow refusal, pause/resume,
  retention of uncertain bytes, discard, key encodings and exact paste framing.
- `cargo test --manifest-path apps/desktop/Cargo.toml --locked`: 19 passed.
- Workspace and desktop strict Clippy, both format checks, Host/desktop builds
  and `git diff --check` passed. Build logs identify development artifacts.
- `python3 scripts/terminal-smoke.py`: real PTYs under Codex/ACP protocol fixtures
  passed, including ownership/input sequence, lost-reply receipts, isolation and
  exit. This checks the shared engine; it does not exercise GPUI key dispatch.

An owned hidden Linux X11 desktop, `fleetpane-keys-20261021`, ran the native app
and a real shell under a synthetic Codex-shaped provider. No existing user session
was opened, restarted or claimed.

1. Zero-delay native key events entered a command, which had no side effect until
   Return. It appended `raw-once` once. Backspace, Left, inserted `x` and End
   produced exact `AxB`; Up recalled shell history. Native Ctrl-C terminated the
   owned `sleep 60`, and a later command wrote `after-interrupt` in the same shell.
2. The first candidate exposed a real Tab defect: focus moved to Page up before
   the screen handler ran. A raw capture timed out, then restored terminal modes.
   [Failure](assets/terminal-keyboard/tab-before-fix.png). GPUI dispatch source
   confirmed key bindings run first. Context-specific Tab/Shift-Tab bindings fix
   that route; no global shortcut suppression was added.
3. The final binary ran a raw PTY capture enabling application cursor and bracketed
   paste. Native Up, Tab, Shift-Tab, F2 and Ctrl-Shift-V delivered exactly 52 bytes:
   `ESC O A`, tab, `ESC [ Z`, `ESC O Q`, then bracketed Chinese text containing a
   newline and two-space indentation. Exact bytes are retained in
   [readback](assets/terminal-keyboard/exact-readback.json) and the
   [capture program](assets/terminal-keyboard/capture.py).
   [Rendered result](assets/terminal-keyboard/final-raw-paste.png).
4. The owned Host was stopped briefly with SIGSTOP. A 6,035-byte native paste and
   Return produced 6,048 bytes including paste markers; 1,952 waited behind the
   outstanding 4,096-byte packet. Alt-F4 kept the
   window open with a waiting banner. SIGCONT let the queue settle; the command
   appended exact `QUEUED_OK` once and the app exited normally. See
   [waiting window](assets/terminal-keyboard/closing-queued.png).
5. Reopening retained Host PID 3104219 and shell PID 3109273, terminal UUID and
   input sequence. Keyboard input started disabled. After enabling it, zero-delay
   keys and cursor editing again produced exact `AxB` and appended `REOPEN_OK`
   once. [Reopened screen](assets/terminal-keyboard/final-reopened-keys.png).

Step 1 used the candidate before the Tab binding fix; its executable hash was not
captured. Steps 3–5 used the recorded final binary. Three desktop apps exited with
status 0. After readback, the owned Host was gracefully stopped; Host/provider/
shell PIDs and socket disappeared, and the hidden workspace stopped.

## History inspection and limits

Paseo review advances from 164 to 168 over 16 selected paths. The batch includes
tool replay metadata matching and its early-return correction, preview plumbing,
permission keying and mixed user/tool history. Missing-ID inference, private
provider history import and Git Diff expansion were not ported. The newly added
historical hydration fixture lacks the bodies its assertions expect; it was not
run and does not establish successful reconstruction. See the
[separate review receipt](assets/paseo-replay/terminal-keyboard/receipt.json).
Direct keyboard input fills the earlier terminal/PRD gap independently of this
batch; it is not a direct port of commits 165–168.

No native overflow, uncertain-delivery recovery, ownership-loss interaction, IME,
selection, mouse/query replies, macOS/Windows/Wayland, fresh external provider,
remote CI or performance acceptance is claimed here. Queue failure cases have
core checks and existing Host process evidence only. Forced app termination can
lose unsent keyboard bytes. Active PTY transfer across a Host update remains open.
