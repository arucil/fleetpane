# Composer send shortcut

Paseo history ordinal 560 adds Ctrl/Cmd+Enter to send immediately. Fleetpane
retains its established multiline Enter behavior and adds the platform secondary
Enter shortcut to the focused conversation composer only.

Capture the Kit input action before the textarea inserts a newline. Idle sessions
use the existing Send command; an active Codex turn uses the existing explicit
Steer command with its expected turn ID. Apply the same availability checks as
the visible buttons, including controller ownership, saved-draft settlement,
Stop/drain, images and provider capability. An unavailable shortcut is consumed
without editing the draft. Other inputs and marked IME text keep input handling.

Do not interrupt a running turn, queue a new send, trim text, bypass approval or
clear drafts before the existing durable command settles. Keyboard dispatch is
presentation state; protocol and lifecycle behavior remain in shared commands.
