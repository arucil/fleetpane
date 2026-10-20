#!/usr/bin/env python3
"""Durable admission, bounded discovery, crash reconciliation and fencing.

--baseline verifies the old 128-operation lifetime rejection, using a saved binary.
No shared runtime, credential, model, or existing workspace is used.
"""

import argparse
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


def eventually(probe, label):
    end = time.monotonic() + 15
    while time.monotonic() < end:
        if value := probe():
            return value
        time.sleep(0.01)
    raise AssertionError(f"deadline exceeded: {label}")


def exercise(binary, baseline):
    with tempfile.TemporaryDirectory(prefix="fleetpane-journal-") as directory:
        root = Path(directory)
        endpoint = root / "host" / "control.sock"
        command = [
            str(binary),
            "serve",
            str(endpoint),
            str(root),
            sys.executable,
            str(Path(__file__).with_name("journal-fixture.py").resolve()),
        ]
        process = None
        logs = []

        def start():
            nonlocal process
            log = (root / f"host-{len(logs)}.log").open("w+")
            logs.append(log)
            process = subprocess.Popen(
                command, stderr=log, stdout=subprocess.DEVNULL, start_new_session=True
            )

            def ready():
                assert process.poll() is None, log.name
                return (
                    endpoint.exists()
                    and "fleetpane-host ready:" in Path(log.name).read_text()
                )

            eventually(ready, "Host initialization")

        def stop(crash=False):
            if process and process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL if crash else signal.SIGINT)
                process.wait(timeout=10)
            if not crash:
                assert not endpoint.exists(), "clean shutdown retained socket"

        def request(value):
            with socket.socket(socket.AF_UNIX) as client:
                client.settimeout(5)
                client.connect(str(endpoint))
                client.sendall((json.dumps(value) + "\n").encode())
                return json.loads(client.makefile("rb").readline(1024 * 1024))

        def data(value, expected):
            response = request(value)
            assert response["response"] == expected, response
            return response["data"]

        def snapshot():
            return data({"command": "snapshot"}, "snapshot")

        def claim(state):
            return data(
                {
                    "command": "claim",
                    "protocol": state["protocol"],
                    "host_id": state["host_id"],
                    "expected_generation": 0,
                    "controller_id": str(uuid.uuid4()),
                },
                "lease",
            )

        def effects():
            path = root / "effects.jsonl"
            return (
                [json.loads(row) for row in path.read_text().splitlines()]
                if path.exists()
                else []
            )

        try:
            start()
            original = snapshot()
            lease = claim(original)
            first = None
            count = 128 if baseline else 300
            for index in range(count):
                operation_id = str(uuid.uuid4())
                value = {
                    "command": "execute",
                    "lease": lease,
                    "operation_id": operation_id,
                    "method": "fixture/mutate",
                    "params": {"nonce": index},
                }
                first = first or value
                data(value, "operation")

                def settled(operation_id=operation_id):
                    receipt = data(
                        {"command": "operation", "operation_id": operation_id},
                        "operation",
                    )
                    return (
                        receipt if receipt["outcome"]["state"] == "finished" else None
                    )

                eventually(settled, f"result {index}")
            assert len(effects()) == count
            if baseline:
                rejected = request(dict(first, operation_id=str(uuid.uuid4())))
                assert rejected["data"]["code"] == "capacity", rejected
                print(
                    json.dumps(
                        {
                            "baseline": "128 settled mutations exhaust lifetime journal",
                            "side_effects": len(effects()),
                        }
                    )
                )
                return
            assert snapshot()["journal_total"] == 300
            assert len(snapshot()["operation_ids"]) <= 128
            assert first["operation_id"] not in snapshot()["operation_ids"]
            assert data(first, "operation")["outcome"]["state"] == "finished"
            assert (
                request(dict(first, params={"nonce": "different"}))["data"]["code"]
                == "operation_conflict"
            )
            assert len(effects()) == 300

            # Appending at the head does not shift the next older page.
            first_page = data({"command": "operations", "cursor": None}, "operations")
            held = dict(
                first,
                operation_id=str(uuid.uuid4()),
                params={"nonce": "crash", "hold": True},
            )
            assert data(held, "operation")["outcome"]["state"] == "accepted"
            eventually(lambda: len(effects()) == 301, "crash side effect")
            found = first_page["entries"]
            cursor = first_page["next_cursor"]
            while cursor:
                page = data({"command": "operations", "cursor": cursor}, "operations")
                assert len(page["entries"]) <= 32
                found.extend(page["entries"])
                cursor = page["next_cursor"]
            assert len(found) == len({row["id"] for row in found}) == 300
            assert found[-1]["id"] == first["operation_id"]
            foreign = json.loads(first_page["next_cursor"])
            foreign["journal"] = str(uuid.uuid4())
            assert (
                request({"command": "operations", "cursor": json.dumps(foreign)})[
                    "response"
                ]
                == "error"
            )

            # A second Host must fail before launching a provider or altering receipts.
            duplicate = subprocess.run(
                command, check=False, capture_output=True, timeout=10, text=True
            )
            assert duplicate.returncode and "journal already owned" in duplicate.stderr
            assert snapshot()["host_pid"] == process.pid
            stop(crash=True)
            assert endpoint.exists(), "crash fixture requires stale socket"

            # Configuration mismatch cannot silently attach to the old journal.
            mismatch = command.copy()
            mismatch[3] = str(root / "other-workspace")
            Path(mismatch[3]).mkdir()
            rejected = subprocess.run(
                mismatch, check=False, capture_output=True, timeout=10, text=True
            )
            assert rejected.returncode and "binding differs" in rejected.stderr
            start()  # Exclusive owner safely removes the refused stale socket.
            recovered = snapshot()
            assert recovered["host_id"] != original["host_id"]
            assert recovered["journal_total"] == 301 and recovered["fault"] is None
            assert request(first)["data"]["code"] == "stale_controller"
            lease = claim(recovered)
            saved = data(dict(first, lease=lease), "operation")
            assert saved["outcome"]["state"] == "finished"
            assert saved["completed_sequence"] is None
            uncertain = data(dict(held, lease=lease), "operation")
            assert uncertain["outcome"]["state"] == "unknown", uncertain
            assert len(effects()) == 301, "old UUID was redispatched"
            fresh = dict(
                first,
                lease=lease,
                operation_id=str(uuid.uuid4()),
                params={"nonce": "after-recovery"},
            )
            data(fresh, "operation")
            eventually(
                lambda: (
                    data(
                        {"command": "operation", "operation_id": fresh["operation_id"]},
                        "operation",
                    )["outcome"]["state"]
                    == "finished"
                ),
                "new operation after recovery",
            )
            assert len(effects()) == 302
            # Human CLI goes through the exact same read-only API.
            for verb, arguments in [
                ("operations", []),
                ("operation", [first["operation_id"]]),
            ]:
                result = subprocess.run(
                    [str(binary), verb, str(endpoint), *arguments],
                    capture_output=True,
                    timeout=10,
                    text=True,
                    check=True,
                )
                assert json.loads(result.stdout)["response"] == verb
            assert snapshot()["owner"] == lease
            receipt = {
                "settled_before_crash": 300,
                "durable_receipts": snapshot()["journal_total"],
                "exact_side_effects": 302,
                "resident_ids": len(snapshot()["operation_ids"]),
                "historical_result": saved["outcome"]["state"],
                "crashed_result": uncertain["outcome"]["state"],
                "duplicate_replay": False,
                "historical_event_watermark": saved["completed_sequence"],
            }
            print(json.dumps(receipt))
        finally:
            stop()
            for log in logs:
                log.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--host", type=Path, default=Path("target/debug/fleetpane-host")
    )
    parser.add_argument("--baseline", action="store_true")
    args = parser.parse_args()
    exercise(args.host.resolve(), args.baseline)
