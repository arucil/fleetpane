# Focus the conversation composer — 2026-10-22

Ctrl+L (Cmd+L on macOS) focuses the editable composer of the controlled, selected
conversation. The hint now exposes the shortcut. It applies independently of
sidebar visibility, keeps session/model palettes and pending question input in
place, and declines during composition, held keys, busy operations, or read-only
ownership. The existing Send/Steer command remains the submission boundary.

Linux X11 native checks cover Codex and DeepSeek synthetic Hosts, search-to-composer,
model and session palette focus, and 900×560. Exact clipboard readbacks are 21-byte
ACP and 32-byte Codex Chinese multiline drafts, retained after normal close/reopen.
After a separate CLI takes control, Ctrl+L retains the session filter focus;
copy reads `leasequery`. The provider PIDs, operation IDs/count, pending request,
active turn inventory and fault state match the pre-check snapshots.

36 desktop tests, strict all-target Clippy, format check and native build pass.
[Source/binary identity and artifacts](assets/composer-focus/receipt.json) retain
screenshots, snapshots and check output. This verifies fixture behavior, not real
provider turns, IME, Wayland or other platforms. The binding is not configurable.
