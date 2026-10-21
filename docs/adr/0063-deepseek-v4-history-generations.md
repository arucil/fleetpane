# Select authoritative DeepSeek history generations

Accepted 2026-10-21. Extends [offline migration](0059-offline-paseo-workspace-history-import.md).

The installed DeepSeek Harness uses physical format v4. Its released v4 codec
preserves v3 row framing but carries tool results as native tool-role messages.
The importer already projects their content arrays and preserves every original
record. Its hardcoded v3 filenames and header check were the missing boundary:
v4-only histories failed discovery, and coexisting v3/v4 files silently imported
the obsolete v3 generation.

The shared archive engine now enumerates canonical generation filenames in the
bound provider session directory and selects the numerically highest generation.
It supports v3 and v4, plain JSONL or concatenated Zstandard frames. The chosen
generation must match the header version and the existing bound session ID/cwd.
It retains the selected filename version separately from its canonicalized path.
The directory and selected target must remain within the configured provider root.

Generation syntax follows DSH's canonical parser: `session.jsonl` is v0, positive
safe-integer versions use `session.vN.jsonl`, with optional `.zstd`. Leading zeroes,
temporary files and noncanonical names do not identify generations. Enumeration
is bounded to 1,024 entries. Mixed plain/compressed canonical generations are
refused, matching DSH's encoding ambiguity rule. An unsupported newest version,
corrupt file or mismatched header fails explicitly; no older-generation fallback.

No provider SDK becomes a runtime dependency. Optional smoke validation uses the
locally installed released v4 encoder/decoder with synthetic events. It verifies
physical framing and native tool-role admission, not a runnable session lifecycle.
Full DSH semantic validation, synchronization with concurrent source writes and
future format support remain separate work.

Existing private snapshots keep their original bytes and never refresh implicitly.
An archive copied earlier from v3 remains that snapshot even if v4 later appears.
Updating an already imported archive needs a separate explicit refresh contract.
This change supports history browsing; DeepSeek context continuation remains open.

[Verification](../verification/2026-10-21-paseo-dsh-v4.md).
