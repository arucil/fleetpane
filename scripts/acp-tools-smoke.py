#!/usr/bin/env python3
"""Minimal ACP tool calls, replacement patches, stable rows and archive restart."""
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
    with tempfile.TemporaryDirectory(prefix="fp-acp-tools-") as directory:
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
                assert time.monotonic() < deadline, "private tool fixture timed out"
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
                capture_output=True, text=True, timeout=12,
            )
            assert result.returncode == 0, result.stderr
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

        def advance(index):
            tmp = root / "gate.tmp"
            tmp.write_text(str(index))
            tmp.replace(root / "tool-progress-next.txt")
            wait(lambda: (root / "tool-progress-stage.txt").exists()
                 and (root / "tool-progress-stage.txt").read_text() == str(index))

        def opened():
            assert not snapshot()["fault"], snapshot()["fault"]
            return command("open", identity)["thread"]

        def tool(status):
            t = opened()
            items = t["turns"][0]["items"]
            return t if len(items) > 1 and items[1]["call"]["status"] == status else None

        try:
            start()
            identity = command("create", str(root))["thread"]["id"]
            command("send", identity, "[tool-progress]")
            first = wait(lambda: tool("pending"))
            a = first["turns"][0]["items"][1]
            assert a["call"]["kind"] == "other" and a["call"]["rawInput"] is False
            assert a["call"]["locations"][0]["line"] == 0
            advance(1)
            wait(lambda: tool("in_progress"))
            advance(2)
            done = wait(lambda: tool("completed"))["turns"][0]["items"]
            assert len(done) == 2 and done[1]["id"] == a["id"]
            assert done[1]["call"]["rawOutput"] == 0
            assert done[1]["call"]["rawInput"] == {"final": False}
            assert done[1]["call"]["name"] == "read_file" and done[1]["call"]["kind"] == "read"
            assert done[1]["call"]["title"] == ""
            assert done[1]["call"]["locations"][0]["line"] == 42
            advance(3)
            cleared = wait(lambda: (t if not (t := opened())["turns"][0]["items"][1]["call"]["content"] else None))
            call = cleared["turns"][0]["items"][1]["call"]
            assert not call["locations"] and call["rawOutput"] == 0
            assert call["rawInput"] == {"final": False}
            advance(4)
            final = wait(lambda: (t if (t := opened())["turns"][0]["status"] == "completed" else None))
            items = final["turns"][0]["items"]
            assert len(items) == 3 and items[2]["call"]["status"] == "failed"
            assert items[2]["call"]["rawOutput"] is False
            assert items[1]["id"] == a["id"]
            stop()
            start()
            recovered = opened()
            assert recovered["status"]["type"] == "archived"
            assert recovered["turns"] == final["turns"]
            assert (root / "prompt-count.txt").read_text() == "[tool-progress]\n"
            print(json.dumps({"provider": "private ACP fixture", "minimal_pending_default": "PASS",
                              "one_card_through_updates": "PASS", "opaque_json_replaced_not_merged": "PASS",
                              "null_preserves_empty_collections_replace": "PASS",
                              "name_kind_and_locations": "PASS", "falsy_output_and_failed_status": "PASS",
                              "restart_preserves_exact_rows_without_replay": "PASS"}))
        finally:
            stop()


if __name__ == "__main__":
    main()
