#!/usr/bin/env python3
"""Owned-process ACP crash recovery and archive-before-publication failure proof."""
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


def wait(probe, description):
    deadline = time.monotonic() + 15
    while True:
        value = probe()
        if value:
            return value
        assert time.monotonic() < deadline, description
        time.sleep(0.025)


def alive(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    return True


def main():
    binary = ROOT / "target/debug/fleetpane-host"
    with tempfile.TemporaryDirectory(prefix="fp-acpa-") as directory:
        root = Path(directory)
        control = root / "private/control.sock"
        database = Path(str(control) + ".journal/operations.sqlite3")
        process = None
        pids = []
        generation = 0

        def raw(value):
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(8)
                stream.connect(str(control))
                stream.sendall(json.dumps(value).encode() + b"\n")
                return json.loads(stream.makefile().readline())

        def snapshot():
            response = raw({"command": "snapshot"})
            assert response["response"] == "snapshot", response
            return response["data"]

        def run(*arguments):
            return subprocess.run(
                [str(binary), "session", str(control), "--take-control", *arguments],
                capture_output=True, text=True, timeout=12, check=False,
            )

        def command(*arguments):
            response = run(*arguments)
            assert response.returncode == 0, response.stderr
            return json.loads(response.stdout)["result"]

        def start():
            nonlocal process, generation
            generation += 1
            path = root / f"host-{generation}.log"
            with path.open("w") as log:
                process = subprocess.Popen(
                    [str(binary), "serve-acp", str(control), str(root), "python3", str(ROOT / "scripts/acp-fixture.py")],
                    stderr=log, stdout=subprocess.DEVNULL, start_new_session=True,
                )
            wait(lambda: process.poll() is None and control.exists() and "ready:" in path.read_text(), "owned Host ready")
            state = snapshot()
            pids.extend([process.pid, state["provider_pid"]])
            assert state["fault"] is None
            return state

        def stop(sig=signal.SIGINT):
            nonlocal process
            os.killpg(process.pid, sig)
            process.wait(timeout=10)
            process = None

        def settled(identity):
            thread = command("open", identity)["thread"]
            return thread if thread["turns"] and thread["turns"][-1]["status"] != "inProgress" else None

        try:
            first = start()
            one = command("create", str(root))["thread"]["id"]
            two = command("create", str(root))["thread"]["id"]
            command("send", one, "first 中文")
            before = wait(lambda: settled(one), "first prompt")
            command("send", two, "independent")
            independent = wait(lambda: settled(two), "independent prompt")
            command("send", one, "[permission]")
            permission = wait(lambda: snapshot()["pending_requests"], "unanswered permission")[0]
            live = command("open", one)["thread"]
            cursor = command("history-turns", one)["nextCursor"]
            assert cursor is not None
            prompts = (root / "prompt-count.txt").read_text()
            stop(signal.SIGKILL)
            second = start()
            assert second["host_id"] != first["host_id"]
            assert second["pending_requests"] == []
            archived = command("open", one)["thread"]
            assert archived["status"]["type"] == "archived"
            assert archived["turns"][0] == before["turns"][0]
            unknown = archived["turns"][-1]
            assert unknown["id"] == live["turns"][-1]["id"]
            assert unknown["items"] == live["turns"][-1]["items"]
            assert unknown["status"] == "unknown" and "nothing was replayed" in unknown["error"]["message"]
            assert command("open", two)["thread"]["turns"] == independent["turns"]
            assert run("send", one, "must not run").returncode != 0
            assert run("permission", json.dumps(permission["id"]), "allow-one").returncode != 0
            assert run("history-turns", one, "--cursor", cursor).returncode != 0
            assert (root / "prompt-count.txt").read_text() == prompts
            assert command("inspect", one, "--resume")["thread"]["turns"] == []
            assert command("open", one)["thread"]["turns"] == archived["turns"]
            assert command("open", two)["thread"]["status"]["type"] == "archived"
            command("send", one, "after crash")
            recovered = wait(lambda: settled(one), "post-crash prompt")
            assert len(recovered["turns"]) == 3
            assert (root / "prompt-count.txt").read_text() == prompts + "after crash\n"
            before_failure = snapshot()["sequence"]
            with sqlite3.connect(database) as connection:
                connection.execute("""CREATE TRIGGER fail_archive BEFORE INSERT ON acp_archive
                    WHEN NEW.kind='item' AND json_extract(NEW.data,'$.type')='acpMessage'
                    BEGIN SELECT RAISE(FAIL,'synthetic archive write failure'); END""")
            failed_turn = command("send", one, "archive must fail")["turn"]["id"]
            fault = wait(lambda: (state if (state := snapshot())["fault"] else None), "archive failure fences Host")
            assert fault["fault_scope"] == "host", fault
            assert "ACP archive write failed" in fault["fault"] and "synthetic archive write failure" in fault["fault"]
            events = raw({"command": "events", "host_id": fault["host_id"], "after": before_failure})
            assert events["response"] == "events", events
            assert not any(event["message"].get("params", {}).get("item", {}).get("type") == "acpMessage" for event in events["data"])
            assert run("send", one, "must stay fenced").returncode != 0
            with sqlite3.connect(database) as connection:
                assert connection.execute("SELECT count(*) FROM acp_archive WHERE parent=? AND json_extract(data,'$.type')='acpMessage'", [failed_turn]).fetchone()[0] == 0
                connection.execute("DROP TRIGGER fail_archive")
            stop()
            start()
            last = command("open", one)["thread"]["turns"][-1]
            assert last["id"] == failed_turn and last["status"] == "unknown"
            assert command("open", two)["thread"]["turns"] == independent["turns"]
            # Startup must refuse damaged known history, rather than return an empty directory.
            stop()
            with sqlite3.connect(database) as connection:
                connection.execute("UPDATE acp_archive SET data='{' WHERE kind='session' AND session=?", [one])
            rejected = subprocess.run(
                [str(binary), "serve-acp", str(control), str(root), "python3", str(ROOT / "scripts/acp-fixture.py")],
                capture_output=True, text=True, timeout=12, check=False,
            )
            assert rejected.returncode != 0 and "ready:" not in rejected.stderr
            assert not control.exists()
        finally:
            if process is not None and process.poll() is None:
                stop()
        wait(lambda: all(not alive(pid) for pid in pids), "all owned Host/provider processes removed")
        print(json.dumps({
            "provider": "synthetic-acp-v1", "kill_restart_retains_exact_items_and_order": "PASS",
            "unknown_turn_and_no_prompt_or_permission_replay": "PASS", "read_only_archive_and_explicit_resume": "PASS",
            "other_session_and_stale_cursor_isolation": "PASS", "archive_failure_fences_host_before_publication": "PASS",
            "corrupt_archive_refuses_startup": "PASS", "owned_host_provider_pids_removed": len(pids),
        }))


if __name__ == "__main__":
    main()
