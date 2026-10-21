#!/usr/bin/env python3
"""Provider archive browsing/restoration through shared commands; synthetic data."""

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


def main():
    receipts = []
    with tempfile.TemporaryDirectory(prefix="fp-restore-") as temporary:
        root = Path(temporary)
        history = root / "history.json"
        rows = [
            {
                "id": f"archived-{n}",
                "cwd": str(root),
                "name": "[restore-fail]" if n == 106 else f"Archived 中文 {n}",
                "preview": "preserved",
                "status": {"type": "notLoaded"},
                "turns": [],
                "fixtureArchived": True,
            }
            for n in range(107)
        ]
        rows.append(
            {
                "id": "active-other",
                "cwd": str(root),
                "name": "Other session",
                "preview": "other",
                "status": {"type": "idle"},
                "turns": [],
            }
        )
        history.write_text(json.dumps(rows))
        for acp in (False, True):
            home = root / ("acp" if acp else "codex")
            home.mkdir()
            endpoint = home / "host/control.sock"
            with (home / "host.log").open("w") as output:
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
                        *([] if acp else ["--history", str(history)]),
                    ],
                    stdout=subprocess.DEVNULL,
                    stderr=output,
                    start_new_session=True,
                )
            try:
                deadline = time.monotonic() + 10
                while (
                    not endpoint.exists()
                    or "ready:" not in (home / "host.log").read_text()
                ):
                    assert process.poll() is None and time.monotonic() < deadline, (
                        home / "host.log"
                    ).read_text()
                    time.sleep(0.02)

                def rpc(request):
                    with socket.socket(socket.AF_UNIX) as stream:
                        stream.settimeout(10)
                        stream.connect(str(endpoint))
                        stream.sendall(json.dumps(request).encode() + b"\n")
                        return json.loads(stream.makefile("rb").readline())

                def run(*arguments):
                    return subprocess.run(
                        [
                            str(BINARY),
                            "session",
                            str(endpoint),
                            "--take-control",
                            *arguments,
                        ],
                        capture_output=True,
                        text=True,
                        timeout=25,
                        check=False,
                    )

                def command(*arguments):
                    result = run(*arguments)
                    assert result.returncode == 0, (arguments, result.stderr)
                    return json.loads(result.stdout)["result"]

                def state():
                    return rpc({"command": "snapshot"})["data"]

                if acp:
                    command("list")
                    before = state()
                    refused = run("list", "--archived")
                    assert (
                        refused.returncode != 0
                        and "does not advertise provider archive listing"
                        in refused.stderr
                    ), refused.stderr
                    after = state()
                    assert after["journal_total"] == before["journal_total"]
                    assert after["activity"] == before["activity"]
                    receipts.append(
                        {
                            "provider": "ACP fixture",
                            "archive_capability_refusal": "passed",
                            "no_archive_inference": True,
                        }
                    )
                    continue
                assert [row["id"] for row in command("list")["data"]] == [
                    "active-other"
                ]
                first = command("list", "--archived")
                assert len(first["data"]) == 100 and first["nextCursor"]
                second = command("list", "--archived", "--cursor", first["nextCursor"])
                assert len(second["data"]) == 7 and not second["nextCursor"]
                assert all(
                    row["status"]["type"] == "notLoaded"
                    for row in first["data"] + second["data"]
                )
                filtered = command(
                    "list", "--cwd", str(root), "--search", "中文 4", "--archived"
                )
                assert len(filtered["data"]) == 11
                assert run("list", "--archived", "--archived").returncode != 0
                # An unrelated pending approval must survive observation and restoration.
                command("send", "active-other", "[approval]")
                deadline = time.monotonic() + 5
                while not state()["pending_requests"]:
                    assert time.monotonic() < deadline
                    time.sleep(0.02)
                before = state()
                command("open", "archived-4")
                observed = state()
                assert (
                    observed["activity"] == before["activity"]
                    and observed["pending_requests"] == before["pending_requests"]
                )
                assert observed["journal_total"] == before["journal_total"]
                refusal = run("restore", "archived-106")
                assert (
                    refusal.returncode != 0
                    and "Synthetic restore refusal" in refusal.stderr
                )
                assert (
                    command("list", "--archived", "--search", "[restore-fail]")["data"][
                        0
                    ]["id"]
                    == "archived-106"
                )
                assert run("drain").returncode == 0
                refusal = run("restore", "archived-4")
                assert refusal.returncode != 0 and "draining" in refusal.stderr.lower()
                assert run("ready").returncode == 0
                receipt = run("restore", "archived-4")
                assert receipt.returncode == 0, receipt.stderr
                result = json.loads(receipt.stdout)
                assert result["result"]["thread"]["id"] == "archived-4"
                assert result["result"]["thread"]["status"]["type"] == "notLoaded"
                after = state()
                assert (
                    after["activity"] == before["activity"]
                    and after["pending_requests"] == before["pending_requests"]
                )
                # The exact UUID/parameters return the original receipt, without another admission.
                operation = rpc(
                    {"command": "operation", "operation_id": result["operationId"]}
                )["data"]
                journal_count = after["journal_total"]
                again = rpc(
                    {
                        "command": "execute",
                        "lease": after["owner"],
                        "operation_id": operation["id"],
                        "method": operation["method"],
                        "params": operation["params"],
                    }
                )
                assert again["response"] == "operation" and again["data"] == operation
                assert state()["journal_total"] == journal_count
                assert (
                    not command("list", "--archived", "--search", "中文 4")["data"][0][
                        "id"
                    ]
                    == "archived-4"
                )
                assert "archived-4" in [row["id"] for row in command("list")["data"]]
                assert (
                    command("inspect", "archived-4", "--resume")["thread"]["status"][
                        "type"
                    ]
                    == "idle"
                )
                receipts.append(
                    {
                        "provider": "Codex fixture",
                        "archived_rows": 107,
                        "page_lengths": [100, 7],
                        "read_only_observation": True,
                        "refusal_preserves_membership": True,
                        "drain_refused": True,
                        "exact_receipt_dedup": True,
                        "unrelated_pending_turn_preserved": True,
                        "explicit_resume": True,
                    }
                )
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGINT)
                    process.wait(timeout=10)
    print(json.dumps(receipts, indent=2))


if __name__ == "__main__":
    main()
