# Focus the current controlled conversation composer

Paseo history 2452 introduces Cmd/Control+L for the message input. Fleetpane uses
the same discoverable platform shortcut at the Host conversation view boundary.
The Workbench owns session switching and file/terminal views; the Host listener
therefore cannot receive a sibling terminal's Control+L. An open provider picker
or pending question retains focus. Only a selected editable conversation with
current control and no in-flight local action is eligible.

The shortcut does not submit, replace or serialize the draft. It focuses the
retained Kit Textarea; the existing Send/Steer gate stays authoritative. Modified
keys, repeated held presses and marked-text composition do not invoke it.
Read-only ownership deliberately retains the active field's behavior. The native
hint names both focus and send bindings. Configurable bindings remain separate.

[Linux fixture acceptance](../verification/2026-10-22-composer-focus.md) includes
exact Codex/DeepSeek drafts, palette preservation, external takeover and normal
close/reopen. The IME gate is code-inspected; actual IME and other platforms have
not been accepted.
