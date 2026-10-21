# Explicit session tree history navigation

The existing Codex tree inspector exposed related identities without an open
action. Paseo history 335–376 shows why a child route needs a return destination
and why stored parent metadata must survive every projection.

Fleetpane now opens a known tree node through the existing history command. A
target pins the Host incarnation, observed tree root, session and thread IDs,
parent, workspace and archive membership. Each click re-observes the bounded tree
and validates the destination and its return source before history navigation.
Unknown archive membership remains inspection-only. A pending check cannot move
a conversation after an intervening command, source dismissal or Host change.

The window retains one explicit return source; it is the session whose tree was
opened, which can differ from the immediate parent of a deeper descendant.
Switching through unrelated navigation clears it. Draft saving and explicit
control remain prerequisites. Opening history never resumes, answers, restores or
spawns. Existing archive membership passes through to the read-only UI. Detail
scroll resets on a session switch so previous scroll cannot hide a new notice.

Provider metadata observation is non-atomic. No claim is made about changes after
confirmation, a persisted navigation stack, root-only directory filtering, MCP
agent orchestration or ACP ancestry.
