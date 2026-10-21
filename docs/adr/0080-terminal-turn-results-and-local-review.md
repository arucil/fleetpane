# Terminal turn results and local review

Accepted 2026-10-22.

Paseo history 308–317 introduces notifications and attention state. Fleetpane
needs to retain explicit turn outcomes independently of its short event ring,
while keeping approvals and questions actionable until the provider resolves them.

Each Host retains at most 64 completion records and 64 KiB of encoded records
for its current incarnation. Only explicit `turn/completed` events add records.
Thread and turn IDs, original terminal status and ACP stop reason remain intact;
observation time describes the Host's receipt, not provider execution time.
Duplicates preserve the first record. A conflicting retained outcome is an error.
Eviction is counted and visible. This feed is not persisted across Host restart.
The optional snapshot field distinguishes older Hosts from supported empty feeds.

The controller store upgrades transactionally from schema 11 to 12. Review receipts
bind the exact socket, Host UUID, event sequence and original record. Single and
batch review use the same engine from GUI and CLI. A batch commits atomically;
conflicts and storage errors retain the previous state. The 32,768-record capacity
returns an explicit error instead of silently deleting reviews. Reviewing never
changes provider outcomes or resolves pending requests. New results stay unread.

Attention has separate pending-request and turn-result views. Both observe all
configured Hosts and report unavailable/legacy endpoints. Source navigation
checks the original Host and retained record before selecting the source view;
control acquisition remains explicit. Opening history uses the normal bounded
inspect/page path, without resuming context. Loaded source turns suspend tail
following so the next layout does not jump away. Older pages require explicit
loading and normal retention limits still apply.

This is an explicit local review workflow. OS notifications, mute/tray settings,
background result aggregation, durable completion history and automatic reading
on session open remain separate work.
