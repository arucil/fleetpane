#!/usr/bin/env python3
"""Exercise paged title/workspace queries and rename through the shared adapter."""

import json
import os
import signal
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path


def main():
    repository = Path(__file__).resolve().parents[1]
    binary = repository / "target/debug/fleetpane-host"
    with tempfile.TemporaryDirectory(prefix="fleetpane-directory-") as directory:
        root = Path(directory)
        history = root / "history.json"
        history.write_text(
            json.dumps(
                [
                    {
                        "id": f"directory-{index:03}",
                        "cwd": "/synthetic/alpha"
                        if index % 2 == 0
                        else "/synthetic/beta",
                        "preview": f"Session {index:03}",
                        "name": f"会话 {index:03}",
                        "status": {"type": "idle"},
                        "turns": [],
                    }
                    for index in range(237)
                ]
            )
        )
        socket = root / "private/control.sock"
        with (root / "host.log").open("w+") as log:
            process = subprocess.Popen(
                [
                    str(binary),
                    "serve",
                    str(socket),
                    str(root),
                    sys.executable,
                    str(repository / "scripts/provider-fixture.py"),
                    "--history",
                    str(history),
                ],
                stdout=subprocess.DEVNULL,
                stderr=log,
                start_new_session=True,
            )
            try:
                deadline = time.monotonic() + 10
                while (
                    not socket.exists()
                    or "ready:" not in (root / "host.log").read_text()
                ):
                    assert process.poll() is None and time.monotonic() < deadline, (
                        root / "host.log"
                    ).read_text()
                    time.sleep(0.05)

                def raw(payload):
                    result = subprocess.run(
                        [str(binary), "request", str(socket)],
                        input=json.dumps(payload),
                        text=True,
                        capture_output=True,
                        timeout=8,
                        check=False,
                    )
                    assert result.stdout, result.stderr
                    return json.loads(result.stdout)

                def state():
                    return raw({"command": "snapshot"})["data"]

                def run(*args):
                    return subprocess.run(
                        [str(binary), "session", str(socket), "--take-control", *args],
                        text=True,
                        capture_output=True,
                        timeout=25,
                        check=False,
                    )

                def command(*args):
                    result = run(*args)
                    assert result.returncode == 0, result.stderr
                    return json.loads(result.stdout)["result"]

                initial = state()
                first = command("list")
                owner = state()["owner"]
                assert len(first["data"]) == 100 and first["nextCursor"]
                second = command("list", "--cursor", first["nextCursor"])
                third = command("list", "--cursor", second["nextCursor"])
                ids = [
                    thread["id"]
                    for page in [first, second, third]
                    for thread in page["data"]
                ]
                assert len(ids) == len(set(ids)) == 237 and third["nextCursor"] is None
                filtered = command(
                    "list", "--search", "会话 23", "--cwd", "/synthetic/alpha"
                )
                assert [thread["id"] for thread in filtered["data"]] == [
                    "directory-230",
                    "directory-232",
                    "directory-234",
                    "directory-236",
                ]
                workspace = command("list", "--cwd", "/synthetic/beta")
                workspace_next = command(
                    "list",
                    "--cwd",
                    "/synthetic/beta",
                    "--cursor",
                    workspace["nextCursor"],
                )
                assert (
                    len(workspace["data"]) == 100 and len(workspace_next["data"]) == 18
                )
                assert all(
                    thread["cwd"] == "/synthetic/beta"
                    for thread in workspace_next["data"]
                )
                assert command("list", "--search", "no-such-title")["data"] == []
                before_rename = state()["sequence"]
                command("rename", "directory-236", "  Rename 中文  ")
                assert (
                    command("open", "directory-236")["thread"]["name"] == "Rename 中文"
                )
                assert len(command("list", "--search", "会话 23")["data"]) == 6
                assert (
                    command("list", "--search", "rename", "--cwd", "/synthetic/alpha")[
                        "data"
                    ][0]["id"]
                    == "directory-236"
                )
                events = raw(
                    {
                        "command": "events",
                        "host_id": initial["host_id"],
                        "after": before_rename,
                    }
                )["data"]
                assert any(
                    event["message"].get("method") == "thread/name/updated"
                    and event["message"]["params"]["threadName"] == "Rename 中文"
                    for event in events
                )
                for args, error in [
                    (("rename", "directory-236", " "), "must contain text"),
                    (
                        ("rename", "directory-236", "[rename-fail]"),
                        "Synthetic rename failure",
                    ),
                    (
                        ("list", "--search", "[list-fail]"),
                        "Synthetic directory failure",
                    ),
                    (("list", "--cwd", "relative"), "absolute directory"),
                ]:
                    failed = run(*args)
                    assert failed.returncode != 0 and error in failed.stderr, (
                        args,
                        failed.stdout,
                        failed.stderr,
                    )
                assert (
                    command("open", "directory-236")["thread"]["name"] == "Rename 中文"
                )
                stale = raw(
                    {
                        "command": "execute",
                        "lease": owner,
                        "operation_id": str(uuid.uuid4()),
                        "method": "thread/name/set",
                        "params": {"threadId": "directory-236", "name": "wrong"},
                    }
                )
                assert stale["data"]["code"] == "stale_controller"
                final = state()
                assert (
                    final["host_id"] == initial["host_id"]
                    and final["provider_pid"] == initial["provider_pid"]
                )
                print(
                    json.dumps(
                        {
                            "provider": "synthetic",
                            "paged_sessions": 237,
                            "global_title_workspace_filters": "PASS",
                            "rename_readback_notification_errors_fencing": "PASS",
                            "same_host_and_provider": True,
                        }
                    )
                )
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGINT)
                    process.wait(timeout=10)


if __name__ == "__main__":
    main()
