# Fleetpane

A native Rust foundation for managing fleets of coding-agent sessions.

Fleetpane is an independent project, starting with a small read-only Paseo client.
The current build is a CLI and protocol library. A native desktop UI is planned;
there is no usable desktop replacement yet, and no performance gain is claimed.

## Try the directory client

With an existing Paseo daemon listening on loopback:

```sh
cargo run -p fleetpane-cli -- 127.0.0.1:6767
```

The CLI prints one page of up to 200 active sessions as JSON. If `pageInfo.hasMore`
is true, pass the returned `nextCursor` as the second argument for another page.
This is a one-shot snapshot. It requests neither timeline streams nor live
subscriptions, and does not start, stop or modify agents.

This prototype supports a directly accessible, unauthenticated loopback WebSocket
daemon only. TLS, relay connections and authentication are not implemented.
Output contains session metadata; use synthetic examples when sharing output.

## Workspace

| Package | Responsibility |
| --- | --- |
| `fleetpane-protocol` | Typed, forward-compatible directory projection and request encoding |
| `fleetpane-client` | Bounded WebSocket handshake and correlated directory request |
| `fleetpane-cli` | Inspect the same client that a future desktop UI will use |

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Tests use a local synthetic WebSocket daemon and need no provider accounts.
See [current behavior and gaps](docs/status.md) and the
[architecture decision](docs/adr/0001-independent-native-client.md).

## Rewrite workflow

The repository includes a [Rust native rewrite skill](.agents/skills/rust-native-rewrite/SKILL.md)
derived from publicly documented ArtCraft engineering practices. It is locally
authored and is not an official ArtCraft skill. Its reference notes pin the exact
public source revisions and distinguish borrowed ideas from our adaptations.

The main lessons are to keep the engine independent of the UI, verify actual
workflows through shared entry points, and compare complete release builds under
the same workload. Interface coverage alone does not establish replacement
readiness. [PhotoCraft's public guide](https://github.com/storytold/photocraft/blob/8d2e615ab93d424c4ec25ab15b4edb5368d1f9a6/AGENTS.md)
and [parity assessment](https://github.com/storytold/photocraft/blob/8d2e615ab93d424c4ec25ab15b4edb5368d1f9a6/docs/target-app-parity.md)
describe these practices.

Apache-2.0. Fleetpane is not affiliated with Paseo or ArtCraft.
