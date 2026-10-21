#!/usr/bin/env python3
"""Cancel pending and in-flight ACP permissions without prematurely ending turns."""

import json
import os
import signal
import socket
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main():
    binary = ROOT / "target/debug/fleetpane-host"
    with tempfile.TemporaryDirectory(prefix="fp-acp-cancel-") as directory:
        root = Path(directory)
        control = root / "private/control.sock"
        process = None
        generation = 0

        def wait(probe):
            deadline = time.monotonic() + 12
            while True:
                value = probe()
                if value:
                    return value
                assert time.monotonic() < deadline, (
                    "private cancellation fixture timed out"
                )
                time.sleep(0.025)

        def snapshot():
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(5)
                stream.connect(str(control))
                stream.sendall(b'{"command":"snapshot"}\n')
                return json.loads(stream.makefile().readline())["data"]

        def command(*args):
            result = subprocess.run(
                [str(binary), "session", str(control), "--take-control", *args],
                capture_output=True,
                text=True,
                timeout=12,
                check=False,
            )
            assert result.returncode == 0, result.stderr
            return json.loads(result.stdout)["result"]

        def start():
            nonlocal process, generation
            generation += 1
            log = root / f"host-{generation}.log"
            with log.open("w") as output:
                process = subprocess.Popen(
                    [
                        str(binary),
                        "serve-acp",
                        str(control),
                        str(root),
                        "python3",
                        str(ROOT / "scripts/acp-fixture.py"),
                    ],
                    stdout=subprocess.DEVNULL,
                    stderr=output,
                    start_new_session=True,
                )
            wait(
                lambda: (
                    process.poll() is None
                    and control.exists()
                    and "ready:" in log.read_text()
                )
            )

        def stop():
            if process is not None and process.poll() is None:
                os.killpg(process.pid, signal.SIGINT)
                process.wait(timeout=10)

        def raw(value):
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(5)
                stream.connect(str(control))
                stream.sendall(json.dumps(value).encode() + b"\n")
                return json.loads(stream.makefile().readline())

        def opened(identity):
            assert not snapshot()["fault"], snapshot()["fault"]
            return command("open", identity)["thread"]

        try:
            start()
            identity = command("create", str(root))["thread"]["id"]
            other = command("create", str(root))["thread"]["id"]
            command("send", other, "[permission]")
            other_request = wait(lambda: snapshot()["pending_requests"])[0]
            command("send", identity, "[permission-cancel-gate]")
            pending = wait(
                lambda: p if len(p := snapshot()["pending_requests"]) == 3 else None
            )
            cancelled_ids = [
                p["id"] for p in pending if p["params"]["threadId"] == identity
            ]
            turn = opened(identity)["turns"][-1]["id"]
            command("stop", identity, turn)
            # The old adapter leaves both requests pending and the provider stuck.
            wait(lambda: (root / f"cancel-{identity}-answered.json").exists())
            state = snapshot()
            assert state["pending_requests"] == [other_request]
            active = opened(identity)
            assert active["status"] == {"type": "active", "cancelRequested": True}
            assert active["turns"][-1]["status"] == "inProgress"
            assert "stopReason" not in active["turns"][-1]
            duplicate = subprocess.run(
                [
                    str(binary),
                    "session",
                    str(control),
                    "--take-control",
                    "stop",
                    identity,
                    turn,
                ],
                capture_output=True,
                text=True,
                timeout=12,
                check=False,
            )
            assert duplicate.returncode != 0 and "stop_pending" in duplicate.stderr, (
                duplicate.stderr
            )
            late = raw(
                {
                    "command": "respond",
                    "lease": snapshot()["owner"],
                    "request_id": cancelled_ids[0],
                    "result": {
                        "outcome": {"outcome": "selected", "optionId": "allow-one"}
                    },
                }
            )
            assert (
                late["response"] == "error"
                and late["data"]["code"] == "unknown_request"
            ), late
            assert snapshot()["pending_requests"] == [other_request]
            wire = [
                json.loads(line)
                for line in (root / "cancel-wire.jsonl").read_text().splitlines()
            ]
            assert len(wire) == 4 and wire[0]["method"] == "session/cancel", wire
            ids = json.loads((root / f"cancel-{identity}-answered.json").read_text())
            assert sorted(p["id"] for p in wire[1:]) == sorted(ids)
            assert all(
                p["result"] == {"outcome": {"outcome": "cancelled"}} for p in wire[1:]
            )
            (root / f"cancel-{identity}-settle").touch()
            final = wait(
                lambda: (
                    t
                    if (t := opened(identity))["turns"][-1]["status"] == "interrupted"
                    else None
                )
            )
            assert final["turns"][-1]["stopReason"] == "cancelled"
            assert (
                final["status"]["type"] == "idle"
                and not final["status"]["cancelRequested"]
            )
            assert snapshot()["pending_requests"] == [other_request]
            # Permission on the other session remains usable by the current controller.
            answer = raw(
                {
                    "command": "respond",
                    "lease": snapshot()["owner"],
                    "request_id": other_request["id"],
                    "result": {
                        "outcome": {"outcome": "selected", "optionId": "reject-one"}
                    },
                }
            )
            assert answer["response"] == "answered", answer
            wait(lambda: opened(other)["turns"][-1]["status"] == "completed")
            stop()
            start()
            archived = opened(identity)
            assert archived["status"]["type"] == "archived"
            assert not archived["status"]["cancelRequested"]
            assert archived["turns"] == final["turns"]
            assert not snapshot()["pending_requests"]
            assert (
                root / "prompt-count.txt"
            ).read_text() == "[permission]\n[permission-cancel-gate]\n"
            # A Host restart during requested cancellation has no proven outcome.
            interrupted = command("create", str(root))["thread"]["id"]
            command("send", interrupted, "[permission-cancel-gate]")
            wait(lambda: len(snapshot()["pending_requests"]) == 2)
            interrupted_turn = opened(interrupted)["turns"][-1]["id"]
            command("stop", interrupted, interrupted_turn)
            wait(lambda: (root / f"cancel-{interrupted}-answered.json").exists())
            assert opened(interrupted)["status"]["cancelRequested"]
            stop()
            start()
            unknown = opened(interrupted)
            assert unknown["status"] == {"type": "archived", "cancelRequested": False}
            assert unknown["turns"][-1]["status"] == "unknown"
            assert unknown["turns"][-1].get("stopReason") is None
            assert not snapshot()["pending_requests"]
            assert len((root / "prompt-count.txt").read_text().splitlines()) == 3
            print(
                json.dumps(
                    {
                        "provider": "private ACP fixture",
                        "cancel_pending_and_late_permissions": "PASS",
                        "wait_for_prompt_settlement": "PASS",
                        "duplicate_stop_no_duplicate_wire": "PASS",
                        "stale_answer_rejected": "PASS",
                        "other_session_permission_unchanged": "PASS",
                        "archive_no_replay_or_permission_restore": "PASS",
                        "restart_mid_cancellation_is_unknown": "PASS",
                    }
                )
            )
        finally:
            stop()


if __name__ == "__main__":
    main()
