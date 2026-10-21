# Host-owned PTYs and byte cursors

Accepted for the Linux engine prototype, 2026-10-20. The user requested walking
Paseo from its root. Root `fee0f4fd2` exposes terminal control, `40b199e32` still
uses a global terminal namespace, and `853a73cc0` guesses settling from quiet
screen content. A later, separately inspected `37a363da6` introduces per-
conversation isolation. These are behavior references, not copied source.

One Session Host owns its PTYs; the controller/GUI and the provider do not.
Use `pty-process` (locked at 0.5.4, MIT license) through its safe async API; keep
our Rust code under `unsafe_code = forbid`. It wraps the platform PTY with a
Tokio child and a controlling terminal. See its
[API](https://docs.rs/pty-process/0.5.4/pty_process/).
No tmux daemon, additional per-terminal Node process or shell-string execution
wrapper is introduced. `/bin/sh -i`, with ENV/BASH_ENV removed, is the explicit
initial shell; configurable login shells are follow-up scope.

Typed commands are shared by the Session client and CLI. Stable terminal UUIDs
are scoped to the exact Host incarnation and workspace UUID; labels are mutable
and need not be unique. The saved workspace checks endpoint and directory
identity; the Host checks device/inode again. On Linux a held directory FD pins
the cwd through process spawn. The non-Linux path does not yet provide that
final rename-race guarantee and has no new runtime acceptance.

Terminal mutations use the existing durable admission/receipt journal, before
touching the PTY. Input additionally compares a monotonically increasing
sequence. A lost reply is queried by its operation UUID; it is never replayed
under a new UUID. Partial/timeout writes fence further input on that PTY, expose
the uncertainty, and allow reading/closing. An acknowledgement means bytes were
written, not that the shell executed or completed a command. Enter and control
keys are separate explicit bytes. No trimming or implicit newline is applied.

PTY actors continuously read raw bytes into a 64 KiB ring, with absolute byte
cursors and an exact dropped-byte count. Bytes are preserved even across invalid
UTF-8 or split codepoints; future emulators must decode incrementally and reset
when loss invalidates screen state. Child status supplies exit code/signal;
output EOF and read failure are separate observations. Closing stops the PTY's
foreground process group and its owned shell, then reaps the shell. Intentionally
detached/background jobs are not represented as an owned process tree.

Limit this foundation to 16 unclosed terminals per Host, 4 queued controls per
terminal and 4 KiB per input. A one-second write deadline bounds an unresponsive
terminal; completion runs outside the Host request loop. Terminal reads do not
reach the provider or claim a new controller. Existing terminals remain usable
during drain; creation is fenced. Live terminal IDs appear in Host activity so
draining output does not overlook them. Closed entries may be removed when a
new terminal is created to keep metadata bounded.

Raw output and shell existence are resident in the Host, not recovered from the
receipt journal after a Host restart. The receipt must never recreate that old
shell. Control replacement is covered; Host handoff/update, persistent emulation
state, conversation attachment, full descendant ownership, remote transport,
native rendering/selection/IME and alternate-screen behavior remain separate
required slices. No performance or replacement-release claim follows.

The provider-fault admission limitation observed on 2026-10-20 is superseded by
[ADR 0029](0029-independent-terminal-fault-domains.md). Provider failure now
preserves local PTY operations and receipts; Host/storage failure still fences
all mutations. Historical verification receipts retain the earlier limitation.
