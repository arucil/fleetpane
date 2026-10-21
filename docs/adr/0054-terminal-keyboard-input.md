# 0054 — Bounded terminal keyboard input

Status: accepted for the Linux prototype, 2026-10-21.

## Context

ADR 0030 provided a native screen and explicit input controls. Typing into the
screen still required a separate textarea. Sending each key through the existing
single-operation scheduler would reject input while an earlier request was busy.
Native testing also reproduced GPUI consuming Tab for focus navigation before
the screen's key event handler could receive it.

## Decision

Keyboard input is explicitly enabled for the selected terminal after taking Host
control. The screen owns a focus handle and a dedicated key context; context-bound
Tab and Shift-Tab actions override global focus navigation only there. Ordinary
key events encode UTF-8, control keys, arrows, function keys and cursor modes in
the toolkit-independent session crate. Native Ctrl-Shift-V reads text and uses
the observed bracketed-paste mode. Paste preserves newlines and indentation,
rejects embedded ESC and enforces the total buffer bound. Ctrl-Shift-C copies the
visible screen; native text selection remains separate work.

A 16 KiB queue counts both waiting bytes and the outstanding packet. Each packet
is at most the existing 4 KiB Host input limit and uses the existing typed input
mutation, expected input sequence, ownership fence and operation UUID. Only one
packet may be outstanding. Its bytes remain retained until a confirmed receipt;
there is no automatic resend. Additional keys join the queue while the packet is
pending. The queue pins the Host incarnation and terminal UUID. Ownership changes,
terminal faults, operation errors and overflow pause sending. Rejected new input
is reported; previously accepted bytes remain inspectable.

Queued bytes expose Copy, Discard and an explicit Resume after receipt resolution.
Copy distinguishes outstanding bytes that must not be resent from waiting bytes.
Selection and explicit mutations cannot interleave with an active keyboard queue.
A normal window close waits for accepted input to settle. Paused unsent bytes
keep the window open for explicit review. Like the earlier panel, a known pending
operation may be left with its durable Host receipt; presentation state is not
a second journal. Panel/window replacement keeps the Host and PTY alive and starts
with direct keyboard input disabled.

## Limits

The queue and panel pending-operation pointer are in memory. Forced termination
can lose unsent bytes; this is not durable local input recovery. Native evidence
covers Linux X11 with an owned real shell and synthetic provider. IME composition,
native selection, mouse reporting, terminal query replies, automatic geometry,
other platforms and complete terminal compatibility remain open. Host replacement
still cannot transfer an active PTY. No throughput or concurrent-session claim
follows from this development-build test.

See [verification](../verification/2026-10-21-terminal-keyboard.md).
