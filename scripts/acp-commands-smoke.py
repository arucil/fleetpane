#!/usr/bin/env python3
"""Advertised ACP commands are idle/live metadata and expire with provider residency."""

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
BINARY = ROOT / "target/debug/fleetpane-host"


def main():
    with tempfile.TemporaryDirectory(prefix="fp-acp-commands-") as directory:
        root = Path(directory)
        control = root / "private/control.sock"
        inbox = root / "metadata.jsonl"
        inbox.touch()
        process = None
        generation = 0

        def wait(probe):
            deadline = time.monotonic() + 12
            while True:
                result = probe()
                if result:
                    return result
                assert time.monotonic() < deadline, "ACP commands fixture timed out"
                time.sleep(0.025)

        def raw(value):
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(5)
                stream.connect(str(control))
                stream.sendall((json.dumps(value) + "\n").encode())
                return json.loads(stream.makefile().readline())["data"]

        def snapshot():
            state = raw({"command": "snapshot"})
            assert state["fault"] is None, state
            return state

        def command(*args):
            result = subprocess.run(
                [str(BINARY), "session", str(control), "--take-control", *args],
                check=False,
                capture_output=True,
                text=True,
                timeout=12,
            )
            assert result.returncode == 0, result.stderr
            return json.loads(result.stdout)["result"]

        def opened(identity):
            return command("open", identity)["thread"]

        def patch(identity, commands):
            with inbox.open("a") as output:
                output.write(
                    json.dumps(
                        {
                            "sessionId": identity,
                            "update": {
                                "sessionUpdate": "available_commands_update",
                                "availableCommands": commands,
                            },
                        }
                    )
                    + "\n"
                )

        def start():
            nonlocal process, generation
            generation += 1
            log = root / f"host-{generation}.log"
            with log.open("w") as output:
                process = subprocess.Popen(
                    [
                        str(BINARY),
                        "serve-acp",
                        str(control),
                        str(root),
                        "python3",
                        str(ROOT / "scripts/acp-fixture.py"),
                    ],
                    env={**os.environ, "FLEETPANE_FIXTURE_METADATA_INBOX": str(inbox)},
                    stdout=subprocess.DEVNULL,
                    stderr=output,
                    start_new_session=True,
                )
            wait(
                lambda: (
                    process.poll() is not None
                    or (control.exists() and "ready:" in log.read_text())
                )
            )
            assert process.poll() is None, log.read_text()

        def stop():
            if process is not None and process.poll() is None:
                os.killpg(process.pid, signal.SIGINT)
                process.wait(timeout=10)

        try:
            start()
            identity = command("create", str(root))["thread"]["id"]
            other = command("create", str(root))["thread"]["id"]
            assert opened(identity)["acpCommands"] is None
            catalog = [
                {
                    "name": "web",
                    "description": "Search 中文",
                    "input": {"hint": "query", "future": 7},
                    "_meta": {"opaque": True},
                    "future": [1, None],
                },
                {"name": "检验", "description": "Exact name"},
            ]
            before = snapshot()
            patch(identity, catalog)
            loaded = wait(
                lambda: t if (t := opened(identity))["acpCommands"] == catalog else None
            )
            assert loaded["turns"] == []
            assert opened(other)["acpCommands"] is None
            assert snapshot()["activity"] == before["activity"]
            assert not (root / "prompt-count.txt").exists()
            command("send", identity, "[permission]")
            pending = wait(lambda: snapshot()["pending_requests"])
            history = opened(identity)["turns"]
            patch(identity, [])
            wait(lambda: opened(identity)["acpCommands"] == [])
            assert snapshot()["pending_requests"] == pending
            assert opened(identity)["turns"] == history
            patch(identity, catalog)
            wait(lambda: opened(identity)["acpCommands"] == catalog)
            database = (
                control.parent / (control.name + ".journal") / "operations.sqlite3"
            )
            with sqlite3.connect(database) as db:
                stored = json.loads(
                    db.execute(
                        "SELECT data FROM acp_archive WHERE kind='session' AND id=?",
                        (identity,),
                    ).fetchone()[0]
                )
            assert "commands" not in stored
            stop()
            inbox.write_text("")
            start()
            restored = opened(identity)
            assert restored["status"]["type"] == "archived"
            assert restored["acpCommands"] is None
            resumed = command("inspect", identity, "--resume")["thread"]
            assert resumed["acpCommands"] is None
            patch(identity, [{"name": "fresh", "description": "After resume"}])
            wait(
                lambda: (
                    opened(identity)["acpCommands"]
                    == [{"name": "fresh", "description": "After resume"}]
                )
            )
            assert (root / "prompt-count.txt").read_text() == "[permission]\n"
            print(
                json.dumps(
                    {
                        "idle_and_pending_updates": True,
                        "exact_extensions_and_clear": True,
                        "no_implicit_prompt_or_history_item": True,
                        "session_isolation": True,
                        "restart_and_resume_forget_commands": True,
                        "original_permission_preserved": True,
                    },
                    indent=2,
                )
            )
        finally:
            stop()


if __name__ == "__main__":
    main()
