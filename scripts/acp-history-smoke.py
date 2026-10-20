#!/usr/bin/env python3
"""Byte-bounded ACP observed history through the shared Host commands."""

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
    with tempfile.TemporaryDirectory(prefix="fp-acph-") as directory:
        root = Path(directory)
        control = root / "private/control.sock"
        log_path = root / "host.log"
        with log_path.open("w") as log:
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
                stderr=log,
                start_new_session=True,
            )
        try:
            deadline = time.monotonic() + 15
            while "ready:" not in log_path.read_text():
                assert process.poll() is None and time.monotonic() < deadline
                time.sleep(0.025)

            def raw(value):
                with socket.socket(socket.AF_UNIX) as stream:
                    stream.settimeout(8)
                    stream.connect(str(control))
                    stream.sendall(json.dumps(value).encode() + b"\n")
                    return json.loads(stream.makefile().readline())

            def snapshot():
                value = raw({"command": "snapshot"})["data"]
                assert value["fault"] is None, value["fault"]
                return value

            def run(*arguments):
                return subprocess.run(
                    [
                        str(binary),
                        "session",
                        str(control),
                        "--take-control",
                        *arguments,
                    ],
                    capture_output=True,
                    text=True,
                    timeout=15,
                    check=False,
                )

            def command(*arguments):
                result = run(*arguments)
                assert result.returncode == 0, result.stderr
                assert len(result.stdout.encode()) < 256 * 1024
                return json.loads(result.stdout)["result"]

            def send(text):
                boundary = snapshot()["sequence"]
                turn = command("send", identity, text)["turn"]["id"]
                deadline = time.monotonic() + 15
                while True:
                    state = snapshot()
                    events = raw(
                        {
                            "command": "events",
                            "host_id": state["host_id"],
                            "after": boundary,
                        }
                    )
                    assert events["response"] == "events", events
                    if any(
                        e["message"].get("method") == "turn/completed"
                        and e["message"]["params"]["turn"]["id"] == turn
                        for e in events["data"]
                    ):
                        return turn
                    if events["data"]:
                        boundary = events["data"][-1]["sequence"]
                    assert time.monotonic() < deadline, "prompt settlement"
                    time.sleep(0.025)

            identity = command("create", str(root))["thread"]["id"]
            old_turn = send("older turn")
            large_turn = send("[large-history]")
            initial = snapshot()
            for flags in [[], ["--resume"]]:
                inspected = command("inspect", identity, *flags)["thread"]
                assert inspected["turns"] == [], "ACP inspect must return metadata only"
                assert inspected["historyPaging"] is True
                assert inspected["historyMode"] == "acp-observed"
            # Explicit resume is a mutation, even for a resident session.
            after_resume = snapshot()
            turns = command("history-turns", identity)
            assert (
                turns["data"][0]["id"] == large_turn and turns["data"][0]["items"] == []
            )
            first = command("history-items", identity, large_turn)
            assert 1 < len(first["data"]) < 16, (
                "page must shrink to its encoded byte budget"
            )
            assert first["data"][0]["item"]["type"] == "acpMessage"
            cursor = first["nextCursor"]
            all_entries = list(first["data"])
            page_sizes = [len(first["data"])]
            while cursor:
                page = command(
                    "history-items", identity, large_turn, "--cursor", cursor
                )
                all_entries.extend(page["data"])
                page_sizes.append(len(page["data"]))
                cursor = page["nextCursor"]
            assert len(all_entries) == 26
            ids = [entry["item"]["id"] for entry in all_entries]
            assert len(set(ids)) == 26
            tools = [
                entry["item"]["call"]
                for entry in all_entries
                if entry["item"]["type"] == "acpToolCall"
            ]
            assert [tool["rawInput"]["index"] for tool in tools] == list(
                range(23, -1, -1)
            )
            for tool in tools:
                index = tool["rawInput"]["index"]
                assert (
                    tool["content"][0]["content"]["text"]
                    == f"BEGIN {index:02}\n" + "x" * 60000 + f"\nEND {index:02}"
                )
            history_bytes = len(json.dumps(all_entries).encode())
            assert history_bytes > 1024 * 1024
            full = run("open", identity)
            assert full.returncode != 0 and "paged" in full.stderr, full.stderr
            assert snapshot()["fault"] is None, (
                "large full read must not fault the Host"
            )
            assert (
                run(
                    "history-items", identity, old_turn, "--cursor", first["nextCursor"]
                ).returncode
                != 0
            )
            assert (
                run(
                    "history-turns", identity, "--cursor", first["nextCursor"]
                ).returncode
                != 0
            )
            assert (
                run(
                    "history-items", identity, large_turn, "--cursor", "invalid"
                ).returncode
                != 0
            )
            before_send = snapshot()
            assert before_send["operation_ids"] == after_resume["operation_ids"], (
                "history reads must not consume mutation admission"
            )
            new_turn = send("after paged reads")
            older = command("history-turns", identity, "--cursor", turns["nextCursor"])
            assert older["data"][0]["id"] == old_turn and older["nextCursor"] is None
            assert command("history-turns", identity)["data"][0]["id"] == new_turn
            final = snapshot()
            assert (
                final["host_pid"] == initial["host_pid"]
                and final["provider_pid"] == initial["provider_pid"]
            )
            print(
                json.dumps(
                    {
                        "provider": "synthetic ACP",
                        "observed_history_bytes": history_bytes,
                        "items": len(all_entries),
                        "page_sizes": page_sizes,
                        "metadata_only_read_resume": "PASS",
                        "scoped_stable_cursors": "PASS",
                        "oversize_full_read_is_error_not_host_fault": "PASS",
                        "send_after_paging_same_runtime": "PASS",
                    }
                )
            )
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGINT)
            process.wait(timeout=10)
            assert not control.exists(), "owned socket cleanup"


if __name__ == "__main__":
    main()
