# Modified terminal Enter — 2026-10-22

Paseo ordinals 1774/1794 identify a useful input distinction. Fleetpane's shared
keyboard encoder returned carriage return for Shift+Enter and Ctrl+Enter and
ESC plus carriage return for Alt+Enter. A new regression failed on Shift+Enter
before the fix. The encoder now emits `ESC [ 13 ; modifier u` for control/alt/
shift combinations, preserving ordinary Enter as carriage return and leaving
platform-modifier shortcuts outside terminal forwarding. Existing queue,
ownership, receipt and paste behavior is unchanged.

The regression covers all eight modifier combinations in normal and application
cursor modes, plus platform-modifier exclusion. Workspace tests pass 301 and
desktop tests pass 36. Workspace/desktop strict Clippy, format and builds pass;
`python3 scripts/terminal-smoke.py` passes real PTY isolation, input-sequence,
ownership, lost-reply and failure cases under synthetic Codex and ACP providers.

An isolated Linux X11 native window enabled Type in terminal for one owned real
PTY. Native Return, Shift+Return, Alt+Return, Ctrl+Return, Ctrl+Shift+Return,
Ctrl+Alt+Return, Alt+Shift+Return and Ctrl+Alt+Shift+Return followed by Ctrl+D
produced exactly the expected 51 bytes in the raw PTY capture. The terminal's
input sequence advanced from 1 to 10, with the same shell PID. Returning focus to
the explicit text editor made Shift+Return insert a newline; clipboard readback
matched the exact 22-byte Chinese draft and did not advance PTY input sequence.
The complete independent ACP snapshot, including its pending permission, stayed
identical. The app closed normally; the owned PTY, Hosts and isolated desktop
were stopped afterward.

[Check logs, capture source, raw readback and artifact digests](assets/modified-enter/receipt.json)
are separate from installed-provider or terminal-application acceptance. No
macOS/Windows, IME, terminal selection, CSI-u negotiation or application-level
keybinding acceptance is claimed. Programs decide how to interpret these bytes.
