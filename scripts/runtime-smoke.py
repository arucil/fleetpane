#!/usr/bin/env python3
"""Process-level ownership handoff test. --codex adds isolated real command/exec.

Does not send LLM turns, read existing sessions, or require provider credentials.
Each request runs a new control client process that exits before the next one.
"""

import argparse
import json
import os
import signal
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path


def eventually(probe, description, seconds=20):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = probe()
        if value:
            return value
        time.sleep(0.05)
    raise AssertionError(f"deadline exceeded: {description}")


def exercise(host_binary, codex, drain=False):
    with tempfile.TemporaryDirectory(prefix="fleetpane-smoke-") as directory:
        root = Path(directory)
        socket = root / "host" / "control.sock"
        env = dict(os.environ, CODEX_HOME=str(root / "codex-home"))
        Path(env["CODEX_HOME"]).mkdir(mode=0o700)
        provider = (
            ["codex", "app-server", "--stdio"]
            if codex
            else [sys.executable, str(Path(__file__).with_name("provider-fixture.py"))]
        )
        command = [str(host_binary), "serve", str(socket), str(root), *provider]
        with (root / "host.log").open("w+") as log:
            process = subprocess.Popen(
                command,
                env=env,
                stdout=subprocess.DEVNULL,
                stderr=log,
                start_new_session=True,
            )
            try:

                def ready():
                    assert process.poll() is None, "host exited during startup"
                    log.flush()
                    return (
                        socket.exists()
                        and "fleetpane-host ready:" in (root / "host.log").read_text()
                    )

                eventually(ready, "host initialize handshake")

                def request(value):
                    result = subprocess.run(
                        [str(host_binary), "request", str(socket)],
                        input=json.dumps(value),
                        text=True,
                        capture_output=True,
                        timeout=8,
                        check=False,
                    )
                    assert result.stdout, result.stderr
                    response = json.loads(result.stdout)
                    assert result.returncode == (
                        1 if response["response"] == "error" else 0
                    ), result.stderr
                    return response

                def data(value, expected):
                    response = request(value)
                    assert response["response"] == expected, response
                    return response.get("data")

                snapshot = data({"command": "snapshot"}, "snapshot")
                assert snapshot["protocol"] == 3 and snapshot["fault"] is None
                assert socket.stat().st_mode & 0o777 == 0o600
                assert socket.parent.stat().st_mode & 0o777 == 0o700
                first = data(
                    {
                        "command": "claim",
                        "protocol": snapshot["protocol"],
                        "host_id": snapshot["host_id"],
                        "expected_generation": 0,
                        "controller_id": str(uuid.uuid4()),
                    },
                    "lease",
                )
                operation_id = str(uuid.uuid4())
                # A file gate proves work was running before transfer and finishes after it.
                child_code = """import os, pathlib, sys, time
root = pathlib.Path(sys.argv[1])
with (root / 'executions').open('a') as output: output.write('one\\n')
(root / 'started').write_text(str(os.getpid()))
deadline = time.monotonic() + 20
while not (root / 'release').exists():
    if time.monotonic() > deadline: raise RuntimeError('test did not release command')
    time.sleep(0.02)
print('completed:' + str(os.getpid()))
"""
                params = {
                    "command": [sys.executable, "-c", child_code, str(root)],
                    "cwd": str(root),
                    "timeoutMs": 25000,
                    "sandboxPolicy": {"type": "dangerFullAccess"},
                }
                execute = {
                    "command": "execute",
                    "lease": first,
                    "operation_id": operation_id,
                    "method": "command/exec",
                    "params": params,
                }
                accepted = data(execute, "operation")
                assert accepted["outcome"]["state"] == "accepted", accepted
                eventually(lambda: (root / "started").exists(), "command is running")
                child_pid = int((root / "started").read_text())
                os.kill(child_pid, 0)
                assert data({"command": "snapshot"}, "snapshot")["operation_ids"] == [
                    operation_id
                ]
                second = data(
                    {
                        "command": "claim",
                        "protocol": snapshot["protocol"],
                        "host_id": snapshot["host_id"],
                        "expected_generation": 1,
                        "controller_id": str(uuid.uuid4()),
                    },
                    "lease",
                )
                stale = request(dict(execute, operation_id=str(uuid.uuid4())))
                assert stale["data"]["code"] == "stale_controller", stale
                conflict = request(
                    {
                        "command": "claim",
                        "protocol": snapshot["protocol"],
                        "host_id": snapshot["host_id"],
                        "expected_generation": 1,
                        "controller_id": str(uuid.uuid4()),
                    }
                )
                assert conflict["data"]["code"] == "ownership_conflict"
                duplicate = data(dict(execute, lease=second), "operation")
                assert duplicate["id"] == operation_id
                current = data({"command": "snapshot"}, "snapshot")
                assert (current["host_pid"], current["provider_pid"]) == (
                    snapshot["host_pid"],
                    snapshot["provider_pid"],
                )
                os.kill(child_pid, 0)
                if drain:
                    admission = data(
                        {
                            "command": "set_admission",
                            "lease": second,
                            "expected_generation": 0,
                            "mode": "draining",
                        },
                        "admission",
                    )
                    assert admission == {"generation": 1, "mode": "draining"}
                    draining = data({"command": "snapshot"}, "snapshot")
                    assert draining["activity"]["in_flight_operations"] == 1
                    assert draining["admission"] == admission
                    blocked = request(
                        dict(execute, lease=second, operation_id=str(uuid.uuid4()))
                    )
                    assert blocked["data"]["code"] == "host_draining", blocked
                    assert (
                        data(dict(execute, lease=second), "operation")["id"]
                        == operation_id
                    )
                    os.kill(child_pid, 0)
                (root / "release").touch()

                def completed():
                    result = data(
                        {"command": "operation", "operation_id": operation_id},
                        "operation",
                    )
                    return result if result["outcome"]["state"] != "accepted" else None

                result = eventually(
                    completed, "accepted command completes under new controller"
                )
                assert result["outcome"]["state"] == "finished", result
                response = result["outcome"]["response"]
                assert response.get("result", {}).get("exitCode") == 0, response
                assert (
                    response["result"]["stdout"].strip() == f"completed:{child_pid}"
                ), response
                assert (root / "executions").read_text() == "one\n", (
                    "duplicate side effect"
                )
                if drain:
                    after_drain = data({"command": "snapshot"}, "snapshot")
                    assert after_drain["activity"]["in_flight_operations"] == 0
                    assert after_drain["admission"]["mode"] == "draining"
                    assert process.poll() is None
                    assert data(
                        {
                            "command": "set_admission",
                            "lease": second,
                            "expected_generation": 1,
                            "mode": "ready",
                        },
                        "admission",
                    ) == {"generation": 2, "mode": "ready"}
                events = data(
                    {"command": "events", "host_id": snapshot["host_id"], "after": 0},
                    "events",
                )
                assert any(
                    event["message"].get("id") == operation_id for event in events
                )

                if not codex:
                    approval_id = str(uuid.uuid4())
                    data(
                        {
                            "command": "execute",
                            "lease": second,
                            "operation_id": approval_id,
                            "method": "fixture/approval",
                            "params": {},
                        },
                        "operation",
                    )
                    eventually(
                        lambda: data({"command": "snapshot"}, "snapshot")[
                            "pending_requests"
                        ],
                        "pending approval",
                    )
                    third = data(
                        {
                            "command": "claim",
                            "protocol": snapshot["protocol"],
                            "host_id": snapshot["host_id"],
                            "expected_generation": 2,
                            "controller_id": str(uuid.uuid4()),
                        },
                        "lease",
                    )
                    pending = data({"command": "snapshot"}, "snapshot")[
                        "pending_requests"
                    ]
                    assert len(pending) == 1 and pending[0]["id"] == 999
                    decision = {
                        "command": "respond",
                        "lease": third,
                        "request_id": 999,
                        "result": {"decision": "decline"},
                    }
                    stale = request(dict(decision, lease=second))
                    assert stale["data"]["code"] == "stale_controller"
                    data(decision, "answered")
                    assert request(decision)["data"]["code"] == "unknown_request"
                    approval = eventually(
                        lambda: (
                            value
                            if (
                                value := data(
                                    {
                                        "command": "operation",
                                        "operation_id": approval_id,
                                    },
                                    "operation",
                                )
                            )["outcome"]["state"]
                            == "finished"
                            else None
                        ),
                        "approval answer reaches provider",
                    )
                    assert approval["outcome"]["response"]["result"] == {
                        "decision": "decline"
                    }

                # A second host must neither replace nor delete the live socket.
                duplicate_host = subprocess.run(
                    command,
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=5,
                    check=False,
                )
                assert duplicate_host.returncode != 0 and socket.exists()
                assert (
                    data({"command": "snapshot"}, "snapshot")["host_id"]
                    == snapshot["host_id"]
                )
                print(
                    json.dumps(
                        {
                            "provider": "codex" if codex else "fixture",
                            "host_pid": process.pid,
                            "provider_pid": snapshot["provider_pid"],
                            "command_pid": child_pid,
                            "drain_preserves_command": drain,
                            "controllers": 2 if codex else 3,
                            "one_side_effect": True,
                            "result": "PASS",
                        }
                    )
                )
            except BaseException:
                log.flush()
                print((root / "host.log").read_text(), file=sys.stderr)
                raise
            finally:
                if process.poll() is None:
                    process.send_signal(signal.SIGINT)
                    try:
                        process.wait(timeout=6)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait(timeout=5)
                # The fixture's command can still be waiting on its file gate
                # after a failed assertion. This is our isolated process group.
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                assert not socket.exists(), "host did not clean up its socket"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--host", type=Path, default=Path("target/debug/fleetpane-host")
    )
    parser.add_argument("--codex", action="store_true")
    parser.add_argument("--drain", action="store_true")
    arguments = parser.parse_args()
    if os.name != "posix":
        parser.error("Unix transport is required")
    exercise(arguments.host.resolve(), arguments.codex, arguments.drain)
