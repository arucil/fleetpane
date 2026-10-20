#!/usr/bin/env python3
"""Bounded shared history commands against a large synthetic conversation."""

import argparse
import json
import os
import signal
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def fixture():
    def turn(number, size):
        return {
            "id": f"turn-{number}",
            "status": "completed",
            "error": None,
            "items": [
                {
                    "id": f"command-{number}-{i:04}",
                    "type": "commandExecution",
                    "command": f"echo synthetic-{number}-{i:04}",
                    "cwd": "/synthetic/history",
                    "status": "completed",
                    "exitCode": 0,
                    "durationMs": i,
                    "aggregatedOutput": f"Synthetic output {number}-{i:04}\n"
                    + "x" * 4096,
                }
                for i in range(size)
            ],
        }

    return [
        {
            "id": identity,
            "cwd": "/synthetic/history",
            "name": title,
            "preview": "Synthetic history verification",
            "historyMode": "paginated",
            "status": {"type": "idle"},
            "turns": [turn(i, size) for i in range(count)],
        }
        for identity, title, count, size in [
            ("large-history", "Large synthetic history · 1200 commands", 2, 600),
            ("short-history", "Three synthetic turns", 3, 2),
        ]
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write-fixture", type=Path)
    args = parser.parse_args()
    rows = fixture()
    encoded = json.dumps(rows)
    assert len(encoded) > 2 * 1024 * 1024
    if args.write_fixture:
        args.write_fixture.write_text(encoded)
        print(json.dumps({"fixture_bytes": len(encoded), "commands": 1200}))
        return
    binary = ROOT / "target/debug/fleetpane-host"
    with tempfile.TemporaryDirectory(prefix="fleetpane-history-pages-") as directory:
        root = Path(directory)
        history = root / "history.json"
        history.write_text(encoded)
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
                    assert len(result.stdout.encode()) < 256 * 1024
                    return json.loads(result.stdout)["result"]

                for resume in [[], ["--resume"]]:
                    assert (
                        command("inspect", "large-history", *resume)["thread"]["turns"]
                        == []
                    )
                turns = command("history-turns", "large-history")
                assert (
                    turns["data"][0]["id"] == "turn-1" and not turns["data"][0]["items"]
                )
                first = command("history-items", "large-history", "turn-1")
                second = command(
                    "history-items",
                    "large-history",
                    "turn-1",
                    "--cursor",
                    first["nextCursor"],
                )
                assert [row["item"]["id"] for row in first["data"]] == [
                    f"command-1-{i:04}" for i in range(599, 583, -1)
                ]
                assert [row["item"]["id"] for row in second["data"]] == [
                    f"command-1-{i:04}" for i in range(583, 567, -1)
                ]
                seen = []
                turn_args = ["history-turns", "short-history"]
                while True:
                    page = command(*turn_args)
                    turn = page["data"][0]
                    seen.append(turn["id"])
                    items = command("history-items", "short-history", turn["id"])
                    assert len(items["data"]) == 2 and items["nextCursor"] is None
                    if page["nextCursor"] is None:
                        break
                    turn_args = [*turn_args[:2], "--cursor", page["nextCursor"]]
                assert seen == ["turn-2", "turn-1", "turn-0"]
                print(
                    json.dumps(
                        {
                            "synthetic_history_bytes": len(encoded),
                            "bounded_recent_and_older_items": "PASS",
                            "turn_boundaries_and_end": "PASS",
                            "metadata_only_resume": "PASS",
                        }
                    )
                )
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGINT)
                    process.wait(timeout=10)


if __name__ == "__main__":
    main()
