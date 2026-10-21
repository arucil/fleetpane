#!/usr/bin/env python3
"""Prompt admission, quiet activity and explicit ACP settlement are distinct."""

import json
import os
import signal
import socket
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REASONS = ("end_turn", "max_tokens", "max_turn_requests", "refusal", "cancelled")


def wait(probe, description):
    deadline = time.monotonic() + 15
    while True:
        result = probe()
        if result:
            return result
        assert time.monotonic() < deadline, description
        time.sleep(0.025)


def main():
    binary = ROOT / "target/debug/fleetpane-host"
    with tempfile.TemporaryDirectory(prefix="fleetpane-acp-stop-") as directory:
        root = Path(directory)
        control = root / "private/control.sock"
        owned = []
        process = None

        def raw(value):
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(8)
                stream.connect(str(control))
                stream.sendall(json.dumps(value).encode() + b"\n")
                return json.loads(stream.makefile().readline())

        def snapshot():
            result = raw({"command": "snapshot"})
            assert result["response"] == "snapshot", result
            assert result["data"]["fault"] is None, result
            return result["data"]

        def command(*args):
            result = subprocess.run(
                [str(binary), "session", str(control), "--take-control", *args],
                text=True, capture_output=True, timeout=12, check=True,
            )
            return json.loads(result.stdout)["result"]

        def start(number):
            nonlocal process
            log = root / f"host-{number}.log"
            with log.open("w") as output:
                process = subprocess.Popen(
                    [str(binary), "serve-acp", str(control), str(root), "python3",
                     str(ROOT / "scripts/acp-fixture.py")],
                    stdout=subprocess.DEVNULL, stderr=output, start_new_session=True,
                )

            def ready():
                assert process.poll() is None, log.read_text()
                return "ready:" in log.read_text()

            wait(ready, "owned Host initialization")
            state = snapshot()
            owned.extend([state["host_pid"], state["provider_pid"]])

        def stop():
            nonlocal process
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGINT)
            process.wait(timeout=10)
            process = None
            assert not control.exists(), "owned socket cleanup"

        def read(identity):
            return command("open", identity)["thread"]

        def ended(identity):
            thread = read(identity)
            return thread if thread["turns"][-1]["status"] != "inProgress" else None

        try:
            start(1)
            gated = command("create", str(root))["thread"]["id"]
            other = command("create", str(root))["thread"]["id"]
            accepted = command("send", gated, "[settlement-gate]")
            assert accepted["turn"]["status"] == "inProgress"
            wait(lambda: len(read(gated)["turns"][-1]["items"]) == 2,
                 "first observed output")
            # Deliberate silence longer than Paseo's former two-second heuristic.
            # The provider reply is held by a file gate, not a timing guess.
            time.sleep(2.2)
            assert read(gated)["turns"][-1]["status"] == "inProgress"
            assert "stopReason" not in read(gated)["turns"][-1]
            for reason in REASONS:
                command("send", other, f"[stop:{reason}]")
                thread = wait(lambda: ended(other), "explicit prompt settlement")
                turn = thread["turns"][-1]
                assert turn["stopReason"] == reason, turn
                assert turn["status"] == ("interrupted" if reason == "cancelled" else "completed")
                assert turn["error"] is None
                page = command("history-turns", other)["data"][0]
                assert page["stopReason"] == reason and page["items"] == []
                assert read(gated)["turns"][-1]["status"] == "inProgress"
            (root / "fixture-settlement.txt").write_text("max_tokens")
            first = wait(lambda: ended(gated), "gated protocol settlement")
            second = read(other)
            assert first["turns"][-1]["stopReason"] == "max_tokens"
            prompt_log = (root / "prompt-count.txt").read_text()
            stop()
            start(2)
            for identity, previous in [(gated, first), (other, second)]:
                archived = read(identity)
                assert archived["status"]["type"] == "archived"
                assert archived["turns"] == previous["turns"]
                assert command("history-turns", identity)["data"][0]["stopReason"] == previous["turns"][-1]["stopReason"]
            assert (root / "prompt-count.txt").read_text() == prompt_log
        finally:
            if process is not None:
                stop()

        def absent(pid):
            try:
                os.kill(pid, 0)
            except ProcessLookupError:
                return True
            return False

        wait(lambda: all(absent(pid) for pid in owned), "owned process cleanup")
        print(json.dumps({
            "provider": "synthetic ACP",
            "admission_before_settlement": "PASS",
            "silence_over_two_seconds_is_not_completion": "PASS",
            "independent_session_progress_while_first_waits": "PASS",
            "exact_stop_reasons_full_and_paged": list(REASONS),
            "exact_history_after_restart_without_replay": "PASS",
            "owned_host_provider_pids_removed": len(owned),
        }))


if __name__ == "__main__":
    main()
