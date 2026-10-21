#!/usr/bin/env python3
"""Message identity/anonymous chronology through shared Host reads and restart."""
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
    with tempfile.TemporaryDirectory(prefix="fp-acp-order-") as directory:
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
                assert time.monotonic() < deadline, "owned fixture timed out"
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
                capture_output=True, text=True, timeout=12, check=True,
            )
            return json.loads(result.stdout)["result"]

        def start():
            nonlocal process, generation
            generation += 1
            log = root / f"host-{generation}.log"
            with log.open("w") as output:
                process = subprocess.Popen(
                    [str(binary), "serve-acp", str(control), str(root),
                     "python3", str(ROOT / "scripts/acp-fixture.py")],
                    stdout=subprocess.DEVNULL, stderr=output, start_new_session=True,
                )
            wait(lambda: process.poll() is None and control.exists()
                 and "ready:" in log.read_text())

        def stop():
            if process is not None and process.poll() is None:
                os.killpg(process.pid, signal.SIGINT)
                process.wait(timeout=10)

        try:
            start()
            identity = command("create", str(root))["thread"]["id"]
            command("send", identity, "[chronology]")
            thread = wait(lambda: (opened if (opened := command("open", identity)["thread"])
                                  ["turns"][-1]["status"] != "inProgress" else None))
            turn = thread["turns"][0]
            items = turn["items"]
            assert [item["type"] for item in items] == [
                "userMessage", "acpMessage", "acpToolCall", "acpMessage", "reasoning",
                "acpMessage", "acpMessage", "acpMessage", "acpToolCall", "acpMessage",
            ], items
            for index, expected in [(1, "Before 中文\n  keep spaces "),
                                    (3, "After first tool"),
                                    (5, "Tail before tool update"),
                                    (6, "New after tool update"),
                                    (7, "Explicit message one two"),
                                    (9, "Anonymous final 中文")]:
                assert items[index]["content"] == [{"type": "text", "text": expected}]
            assert items[4]["content"] == ["Interleaved thought 中文"]
            assert items[2]["call"]["status"] == "completed"
            assert items[2]["call"]["rawOutput"] == "original result 中文"
            assert len({item["id"] for item in items}) == len(items)
            assert turn["stopReason"] == "end_turn"
            original = snapshot()
            command("send", identity, "second turn 中文")
            wait(lambda: command("open", identity)["thread"]["turns"][-1]["status"] != "inProgress")
            assert command("open", identity)["thread"]["turns"][0] == turn
            stop()
            start()
            restored = command("open", identity)["thread"]
            assert restored["status"]["type"] == "archived"
            assert restored["turns"][0] == turn
            assert snapshot()["host_id"] != original["host_id"]
            assert (root / "prompt-count.txt").read_text() == "[chronology]\nsecond turn 中文\n"
            print(json.dumps({"provider": "private ACP fixture",
                              "ordered_anonymous_text_thought_tool_boundaries": "PASS",
                              "exact_whitespace_and_tool_patch": "PASS",
                              "explicit_message_identity_no_anonymous_collision": "PASS",
                              "second_turn_does_not_modify_first": "PASS",
                              "restart_preserves_exact_item_order_and_ids_no_replay": "PASS"}))
        finally:
            stop()


if __name__ == "__main__":
    main()
