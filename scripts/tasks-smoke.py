#!/usr/bin/env python3
"""Real private Hosts + synthetic providers: retained agent task lists."""

import json
import os
import signal
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"


def wait(probe):
    end = time.monotonic() + 15
    while time.monotonic() < end:
        if value := probe():
            return value
        time.sleep(0.025)
    raise AssertionError("task fixture deadline")


def raw(endpoint, value):
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(5)
        stream.connect(str(endpoint))
        stream.sendall((json.dumps(value) + "\n").encode())
        return json.loads(stream.makefile("rb").readline(1024 * 1024))


def main():
    processes = []
    with tempfile.TemporaryDirectory(prefix="fp-tasks-", dir="/tmp") as temporary:
        root = Path(temporary)

        def start(index, acp):
            home = root / str(index)
            home.mkdir(mode=0o700, exist_ok=True)
            endpoint = home / "host/control.sock"
            log = home / "host.log"
            with log.open("w") as output:
                process = subprocess.Popen(
                    [
                        str(BINARY),
                        "serve-acp" if acp else "serve",
                        str(endpoint),
                        str(home),
                        sys.executable,
                        str(
                            ROOT
                            / "scripts"
                            / ("acp-fixture.py" if acp else "provider-fixture.py")
                        ),
                    ],
                    stdout=subprocess.DEVNULL,
                    stderr=output,
                    start_new_session=True,
                    env={
                        **os.environ,
                        "HOME": str(home),
                        "CODEX_HOME": str(home / "codex"),
                        "DSH_HOME": str(home / "dsh"),
                    },
                )
            processes.append(process)
            wait(
                lambda: (
                    process.poll() is not None
                    or (endpoint.exists() and "ready:" in log.read_text())
                )
            )
            assert process.poll() is None, log.read_text()
            return process, endpoint

        def snap(endpoint):
            response = raw(endpoint, {"command": "snapshot"})
            assert response["response"] == "snapshot", response
            return response["data"]

        def cli(endpoint, *args, take=False, ok=True):
            command = [str(BINARY), "session", str(endpoint)]
            if take:
                command.append("--take-control")
            result = subprocess.run(
                [*command, *args],
                capture_output=True,
                text=True,
                timeout=25,
                check=False,
            )
            assert (result.returncode == 0) == ok, (
                args,
                result.stdout[-1000:],
                result.stderr[-1000:],
            )
            return json.loads(result.stdout) if ok else result.stderr

        def settled(endpoint):
            return not snap(endpoint)["activity"]["turns"]

        try:
            for index, acp in enumerate([False, True]):
                process, endpoint = start(index, acp)
                # No controller exists yet: inspection refuses instead of claiming.
                before = snap(endpoint)
                cli(endpoint, "tasks", "none", ok=False)
                assert snap(endpoint) == before
                thread = cli(endpoint, "create", str(root), take=True)["result"][
                    "thread"
                ]["id"]
                initial = snap(endpoint)
                cli(endpoint, "send", thread, "[tasks]", take=True)
                wait(lambda endpoint=endpoint: settled(endpoint))
                view = cli(endpoint, "tasks", thread)
                assert (
                    len(view["plan"]["entries"]) == 1
                    and view["plan"]["entries"][0]["content"] == "重规划：执行验证"
                )
                assert (
                    view["plan"]["entries"][0]["status"] == "in_progress"
                )  # End of turn is not task completion.
                assert (
                    raw(
                        endpoint,
                        {
                            "command": "events",
                            "host_id": initial["host_id"],
                            "after": 0,
                        },
                    )["response"]
                    == "error"
                )
                before = snap(endpoint)
                for _ in range(5):
                    assert cli(endpoint, "tasks", thread) == view
                assert (
                    snap(endpoint) == before
                )  # No claim, UUID journal, provider query or event.
                second = cli(endpoint, "create", str(root), take=True)["result"][
                    "thread"
                ]["id"]
                assert cli(endpoint, "tasks", second)["plan"] is None
                assert cli(endpoint, "tasks", thread)["plan"] == view["plan"]
                if acp:
                    cli(endpoint, "send", thread, "[todos]", take=True)
                    wait(lambda endpoint=endpoint: settled(endpoint))
                    entries = cli(endpoint, "tasks", thread)["plan"]["entries"]
                    assert [e["status"] for e in entries] == [
                        "completed",
                        "awaiting_review",
                    ]
                    assert all(e["priority"] is None for e in entries)
                    cli(endpoint, "send", thread, "[todos-clear]", take=True)
                    wait(lambda endpoint=endpoint: settled(endpoint))
                    assert cli(endpoint, "tasks", thread)["plan"]["entries"] == []
                cli(endpoint, "send", thread, "[tasks-future]", take=True)
                wait(lambda endpoint=endpoint: settled(endpoint))
                assert (
                    cli(endpoint, "tasks", thread)["plan"]["entries"][1]["status"]
                    == "awaiting_review"
                )
                cli(endpoint, "send", thread, "[tasks-clear]", take=True)
                wait(lambda endpoint=endpoint: settled(endpoint))
                assert cli(endpoint, "tasks", thread)["plan"]["entries"] == []
                turn = cli(endpoint, "send", thread, "[tasks-wait]", take=True)[
                    "result"
                ]["turn"]["id"]
                wait(
                    lambda endpoint=endpoint, thread=thread: (
                        len(cli(endpoint, "tasks", thread)["plan"]["entries"]) == 3
                    )
                )
                lease = snap(endpoint)["owner"]
                cli(endpoint, "stop", thread, turn, take=True)
                wait(lambda endpoint=endpoint: settled(endpoint))
                assert (
                    cli(endpoint, "tasks", thread)["plan"]["entries"][1]["status"]
                    == "in_progress"
                )
                before = snap(endpoint)
                stale = raw(
                    endpoint,
                    {
                        "command": "query",
                        "lease": lease,
                        "method": "fleetpane/tasks/read",
                        "params": {"threadId": thread},
                    },
                )
                assert (
                    stale["response"] == "error"
                    and stale["data"]["code"] == "stale_controller"
                ), stale
                invalid = raw(
                    endpoint,
                    {
                        "command": "query",
                        "lease": before["owner"],
                        "method": "fleetpane/tasks/read",
                        "params": {"threadId": thread, "extra": True},
                    },
                )
                assert invalid["response"] == "error"
                assert snap(endpoint) == before
                assert (
                    snap(endpoint)["host_id"] == initial["host_id"]
                    and snap(endpoint)["provider_pid"] == initial["provider_pid"]
                )
                # Draining does not disable the local read or manufacture completion.
                cli(endpoint, "drain", take=True)
                before = snap(endpoint)
                assert cli(endpoint, "tasks", thread)["plan"]["turnId"] == turn
                assert snap(endpoint) == before
                os.kill(before["provider_pid"], signal.SIGTERM)
                wait(lambda endpoint=endpoint: snap(endpoint)["fault"] is not None)
                before = snap(endpoint)
                assert cli(endpoint, "tasks", thread)["plan"]["turnId"] == turn
                assert snap(endpoint) == before
                # A fresh Host cannot claim to have restored its predecessor's plan cache.
                os.killpg(process.pid, signal.SIGINT)
                process.wait(timeout=10)
                _, endpoint = start(index, acp)
                cli(endpoint, "list", take=True)
                assert cli(endpoint, "tasks", thread)["plan"] is None
            print(
                json.dumps(
                    {
                        "ok": True,
                        "providers": ["Codex", "ACP"],
                        "full_replacement_clear_unknown_status": True,
                        "event_ring_expired_cache_retained": True,
                        "read_has_no_claim_dispatch_or_journal": True,
                        "stop_does_not_complete_tasks": True,
                        "deepseek_successful_todo_write_bridge": True,
                        "drain_read": True,
                        "provider_fault_cache_read": True,
                        "host_restart_cache_unavailable": True,
                    }
                )
            )
        finally:
            for process in processes:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGINT)
            for process in processes:
                if process.poll() is None:
                    process.wait(timeout=10)


if __name__ == "__main__":
    main()
