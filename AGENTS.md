# Contributor guidance

Read `docs/status.md` for current capabilities and gaps, and `docs/adr/` for
architecture decisions. For rewrite work use
`.agents/skills/rust-native-rewrite/SKILL.md`.

For history-guided Paseo work start with `docs/reference/paseo-replay.md` and its
linked review ledger. Keep indexed history, inspected changes, implementation
and runtime acceptance distinct; continue from the recorded full commit SHA.

Keep domain/protocol code independent of the presentation toolkit. Public examples
and tests use synthetic data. Track provenance and licenses for reused code and
assets. Return useful boundary errors rather than hiding failures with defaults
or retries.

Verification commands are in README.md. Update behavior status alongside code;
only mark a workflow verified when it has corresponding evidence. Do not present
library tests, native compilation, and end-user runtime validation as equivalent.
