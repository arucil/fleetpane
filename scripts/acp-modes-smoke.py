#!/usr/bin/env python3
"""ACP v1 modes across actual Host admission, activity, archive and restart."""
import json
import os
import signal
import socket
import sqlite3
import subprocess
import tempfile
import time
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"


def main():
    with tempfile.TemporaryDirectory(prefix="fp-acp-modes-") as directory:
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
                assert time.monotonic() < deadline, "private mode fixture timed out"
                time.sleep(0.025)

        def raw(value):
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(5)
                stream.connect(str(control))
                stream.sendall((json.dumps(value) + "\n").encode())
                return json.loads(stream.makefile().readline())

        def snapshot():
            state = raw({"command": "snapshot"})["data"]
            assert state["fault"] is None, state
            return state

        def run(*args):
            return subprocess.run([str(BINARY), "session", str(control), "--take-control", *args],
                                  text=True, capture_output=True, timeout=12)

        def command(*args):
            result = run(*args)
            assert result.returncode == 0, result.stderr
            return json.loads(result.stdout)["result"]

        def opened(identity):
            return command("open", identity)["thread"]

        def mode(identity):
            return opened(identity)["acpModes"]["currentModeId"]

        def patch(identity, current):
            with inbox.open("a") as output:
                output.write(json.dumps({"sessionId": identity, "update": {
                    "sessionUpdate": "current_mode_update", "currentModeId": current}}) + "\n")

        def start():
            nonlocal process, generation
            generation += 1
            log = root / f"host-{generation}.log"
            with log.open("w") as output:
                process = subprocess.Popen([str(BINARY), "serve-acp", str(control), str(root),
                                            "python3", str(ROOT / "scripts/acp-fixture.py")],
                    env={**os.environ, "FLEETPANE_FIXTURE_MODES": "1",
                         "FLEETPANE_FIXTURE_METADATA_INBOX": str(inbox),
                         **({"FLEETPANE_FIXTURE_RESUME_MODE": "fresh"} if generation > 1 else {})},
                    stdout=subprocess.DEVNULL, stderr=output, start_new_session=True)
            wait(lambda: process.poll() is not None or (control.exists() and "ready:" in log.read_text()))
            assert process.poll() is None, log.read_text()

        def stop():
            if process is not None and process.poll() is None:
                os.killpg(process.pid, signal.SIGINT)
                process.wait(timeout=10)

        try:
            start()
            identity = command("create", str(root))["thread"]["id"]
            other = command("create", str(root))["thread"]["id"]
            config = opened(identity)["configOptions"]
            assert mode(identity) == "ask"
            assert command("mode", identity, "code")["modes"]["currentModeId"] == "code"
            assert mode(other) == "ask"
            before = (root / "mode-wire.jsonl").read_text()
            assert run("mode", identity, "missing").returncode != 0
            assert (root / "mode-wire.jsonl").read_text() == before
            denied = run("mode", identity, "restricted")
            assert denied.returncode != 0 and "Synthetic mode change denied" in denied.stderr
            assert mode(identity) == "code"
            command("send", identity, "[permission]")
            pending = wait(lambda: snapshot()["pending_requests"])
            history = opened(identity)["turns"]
            hold = root / f"mode-hold-{identity}"
            hold.touch()
            operation = str(uuid.uuid4())
            accepted = raw({"command": "execute", "lease": snapshot()["owner"],
                            "operation_id": operation, "method": "acp/mode/set",
                            "params": {"threadId": identity, "modeId": "ask"}})
            assert accepted["response"] == "operation", accepted
            wait(lambda: mode(identity) == "ask")
            patch(identity, "provider-only")
            wait(lambda: mode(identity) == "provider-only")
            assert snapshot()["pending_requests"] == pending
            assert opened(identity)["turns"] == history
            assert run("mode", identity, "code").returncode != 0
            assert command("mode", other, "code")["modes"]["currentModeId"] == "code"
            # Stop remains admitted even while the mode ACK is gated.
            command("stop", identity, history[-1]["id"])
            wait(lambda: opened(identity)["turns"][-1]["status"] == "interrupted")
            assert not snapshot()["pending_requests"]
            hold.unlink()
            settled = wait(lambda: (op if (op := raw({"command": "operation", "operation_id": operation})["data"])["outcome"]["state"] == "finished" else None))
            assert settled["outcome"]["response"]["result"]["modes"]["currentModeId"] == "provider-only"
            assert mode(identity) == "provider-only"
            assert opened(identity)["configOptions"] == config
            original_host = snapshot()["host_id"]
            stop()
            inbox.write_text("")
            start()
            archived = opened(identity)
            assert snapshot()["host_id"] != original_host
            assert archived["status"]["type"] == "archived"
            assert archived["acpModes"]["currentModeId"] == "provider-only"
            assert run("mode", identity, "code").returncode != 0
            resumed = command("inspect", identity, "--resume")["thread"]
            assert resumed["acpModes"] == {"currentModeId": "fresh", "availableModes": [{"id": "fresh", "name": "Fresh mode"}]}
            assert run("mode", identity, "code").returncode != 0
            assert command("mode", identity, "fresh")["modes"] == resumed["acpModes"]
            assert opened(identity)["turns"] == archived["turns"]
            assert (root / "prompt-count.txt").read_text() == "[permission]\n"
            database = control.parent / (control.name + ".journal") / "operations.sqlite3"
            with sqlite3.connect(database) as db:
                saved = json.loads(db.execute("SELECT data FROM acp_archive WHERE kind='session' AND id=?", (identity,)).fetchone()[0])
                assert saved["modes"] == resumed["acpModes"]
            print(json.dumps({"provider": "private ACP v1 fixture", "native": "separate acceptance",
                "advertised_choices_exact_session_scope_and_config_preserved": "PASS",
                "provider_rejection_retains_confirmed_mode": "PASS", "unknown_choice_no_provider_io": "PASS",
                "active_permission_mode_update_keeps_history_and_approval": "PASS",
                "pending_mode_is_serial_but_other_session_and_stop_work": "PASS",
                "newer_notification_wins_over_late_ack": "PASS",
                "restart_read_only_archive_and_authoritative_resume": "PASS", "prompts_replayed": 0}))
        finally:
            stop()


if __name__ == "__main__":
    main()
