#!/usr/bin/env python3
"""Idle/live ACP metadata through the real Host, list, permissions and restart."""
import json
import os
import signal
import socket
import sqlite3
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main():
    binary = ROOT / "target/debug/fleetpane-host"
    with tempfile.TemporaryDirectory(prefix="fp-acp-info-") as directory:
        root = Path(directory)
        control = root / "private/control.sock"
        inbox = root / "metadata.jsonl"
        inbox.touch()
        process = None
        generation = 0

        def wait(probe):
            deadline = time.monotonic() + 12
            while True:
                value = probe()
                if value:
                    return value
                assert time.monotonic() < deadline, "private ACP metadata fixture timed out"
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
                    env={**os.environ, "FLEETPANE_FIXTURE_METADATA_INBOX": str(inbox)},
                    stdout=subprocess.DEVNULL, stderr=output, start_new_session=True,
                )
            wait(lambda: process.poll() is None and control.exists()
                 and "ready:" in log.read_text())

        def stop():
            if process is not None and process.poll() is None:
                os.killpg(process.pid, signal.SIGINT)
                process.wait(timeout=10)

        def patch(identity, **fields):
            with inbox.open("a") as output:
                output.write(json.dumps({"sessionId": identity, "update": {
                    "sessionUpdate": "session_info_update", **fields}}) + "\n")

        def opened():
            return command("open", identity)["thread"]

        try:
            start()
            identity = command("create", str(root))["thread"]["id"]
            original = snapshot()
            patch(identity, title="  Idle 中文  ", updatedAt="2026-10-21T09:08:07+08:00",
                  _meta={"vendor": {"opaque": [1, True, None]}})
            idle = wait(lambda: (t if (t := opened())["name"] == "  Idle 中文  " else None))
            assert idle["turns"] == []
            assert idle["acpSessionInfo"]["updatedAt"] == "2026-10-21T09:08:07+08:00"
            listed = command("list")["data"]
            assert next(t for t in listed if t["id"] == identity)["name"] == idle["name"]
            patch(identity, updatedAt=None)
            cleared = wait(lambda: (t if (t := opened())["acpSessionInfo"]["updatedAt"] is None else None))
            assert cleared["name"] == idle["name"]
            assert cleared["acpSessionInfo"]["_meta"] == idle["acpSessionInfo"]["_meta"]
            patch(identity, title=None, _meta=None)
            wait(lambda: opened()["name"] is None)
            command("send", identity, "[permission]")
            pending = wait(lambda: snapshot()["pending_requests"])
            before = opened()
            patch(identity, title="Permission stays 中文")
            live = wait(lambda: (t if (t := opened())["name"] == "Permission stays 中文" else None))
            assert live["turns"] == before["turns"]
            assert snapshot()["pending_requests"] == pending
            assert snapshot()["host_id"] == original["host_id"]
            stop()
            inbox.write_text("")
            start()
            recovered = opened()
            assert recovered["status"]["type"] == "archived"
            assert recovered["name"] == live["name"]
            assert recovered["acpSessionInfo"] == live["acpSessionInfo"]
            assert recovered["turns"][0]["status"] == "unknown"
            assert recovered["turns"][0]["items"] == live["turns"][0]["items"]
            assert not snapshot()["pending_requests"]
            assert (root / "prompt-count.txt").read_text() == "[permission]\n"
            identity = command("create", str(root))["thread"]["id"]
            patch(identity, title="Committed before failure")
            wait(lambda: opened()["name"] == "Committed before failure")
            before_failure = snapshot()
            database = control.parent / (control.name + ".journal") / "operations.sqlite3"
            with sqlite3.connect(database) as db:
                db.execute("CREATE TRIGGER reject_info BEFORE UPDATE ON acp_archive "
                           "WHEN NEW.kind='session' BEGIN SELECT RAISE(FAIL,'injected metadata commit failure'); END")
            patch(identity, title="Must not publish")
            fault = wait(lambda: snapshot()["fault"])
            assert "injected metadata commit failure" in fault
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(5)
                stream.connect(str(control))
                stream.sendall((json.dumps({"command": "events", "host_id": before_failure["host_id"],
                                            "after": before_failure["sequence"]}) + "\n").encode())
                events = json.loads(stream.makefile().readline())["data"]
            assert not any(e["message"].get("method") == "thread/acpSessionInfo/updated" for e in events)
            with sqlite3.connect(database) as db:
                saved = json.loads(db.execute("SELECT data FROM acp_archive WHERE kind='session' AND id=?", (identity,)).fetchone()[0])
                assert saved["summary"]["title"] == "Committed before failure"
            stop()
            inbox.write_text("")
            with sqlite3.connect(database) as db:
                db.execute("DROP TRIGGER reject_info")
            start()
            assert opened()["name"] == "Committed before failure"
            print(json.dumps({"provider": "private ACP fixture",
                              "idle_title_and_exact_opaque_metadata": "PASS",
                              "omitted_fields_preserved_null_clears": "PASS",
                              "list_reflects_live_title": "PASS",
                              "pending_permission_history_and_host_unchanged": "PASS",
                              "restart_preserves_metadata_without_resume_or_replay": "PASS",
                              "failed_commit_fences_host_without_publishing_metadata": "PASS"}))
        finally:
            stop()


if __name__ == "__main__":
    main()
