#!/usr/bin/env python3
"""Repeated shared reads must not consume mutation identities or repeat a send."""

import json
import os
import signal
import socket as sockets
import subprocess
import tempfile
import time
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main():
    binary = ROOT / "target/debug/fleetpane-host"
    with tempfile.TemporaryDirectory(prefix="fleetpane-query-") as directory:
        root = Path(directory)
        history = root / "history.json"
        history.write_text(
            json.dumps(
                [
                    {
                        "id": "saved",
                        "cwd": "/synthetic",
                        "name": "Synthetic history",
                        "preview": "Synthetic history",
                        "historyMode": "paginated",
                        "status": {"type": "idle"},
                        "turns": [
                            {
                                "id": "saved-turn",
                                "status": "completed",
                                "error": None,
                                "items": [
                                    {
                                        "id": "saved-item",
                                        "type": "userMessage",
                                        "content": [{"type": "text", "text": "saved"}],
                                    }
                                ],
                            }
                        ],
                    }
                ]
            )
        )
        socket = root / "private/control.sock"
        with (root / "host.log").open("w") as log:
            process = subprocess.Popen(
                [
                    str(binary),
                    "serve",
                    str(socket),
                    str(root),
                    "python3",
                    str(ROOT / "scripts/provider-fixture.py"),
                    "--history",
                    str(history),
                ],
                stdout=subprocess.DEVNULL,
                stderr=log,
                start_new_session=True,
            )
            try:
                deadline = time.monotonic() + 15
                while "ready:" not in (root / "host.log").read_text():
                    assert process.poll() is None
                    assert time.monotonic() < deadline, "Host startup deadline"
                    time.sleep(0.025)

                def raw(value):
                    with sockets.socket(sockets.AF_UNIX) as stream:
                        stream.settimeout(8)
                        stream.connect(str(socket))
                        stream.sendall(json.dumps(value).encode() + b"\n")
                        with stream.makefile("r") as reader:
                            return json.loads(reader.readline())

                def command(*arguments):
                    result = subprocess.run(
                        [
                            str(binary),
                            "session",
                            str(socket),
                            "--take-control",
                            *arguments,
                        ],
                        capture_output=True,
                        text=True,
                        timeout=10,
                        check=False,
                    )
                    assert result.returncode == 0, result.stderr
                    return json.loads(result.stdout)

                created = command("create", str(root))["result"]["thread"]["id"]
                before = raw({"command": "snapshot"})["data"]
                reads = [
                    ("list",),
                    ("models",),
                    ("inspect", "saved"),
                    ("history-turns", "saved"),
                    ("history-items", "saved", "saved-turn"),
                ]
                for index in range(160):
                    result = command(*reads[index % len(reads)])
                    assert result["sequence"] > before["sequence"]
                after = raw({"command": "snapshot"})["data"]
                assert after["operation_ids"] == before["operation_ids"], (
                    "reads consumed mutation admission slots"
                )
                for method in (
                    "thread/resume",
                    "turn/start",
                    "command/exec",
                    "future/read",
                ):
                    rejected = raw(
                        {
                            "command": "query",
                            "lease": after["owner"],
                            "method": method,
                            "params": {},
                        }
                    )
                    assert rejected["data"]["code"] == "not_query", rejected
                stale = raw(
                    {
                        "command": "query",
                        "lease": before["owner"],
                        "method": "thread/read",
                        "params": {"threadId": "saved"},
                    }
                )
                assert stale["data"]["code"] == "stale_controller", stale
                failed = raw(
                    {
                        "command": "query",
                        "lease": after["owner"],
                        "method": "thread/list",
                        "params": {"searchTerm": "[list-fail]", "limit": 1},
                    }
                )
                assert (
                    failed["response"] == "query"
                    and "Synthetic directory failure"
                    in failed["data"]["message"]["error"]["message"]
                ), failed
                sequence = failed["data"]["sequence"]
                with sockets.socket(sockets.AF_UNIX) as stream:
                    stream.connect(str(socket))
                    stream.sendall(
                        json.dumps(
                            {
                                "command": "query",
                                "lease": after["owner"],
                                "method": "thread/list",
                                "params": {"limit": 1},
                            }
                        ).encode()
                        + b"\n"
                    )
                    # The provider reply must be ordered even if this caller exits.
                deadline = time.monotonic() + 5
                while raw({"command": "snapshot"})["data"]["sequence"] == sequence:
                    assert time.monotonic() < deadline, "abandoned query response"
                    time.sleep(0.01)
                execute = {
                    "command": "execute",
                    "lease": after["owner"],
                    "operation_id": str(uuid.uuid4()),
                    "method": "turn/start",
                    "params": {
                        "threadId": created,
                        "input": [{"type": "text", "text": "one send after 160 reads"}],
                    },
                }
                for _ in range(2):
                    accepted = raw(execute)
                    assert accepted["response"] == "operation", accepted
                deadline = time.monotonic() + 10
                while True:
                    opened = command("open", created)["result"]["thread"]
                    assert len(opened["turns"]) == 1, "duplicate turn submission"
                    if opened["turns"][0]["status"] == "completed":
                        break
                    assert time.monotonic() < deadline, "synthetic turn completion"
                    time.sleep(0.025)
                final = raw({"command": "snapshot"})["data"]
                assert len(final["operation_ids"]) == 2
                assert final["fault"] is None
                assert (final["host_pid"], final["provider_pid"]) == (
                    before["host_pid"],
                    before["provider_pid"],
                )
                print(
                    json.dumps(
                        {
                            "reads": 160,
                            "mutation_records": len(final["operation_ids"]),
                            "turns_after_same_id_resubmission": len(opened["turns"]),
                            "same_host_and_provider": True,
                            "stale_read_and_mutating_query_rejected": True,
                            "provider_read_error_visible": True,
                            "abandoned_read_does_not_fault_host": True,
                        }
                    )
                )
            finally:
                os.killpg(process.pid, signal.SIGINT)
                process.wait(timeout=10)
        assert not socket.exists(), "owned Host did not clean up its socket"


if __name__ == "__main__":
    main()
