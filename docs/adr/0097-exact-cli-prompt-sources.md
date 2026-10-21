# Exact CLI prompt sources

Status: accepted, 2026-10-22.

Paseo ordinal 1619 (`f12dc6127b6d4534c020eb16b75bdfb3c3a39a86`)
introduces a useful file-input workflow. Shell substitution removes trailing
newlines, and large prompts are inconvenient as command arguments.

The Unix Host CLI accepts exactly one positional text, `--prompt TEXT`,
`--prompt-file PATH`, or `--stdin` for Send and live follow-up. `-- TEXT` permits
literal text starting with a dash. Send retains provider-validated model/effort
options; follow-up uses its existing active turn.

Parse all arguments before reading any input. Read at most 65,537 bytes from a
file or stdin, then enforce the shared 65,536-byte prompt limit and nonblank
UTF-8. Never trim or normalize the accepted text. Open file descriptors with
nonblocking/no-controlling-terminal flags and validate the opened object's
metadata as a regular file; a path check before open would race replacement and
could hang on a FIFO. Input errors occur before snapshot or lease acquisition.

The shared typed command remains the only dispatch path. Public `validate()`
reuses that command's RPC validation without exposing the internal transport
mapping. Input errors do not produce operation receipts because no operation
was admitted; admitted Send/follow-up retain their existing exact UUID receipts.

This adds no implicit stdin detection, shell execution, background send queue,
new GUI workflow or ACP live follow-up support. Local synthetic CLI acceptance
is recorded separately from installed provider and other-platform acceptance.
