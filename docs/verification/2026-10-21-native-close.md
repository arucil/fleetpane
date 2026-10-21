# Native close acceptance and automation diagnosis — 2026-10-21

Normal titlebar close exits the unchanged Linux desktop successfully. The
previous `close-window` automation is a different operation: the installed
Agent Workspace executable invokes `xdotool windowclose`, whose installed
manual says it destroys the X window without terminating its client. It
distinguishes this from `windowquit`, which sends a graceful close request.
The local [manual excerpt](assets/native-close/xdotool-close-contract.txt) and
[receipt](assets/native-close/receipt.json) retain the contract and binary hash.

In the same isolated X11 workspace, direct `close-window` destroys the Host
view's window but leaves process 2104137 alive after a three-second wait. It was
explicitly stopped through task-owned cleanup. A separate owned debugger run
found the main thread still in the event loop after window destruction. This
does not establish that every earlier cleanup used the same path.

Clicking the window manager's titlebar close button exits each of four ordinary
desktop processes with code 0. One uses the demo screen; three use the same
private ACP Host with an active turn blocked on permission. Independent snapshot
readback after each relevant close preserves the Host/provider PIDs, Host ID,
pending request, active turn and mutation journal count. No permission is answered
and no Stop, prompt replay or runtime adoption occurs.

An exact Unicode draft survives close and explicit session selection after
reopening; [native render](assets/native-close/reopen-steady.png) and clipboard
readback retain the initial text. A final edit uses an actual newline and
immediately closes through the titlebar; the process exits 0 and SQLite readback
contains the exact 47-byte multiline string, with no send attempt. This final
draft has storage acceptance; the initial reopened draft has native render/copy
acceptance. They are separate observations.

The isolated desktop and private fixture Host are cleaned up after evidence
capture. No application source changes are needed for this diagnosis. Earlier
records of forced cleanup remain historical facts; normal close now has its own
acceptance. Forced X window destruction remains non-graceful. No native macOS,
Windows, crash durability, filesystem failure or exit-latency guarantee is
claimed by this check.
