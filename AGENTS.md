# Contributor guidance

Read `docs/status.md` for current capabilities and gaps, and `docs/adr/` for
architecture decisions. For rewrite work use
`.agents/skills/rust-native-rewrite/SKILL.md`.

Keep domain/protocol code independent of the presentation toolkit. Public examples
and tests use synthetic data. Track provenance and licenses for reused code and
assets. Return useful boundary errors rather than hiding failures with defaults
or retries.

Verification commands are in README.md. Update behavior status alongside code;
only mark a workflow verified when it has corresponding evidence. Do not present
library tests, native compilation, and end-user runtime validation as equivalent.
