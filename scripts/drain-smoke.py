#!/usr/bin/env python3
"""Owned old/new Hosts: persistent drain, exact active work and explicit new work.

No auto-migration, shared provider home, credentials or external model requests.
"""

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


def wait(probe, label):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        if value := probe():
            return value
        time.sleep(0.025)
    raise AssertionError(label)


def exercise(acp):
    with tempfile.TemporaryDirectory(prefix="fp-drain-") as directory:
        root = Path(directory)
        hosts = []

        def raw(index, value):
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(5)
                stream.connect(str(hosts[index]["socket"]))
                stream.sendall(json.dumps(value).encode() + b"\n")
                return json.loads(stream.makefile("rb").readline(1024 * 1024))

        def data(index, value, expected):
            result = raw(index, value)
            assert result["response"] == expected, result
            if expected == "answered":
                assert result == {"response": "answered"}, result
                return None
            return result["data"]

        def snapshot(index):
            state = data(index, {"command": "snapshot"}, "snapshot")
            assert state["fault"] is None, state
            return state

        def claim(index):
            state = snapshot(index)
            lease = data(
                index,
                {
                    "command": "claim",
                    "protocol": state["protocol"],
                    "host_id": state["host_id"],
                    "expected_generation": (state["owner"] or {}).get("generation", 0),
                    "controller_id": str(uuid.uuid4()),
                },
                "lease",
            )
            hosts[index]["lease"] = lease
            return lease

        def execute(index, method, params):
            identity = str(uuid.uuid4())
            value = {
                "command": "execute",
                "lease": hosts[index]["lease"],
                "operation_id": identity,
                "method": method,
                "params": params,
            }
            data(index, value, "operation")

            def settled():
                result = data(
                    index,
                    {"command": "operation", "operation_id": identity},
                    "operation",
                )
                return result if result["outcome"]["state"] != "accepted" else None

            receipt = wait(settled, method)
            assert receipt["outcome"]["state"] == "finished", receipt
            assert "error" not in receipt["outcome"]["response"], receipt
            return value, receipt["outcome"]["response"]["result"]

        def query(index, method, params):
            response = data(
                index,
                {
                    "command": "query",
                    "lease": hosts[index]["lease"],
                    "method": method,
                    "params": params,
                },
                "query",
            )["message"]
            assert "error" not in response, response
            return response["result"]

        def start(index=None):
            if index is None:
                index = len(hosts)
                home = root / str(index)
                home.mkdir(mode=0o700)
                control = home / "host" / "control.sock"
                provider = (
                    ROOT
                    / "scripts"
                    / ("acp-fixture.py" if acp else "provider-fixture.py")
                )
                command = [
                    str(BINARY),
                    "serve-acp" if acp else "serve",
                    str(control),
                    str(home),
                    sys.executable,
                    str(provider),
                ]
                hosts.append({"command": command, "socket": control, "home": home})
            host = hosts[index]
            log = (host["home"] / f"host-{uuid.uuid4()}.log").open("w+")
            process = subprocess.Popen(
                host["command"],
                stdout=subprocess.DEVNULL,
                stderr=log,
                start_new_session=True,
            )
            host["process"] = process
            host["log"] = log

            def ready():
                assert process.poll() is None, Path(log.name).read_text()
                return (
                    host["socket"].exists()
                    and "fleetpane-host ready:" in Path(log.name).read_text()
                )

            wait(ready, "initialization")
            claim(index)
            return index

        def stop(index, crash=False):
            host = hosts[index]
            process = host["process"]
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL if crash else signal.SIGINT)
                process.wait(timeout=10)
            host["log"].close()

        def admission(index, mode, expected=None, lease=None):
            state = snapshot(index)
            return raw(
                index,
                {
                    "command": "set_admission",
                    "lease": lease or hosts[index]["lease"],
                    "expected_generation": state["admission"]["generation"]
                    if expected is None
                    else expected,
                    "mode": mode,
                },
            )

        def reject_new(index, method, params):
            before = snapshot(index)["journal_total"]
            result = raw(
                index,
                {
                    "command": "execute",
                    "lease": hosts[index]["lease"],
                    "operation_id": str(uuid.uuid4()),
                    "method": method,
                    "params": params,
                },
            )
            assert result["data"]["code"] == "host_draining", result
            assert snapshot(index)["journal_total"] == before, (
                "rejected work must not consume an identity"
            )

        def turn_status(index, thread):
            return query(
                index,
                "thread/turns/list",
                {
                    "threadId": thread,
                    "limit": 1,
                    "itemsView": "notLoaded",
                    "sortDirection": "desc",
                },
            )["data"][0]["status"]

        try:
            old = start()
            new = start()
            before = snapshot(old)
            _create, thread = execute(
                old,
                "thread/start",
                {"cwd": str(hosts[old]["home"]), "historyMode": "paginated"},
            )
            identity = thread["thread"]["id"]
            # Keep real work on the original provider while draining admission.
            prompt = "[permission]" if acp else "[wait]"
            send, result = execute(
                old,
                "turn/start",
                {"threadId": identity, "input": [{"type": "text", "text": prompt}]},
            )
            turn = result["turn"]["id"]
            wait(lambda: snapshot(old)["activity"]["turns"], "observed active turn")
            if acp:
                wait(
                    lambda: snapshot(old)["pending_requests"],
                    "pending exact permission",
                )
            drained = admission(old, "draining")
            assert drained["response"] == "admission" and drained["data"] == {
                "generation": 1,
                "mode": "draining",
            }, drained
            assert (
                admission(old, "ready", expected=0)["data"]["code"]
                == "admission_conflict"
            )
            stale = hosts[old]["lease"]
            claim(old)
            assert (
                admission(old, "ready", lease=stale)["data"]["code"]
                == "stale_controller"
            )
            assert snapshot(old)["admission"] == drained["data"]
            for method, params in [
                ("thread/start", {"cwd": str(hosts[old]["home"])}),
                ("thread/resume", {"threadId": identity}),
                ("turn/start", {"threadId": identity}),
                ("command/exec", {}),
                ("future/mutation", {}),
                ("acp/config/set", {}),
            ]:
                reject_new(old, method, params)
            # Existing UUID receipt is still discoverable, even though a fresh
            # request using that method is fenced. No second turn is dispatched.
            saved = data(old, dict(send, lease=hosts[old]["lease"]), "operation")
            assert saved["outcome"]["state"] == "finished"
            active = snapshot(old)["activity"]["turns"]
            assert active == [{"thread_id": identity, "turn_id": turn}], active
            assert turn_status(old, identity) == "inProgress"
            if acp:
                pending = snapshot(old)["pending_requests"][0]
                original_id = pending["id"]
                stale_answer = raw(
                    old,
                    {
                        "command": "respond",
                        "lease": stale,
                        "request_id": original_id,
                        "result": {
                            "outcome": {"outcome": "selected", "optionId": "reject-one"}
                        },
                    },
                )
                assert stale_answer["data"]["code"] == "stale_controller"
                data(
                    old,
                    {
                        "command": "respond",
                        "lease": hosts[old]["lease"],
                        "request_id": original_id,
                        "result": {
                            "outcome": {"outcome": "selected", "optionId": "reject-one"}
                        },
                    },
                    "answered",
                )
                wait(
                    lambda: turn_status(old, identity) == "completed",
                    "settlement after explicit permission",
                )
            else:
                execute(old, "turn/interrupt", {"threadId": identity, "turnId": turn})
                wait(
                    lambda: turn_status(old, identity) == "interrupted",
                    "Stop during drain",
                )
            wait(lambda: not snapshot(old)["activity"]["turns"], "observed completion")
            assert not snapshot(old)["pending_requests"]
            unchanged = snapshot(old)
            assert (
                unchanged["host_id"],
                unchanged["host_pid"],
                unchanged["provider_pid"],
            ) == (before["host_id"], before["host_pid"], before["provider_pid"])
            assert (
                unchanged["admission"]["mode"] == "draining"
                and hosts[old]["process"].poll() is None
            )

            # The configured second endpoint accepts independent new work.
            _, fresh = execute(
                new,
                "thread/start",
                {"cwd": str(hosts[new]["home"]), "historyMode": "paginated"},
            )
            new_thread = fresh["thread"]["id"]
            execute(
                new,
                "turn/start",
                {
                    "threadId": new_thread,
                    "input": [{"type": "text", "text": "fresh Host work"}],
                },
            )
            wait(lambda: turn_status(new, new_thread) == "completed", "new Host result")
            config = root / "hosts.json"
            config.write_text(
                json.dumps(
                    {
                        "version": 1,
                        "hosts": [
                            {"name": "Old", "socket": str(hosts[old]["socket"])},
                            {"name": "New", "socket": str(hosts[new]["socket"])},
                        ],
                    }
                )
            )
            probe = subprocess.run(
                [str(BINARY), "hosts", str(config)],
                capture_output=True,
                text=True,
                check=True,
                timeout=10,
            )
            reports = json.loads(probe.stdout)
            assert reports[0]["snapshot"]["admission"]["mode"] == "draining"
            assert reports[1]["snapshot"]["admission"]["mode"] == "ready"

            # Drain survives abrupt restart; startup never reopens admission.
            stop(old, crash=True)
            start(old)
            assert snapshot(old)["host_id"] != before["host_id"]
            assert snapshot(old)["admission"] == drained["data"]
            reject_new(old, "thread/start", {"cwd": str(hosts[old]["home"])})
            assert admission(old, "ready")["data"] == {"generation": 2, "mode": "ready"}
            execute(
                old,
                "thread/start",
                {"cwd": str(hosts[old]["home"]), "historyMode": "paginated"},
            )
            # Shared human CLI explicitly claims, drains and reopens admission.
            for mode in ["drain", "ready"]:
                result = subprocess.run(
                    [
                        str(BINARY),
                        "session",
                        str(hosts[old]["socket"]),
                        "--take-control",
                        mode,
                    ],
                    capture_output=True,
                    text=True,
                    check=True,
                    timeout=10,
                )
                assert json.loads(result.stdout)["mode"] == (
                    "draining" if mode == "drain" else "ready"
                )
            print(
                json.dumps(
                    {
                        "provider": "synthetic ACP" if acp else "synthetic Codex",
                        "old_work_same_host_provider_turn": True,
                        "permission_or_stop_during_drain": True,
                        "fresh_work_rejected_without_admission": True,
                        "old_uuid_not_replayed": True,
                        "new_host_work_completed": True,
                        "controller_and_admission_cas": True,
                        "drain_survives_crash_restart": True,
                        "automatic_retirement": False,
                    }
                )
            )
        finally:
            for index in range(len(hosts)):
                stop(index)


if __name__ == "__main__":
    exercise(False)
    exercise(True)
