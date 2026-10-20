#!/usr/bin/env python3
"""Twenty synthetic ACP streams with reads, cancellation and controller replacement.

Measures Host responses, not desktop input latency or real model capacity.
The fixture has one Python thread per stream and is part of the recorded workload.
"""

import argparse
import hashlib
import json
import math
import os
import platform
import signal
import socket
import subprocess
import tempfile
import time
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SESSIONS = 20


def percentile(values, fraction):
    return round(sorted(values)[max(0, math.ceil(len(values) * fraction) - 1)], 3)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--binary", type=Path, default=ROOT / "target/debug/fleetpane-host"
    )
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="fp-concurrency-") as directory:
        root = Path(directory)
        control = root / "private/control.sock"
        log_path = root / "host.log"
        with log_path.open("w") as log:
            process = subprocess.Popen(
                [
                    str(args.binary),
                    "serve-acp",
                    str(control),
                    str(root),
                    "python3",
                    str(ROOT / "scripts/acp-fixture.py"),
                ],
                stdout=subprocess.DEVNULL,
                stderr=log,
                start_new_session=True,
            )
        samples = []

        def raw(value):
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(8)
                stream.connect(str(control))
                stream.sendall(json.dumps(value).encode() + b"\n")
                return json.loads(stream.makefile().readline())

        def snapshot():
            value = raw({"command": "snapshot"})
            assert value["response"] == "snapshot", value
            assert value["data"]["fault"] is None, value["data"]["fault"]
            return value["data"]

        def operation(method, params):
            value = raw(
                {
                    "command": "execute",
                    "lease": lease,
                    "operation_id": str(uuid.uuid4()),
                    "method": method,
                    "params": params,
                }
            )
            assert value["response"] == "operation", value
            deadline = time.monotonic() + 10
            while value["data"]["outcome"]["state"] == "accepted":
                assert time.monotonic() < deadline
                time.sleep(0.005)
                value = raw(
                    {"command": "operation", "operation_id": value["data"]["id"]}
                )
            outcome = value["data"]["outcome"]
            assert outcome["state"] == "finished", outcome
            assert "error" not in outcome["response"], outcome
            return outcome["response"]["result"]

        def query(method, params):
            value = raw(
                {"command": "query", "lease": lease, "method": method, "params": params}
            )
            assert value["response"] == "query", value
            message = value["data"]["message"]
            assert "error" not in message, message
            return message["result"]

        def claim(state):
            value = raw(
                {
                    "command": "claim",
                    "protocol": state["protocol"],
                    "host_id": state["host_id"],
                    "expected_generation": state["owner"]["generation"]
                    if state["owner"]
                    else 0,
                    "controller_id": str(uuid.uuid4()),
                }
            )
            assert value["response"] == "lease", value
            return value["data"]

        try:
            deadline = time.monotonic() + 15
            while "ready:" not in log_path.read_text():
                assert process.poll() is None and time.monotonic() < deadline
                time.sleep(0.025)
            initial = snapshot()
            lease = claim(initial)
            identities = [
                operation("thread/start", {"cwd": str(root)})["thread"]["id"]
                for _ in range(SESSIONS)
            ]
            turns = {
                identity: operation(
                    "turn/start",
                    {
                        "threadId": identity,
                        "input": [{"type": "text", "text": "[burst]"}],
                    },
                )["turn"]["id"]
                for identity in identities
            }
            active = [
                query("thread/turns/list", {"threadId": identity})["data"][0]["status"]
                for identity in identities
            ]
            assert active == ["inProgress"] * SESSIONS, active
            boundary = snapshot()["sequence"]
            old_lease = lease
            lease = claim(snapshot())
            stale = raw(
                {
                    "command": "query",
                    "lease": old_lease,
                    "method": "thread/read",
                    "params": {"threadId": identities[0]},
                }
            )
            assert stale["response"] == "error", stale
            started = time.monotonic()
            finished = set()
            cancelled = False
            deadline = started + 20
            peak_rss = {"host": 0, "fixture_provider": 0, "probe": 0, "combined": 0}
            while len(finished) < SESSIONS:
                before = time.perf_counter()
                state = snapshot()
                query(
                    "thread/read",
                    {
                        "threadId": identities[len(samples) % SESSIONS],
                        "includeTurns": False,
                    },
                )
                samples.append((time.perf_counter() - before) * 1000)
                assert (state["host_pid"], state["provider_pid"]) == (
                    initial["host_pid"],
                    initial["provider_pid"],
                )
                events = raw(
                    {
                        "command": "events",
                        "host_id": state["host_id"],
                        "after": boundary,
                    }
                )
                assert events["response"] == "events", events
                for event in events["data"]:
                    message = event["message"]
                    if message.get("method") == "turn/completed":
                        finished.add(message["params"]["threadId"])
                if events["data"]:
                    boundary = events["data"][-1]["sequence"]
                if not cancelled and time.monotonic() - started >= 1:
                    operation(
                        "turn/interrupt",
                        {"threadId": identities[0], "turnId": turns[identities[0]]},
                    )
                    cancelled = True
                combined = 0
                for name, pid in [
                    ("host", initial["host_pid"]),
                    ("fixture_provider", initial["provider_pid"]),
                    ("probe", os.getpid()),
                ]:
                    status = Path(f"/proc/{pid}/status")
                    if status.exists():
                        line = next(
                            line
                            for line in status.read_text().splitlines()
                            if line.startswith("VmRSS:")
                        )
                        resident_kib = int(line.split()[1])
                        peak_rss[name] = max(peak_rss[name], resident_kib)
                        combined += resident_kib
                peak_rss["combined"] = max(peak_rss["combined"], combined)
                assert time.monotonic() < deadline, "concurrent prompt settlement"
                time.sleep(0.01)
            counts = []
            for index, identity in enumerate(identities):
                turn = query("thread/turns/list", {"threadId": identity})["data"][0]
                assert turn["status"] == (
                    "interrupted" if index == 0 else "completed"
                ), turn
                entries = query(
                    "thread/items/list", {"threadId": identity, "turnId": turn["id"]}
                )["data"]
                message = next(
                    entry["item"]
                    for entry in entries
                    if entry["item"]["type"] == "acpMessage"
                )
                text = message["content"][0]["text"]
                count = len(text.splitlines())
                assert text == "".join(f"chunk-{i:02} 你好\n" for i in range(count))
                assert (0 < count < 50) if index == 0 else count == 50, count
                counts.append(count)
            assert (root / "prompt-count.txt").read_text().splitlines() == [
                "[burst]"
            ] * SESSIONS
            final = snapshot()
            assert len(final["operation_ids"]) == 2 * SESSIONS + 1
            summary = {
                "system": {
                    "platform": platform.platform(),
                    "python": platform.python_version(),
                },
                "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                "fixture_sha256": hashlib.sha256(
                    (ROOT / "scripts/acp-fixture.py").read_bytes()
                ).hexdigest(),
                "provider": "synthetic ACP, one Python thread per session",
                "sessions": SESSIONS,
                "simultaneously_active": SESSIONS,
                "configured_chunks_per_second": 10 * SESSIONS,
                "completed": SESSIONS - 1,
                "interrupted": 1,
                "exact_text_and_single_execution": True,
                "same_host_provider_across_controller_replacement": True,
                "samples": len(samples),
                "snapshot_plus_metadata_ms": {
                    f"p{p}": percentile(samples, p / 100) for p in [50, 95, 99]
                },
                "sampled_rss_peak_kib": peak_rss if Path("/proc").exists() else None,
                "ui_latency_measured": False,
                "real_model_capacity_measured": False,
            }
            if args.output:
                args.output.write_text(
                    json.dumps(
                        {
                            "summary": summary,
                            "samples_ms": samples,
                            "chunks_per_session": counts,
                        },
                        indent=2,
                    )
                )
            print(json.dumps(summary))
        finally:
            os.killpg(process.pid, signal.SIGINT)
            process.wait(timeout=15)
            assert not control.exists(), "owned socket must be removed"


if __name__ == "__main__":
    main()
