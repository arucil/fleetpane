#!/usr/bin/env python3
"""Owned provider: Stop RPC and exact-turn settlement are independent gates."""

import json
import os
import signal
import socket
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"


def wait(probe):
    deadline = time.monotonic() + 12
    while time.monotonic() < deadline:
        if result := probe():
            return result
        time.sleep(0.025)
    raise AssertionError("Stop progress fixture deadline")


def main():
    with tempfile.TemporaryDirectory(prefix="fp-stop-") as directory:
        root = Path(directory)
        control = root / "control.json"
        effects = root / "interrupts.jsonl"
        endpoint = root / "host/control.sock"

        def configure(mode, release=False):
            temporary = control.with_suffix(".tmp")
            temporary.write_text(json.dumps({"mode": mode, "release": release}))
            temporary.replace(control)

        def rpc(value):
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(5)
                stream.connect(str(endpoint))
                stream.sendall(json.dumps(value).encode() + b"\n")
                return json.loads(stream.makefile("rb").readline(1024 * 1024))

        def snapshot():
            return rpc({"command": "snapshot"})["data"]

        def command(*arguments):
            result = subprocess.run(
                [str(BINARY), "session", str(endpoint), "--take-control", *arguments],
                capture_output=True,
                text=True,
                timeout=15,
                check=True,
            )
            return json.loads(result.stdout)["result"]

        def execute(method, params, identity=None, lease=None):
            return rpc(
                {
                    "command": "execute",
                    "lease": lease or snapshot()["owner"],
                    "operation_id": identity or str(uuid.uuid4()),
                    "method": method,
                    "params": params,
                }
            )

        def claim():
            state = snapshot()
            return rpc(
                {
                    "command": "claim",
                    "protocol": state["protocol"],
                    "host_id": state["host_id"],
                    "expected_generation": state["owner"]["generation"],
                    "controller_id": str(uuid.uuid4()),
                }
            )["data"]

        def blocked(thread):
            for method in (
                "turn/start",
                "turn/steer",
                "thread/archive",
                "turn/interrupt",
            ):
                result = execute(method, {"threadId": thread, "turnId": "irrelevant"})
                assert (
                    result["response"] == "error"
                    and result["data"]["code"] == "stop_pending"
                ), result

        configure("ack-first")
        with (root / "host.log").open("w") as log:
            process = subprocess.Popen(
                [
                    str(BINARY),
                    "serve",
                    str(endpoint),
                    str(root),
                    sys.executable,
                    str(ROOT / "scripts/provider-fixture.py"),
                    "--stop-control",
                    str(control),
                    "--stop-effects",
                    str(effects),
                ],
                stdout=subprocess.DEVNULL,
                stderr=log,
                start_new_session=True,
            )
        try:
            wait(
                lambda: (
                    endpoint.exists() and "ready:" in (root / "host.log").read_text()
                )
            )
            thread = command("create", str(root))["thread"]["id"]
            other = command("create", str(root))["thread"]["id"]
            command("send", other, "[approval]")
            approval = wait(lambda: snapshot()["pending_requests"])[0]
            host_id = snapshot()["host_id"]
            for mode in ("ack-first", "completion-first", "reject"):
                configure(mode)
                turn = command("send", thread, "[wait]")["turn"]["id"]
                identity = str(uuid.uuid4())
                params = {"threadId": thread, "turnId": turn}
                assert (
                    execute("turn/interrupt", params, identity)["response"]
                    == "operation"
                )
                pending = wait(lambda: snapshot()["activity"]["stopping"])[0]
                if mode == "ack-first":
                    wait(
                        lambda: (
                            snapshot()["activity"]["stopping"][0]["phase"]
                            == "awaiting_completion"
                        )
                    )
                    assert {"thread_id": thread, "turn_id": turn} in snapshot()[
                        "activity"
                    ]["turns"]
                elif mode == "completion-first":
                    wait(
                        lambda: (
                            snapshot()["activity"]["stopping"][0]["terminal_status"]
                            == "interrupted"
                        )
                    )
                    assert all(
                        active["thread_id"] != thread
                        for active in snapshot()["activity"]["turns"]
                    )
                else:
                    wait(
                        lambda: (
                            snapshot()["activity"]["stopping"][0]["phase"] == "rejected"
                        )
                    )
                    assert (
                        "Synthetic stop rejection"
                        in snapshot()["activity"]["stopping"][0]["error"]
                    )
                    assert (
                        execute("turn/start", {"threadId": thread})["data"]["code"]
                        == "stop_pending"
                    )
                    configure("ack-first")
                    identity = str(uuid.uuid4())
                    execute("turn/interrupt", params, identity)
                    wait(
                        lambda: (
                            snapshot()["activity"]["stopping"][0]["phase"]
                            == "awaiting_completion"
                        )
                    )
                blocked(thread)
                old_lease = snapshot()["owner"]
                next_lease = claim()
                assert (
                    execute("turn/interrupt", params, identity, old_lease)["data"][
                        "code"
                    ]
                    == "stale_controller"
                )
                duplicate = execute("turn/interrupt", params, identity, next_lease)
                assert duplicate["response"] == "operation"
                assert snapshot()["activity"]["stopping"][0]["operation_id"] == identity
                assert snapshot()["pending_requests"] == [approval]
                count = len(effects.read_text().splitlines())
                execute("turn/interrupt", params, identity)
                assert len(effects.read_text().splitlines()) == count
                # Queries remain independent of the original mutation receipt.
                query = rpc(
                    {
                        "command": "query",
                        "lease": next_lease,
                        "method": "thread/read",
                        "params": {"threadId": other, "includeTurns": True},
                    }
                )
                assert query["response"] == "query", query
                configure(
                    "completion-first" if mode == "completion-first" else "ack-first",
                    True,
                )
                wait(lambda: not snapshot()["activity"]["stopping"])
                stale = execute("turn/interrupt", params)
                assert (
                    stale["response"] == "error"
                    and stale["data"]["code"] == "stop_target_changed"
                )
                assert snapshot()["pending_requests"] == [approval]
                assert pending["thread_id"] == thread
            assert len(effects.read_text().splitlines()) == 4
            # An unanswered Stop becomes unknown on provider loss, without replay.
            configure("unanswered")
            turn = command("send", thread, "[wait]")["turn"]["id"]
            execute("turn/interrupt", {"threadId": thread, "turnId": turn})
            wait(lambda: len(effects.read_text().splitlines()) == 5)
            provider_pid = snapshot()["provider_pid"]
            os.kill(provider_pid, signal.SIGTERM)
            wait(lambda: snapshot()["fault"])
            assert snapshot()["activity"]["stopping"][0]["phase"] == "unknown"
            assert snapshot()["host_id"] == host_id
            assert len(effects.read_text().splitlines()) == 5
            print(
                json.dumps(
                    {
                        "ack_first": "PASS",
                        "completion_first": "PASS",
                        "rejected_explicit_retry": "PASS",
                        "same_uuid_no_replay": "PASS",
                        "controller_takeover": "PASS",
                        "other_pending_approval_unchanged": "PASS",
                        "provider_loss_unknown": "PASS",
                        "provider_interrupt_frames": 5,
                    }
                )
            )
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGINT)
            process.wait(timeout=10)


if __name__ == "__main__":
    main()
