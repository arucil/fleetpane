# Exact CLI prompt input

Local Linux Unix Host CLI validation, 2026-10-22. Uses private temporary Host and
synthetic Codex provider, without contacting installed providers or user sessions.

Commands:

```sh
cargo test -p fleetpane-host -p fleetpane-session
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p fleetpane-host
python3 scripts/prompt-input-smoke.py --evidence-dir docs/verification/assets/prompt-input
```

Host input parser: 2 tests passed. Shared session: 210 tests passed. Workspace
Clippy and Host build passed. Process smoke passed with exact file/stdin/explicit
and positional text sends, leading dash text and exactly 65,536 bytes. Readback
checks user-message content rather than only the command's acknowledgement.
CRLF, Chinese characters, indentation, spaces and final newline remain exact.
An active-turn file follow-up is read back from the same turn, then stopped.

Fifteen invalid input cases include conflicting sources, blank/invalid UTF-8,
oversized text, missing file and directory/FIFO input. They leave the complete
Host snapshot unchanged and produce no provider Send effects. FIFO rejection is
tested with no writer; the subprocess deadline bounds failure. Invalid follow-up
input on an absent socket reports the input error before any transport error.
Seven deliberate Send effects occurred; all owned processes and socket are gone.

The [receipt](assets/prompt-input/receipt.json) binds the binary and exact prompt
digests. No native UI changed. External OpenCodex/DeepSeek model calls, ACP input,
macOS/Windows and interactive stdin cancellation were not verified here.
