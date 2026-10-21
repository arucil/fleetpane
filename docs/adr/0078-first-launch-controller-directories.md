# 0078: Create missing controller-store ancestors on first launch

Date: 2026-10-22. Status: accepted.

An actual fresh native launch failed to load appearance because the default
`fleetpane/preferences` path had no application parent. The controller Store
created only the final directory; all previous fixture paths supplied a parent.

When opening a local controller Store, create missing ancestor directories with
0700 on the existing blocking I/O boundary. Preserve existing ancestor modes and
system aliases. Keep the final directory's private/non-symlink validation and the
private regular database validation. Do not silently discard errors or change the
Host socket directory contract. Directory and database errors remain explicit.

This applies to controller stores generally so nested explicit state paths work
consistently with device preferences. Tests cover fresh parents, unchanged shared
parents, unsafe leaves and non-directory ancestors. Native preference save and
reopen are in the [verification](../verification/2026-10-22-tool-failure-first-start.md).
