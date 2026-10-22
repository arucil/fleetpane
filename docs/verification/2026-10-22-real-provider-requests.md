# Real provider approvals and questions

2026-10-22, base `7f9e8ee`, Linux X11 hidden workspace, GPUI Kit 0.7.3,
Mesa software rendering and debug builds. Installed Codex 0.163.0 used the
unchanged existing OpenCodex 2.89.0 loopback proxy and `gpt-6.1-sol`. Installed
DeepSeek Harness 0.2.0-rc.2 used `deepseek-v4-flash`. Both used independent private
provider homes and synthetic workspaces. [Receipt](assets/real-provider-requests/receipt.json)
binds source/binary hashes, snapshots, histories, original protocol schemas,
scripts, test logs and screenshots.
Protocol schema artifacts were generated locally with
`codex app-server generate-json-schema --experimental --out /tmp/fp-real-request-schema`;
they describe the installed OpenAI Codex app-server contract.

Real approval testing found the fixed Codex buttons ignored `availableDecisions`.
The actual provider offered `accept`, an execpolicy amendment and `cancel`, while
the old UI sent `decline`. The new process regression fails on the old binary:
it returns `answered` for an unoffered decline and forgets the request. With the
fix, the Host rejects it before writing and retains the exact request, ownership,
activity, journal and provider identities. The same refusal is verified against
the new Host with an actual pending Codex request. See [ADR 0110](../adr/0110-provider-supplied-approval-decisions.md).

The primary desktop/Host binaries have these installed-model acceptance results:

- Both providers independently issued request `0`. Global navigation opens the
  correct Host/session source without changing either pending request, owner,
  process, admission, mutation journal or activity. Takeover remains explicit.
- Window close/reopen keeps both provider processes and exact requests;
  the new window starts read-only. A 35-byte Chinese/multiline/trailing-space draft
  survives replacement and is copied exactly through the native clipboard.
- At [900×560](assets/real-provider-requests/small-replaced.png), the Codex footer
  offers Allow once and Cancel · stop turn. ACP likewise retains its exact Allow
  once/Reject buttons. Review details scroll independently of those actions.
- Native Codex Cancel settles the original turn as `interrupted`; the requested
  `request-qa.txt` remains absent. DeepSeek's simultaneous same-number request
  and complete snapshot remain unchanged. Native ACP Reject reaches the real
  model as a failed permission tool and the turn completes without executing it.
- A second native Codex Allow once creates `allowed-qa.txt` with the exact
  `ALLOW-QA-20261022\n` bytes. Its cancelled predecessor remains absent and the
  other Host remains unchanged. Native ACP Allow once subsequently returns the
  exact requested printf marker and a successful tool result. No standing policy
  amendment was applied through the UI.
- A separate real Codex plan-mode turn calls `request_user_input` with two
  questions. At 900×560, no option starts selected; native scrolling, selecting
  Rust, Next, selecting Light and Submit answers complete successfully. Submission
  stays disabled while the second question is unanswered. The actual model reply
  is `Language: Rust (Recommended)` and `Theme: Light`; the original request clears
  and DeepSeek remains unchanged. Question forms still require scrolling.
- After settlement, global navigation reports an empty queue. The existing
  draft remains independently stored; no GUI Send/follow-up was used.

The first private Codex startup rejected the obsolete `untrusted` config value;
the working and final probes use `on-request` plus a read-only sandbox. A normal
DeepSeek workspace write did not request approval, so a separate explicit
permission probe requested broader sandbox access for a harmless printf.
The question harness initially used invalid Host command names; these returned
`invalid_request` and created no operation. The successful probe used the existing
`execute` command with the installed Codex schema's plan-mode settings. None of
these harness corrections introduced a product fallback or retry.

Passed verification commands:

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build -p fleetpane-host --locked
cargo fmt --manifest-path apps/desktop/Cargo.toml --check
cargo test --manifest-path apps/desktop/Cargo.toml --locked
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path apps/desktop/Cargo.toml --locked
python3 scripts/session-smoke.py
python3 scripts/runtime-smoke.py
git diff --check
```

331 workspace tests and 42 desktop tests pass. Shared tests cover offered order,
missing/null/empty choices, explicit session grants, unsupported variants,
malformed/duplicate/oversized lists and invalid responses. Synthetic process
checks cover invalid-response state preservation, cancel/accept delivery,
duplicate refusal and existing legacy approvals/questions/handoff.

Review additionally requires an object `params` payload; malformed params
cannot be treated as the missing legacy list. All listed static/unit/process
checks were rerun. Those binaries then repeated installed-provider native
cancel/reject, invalid-response refusal and minimum-size checks in new private
homes. Full allow/question/replacement observations above predate this
validation-only change. A further failing regression showed Serde's default
unit-enum decoding also accepted `{"accept":null}` as the string decision
`accept`. Decisions now deserialize strictly from strings; unit/process checks
cover refusal of that object without losing the request. All listed checks were
rerun after that change; its final binary has unit/process acceptance, and the
earlier installed-provider binary hashes are recorded separately in the receipt.

The MCP window-close operation removed each window but left its application
process running. This verifies view replacement and retained Host state, not
normal application exit. Workspace teardown subsequently stopped all four owned
desktop processes and their absence was read back. Normal-exit acceptance is not
claimed for this run.

Owned desktops, provider/Host processes and hidden workspace are stopped; private
DSH credential copies are removed. Shared Paseo, existing sessions and the shared
OpenCodex proxy are not restarted. Persistent exec/network policy amendments,
native session-grant semantics, file-change approvals, broader provider versions,
IME, macOS/Windows, performance, hosted CI and notification/full mute acceptance
remain open. These local workflows do not establish replacement readiness or
seamless Session Host updates.
