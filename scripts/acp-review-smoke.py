#!/usr/bin/env python3
"""ACP permission details, exact choices, isolation and transient request lifecycle."""
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
    with tempfile.TemporaryDirectory(prefix="fp-acp-review-") as directory:
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
                assert time.monotonic() < deadline, "private review fixture timed out"
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

        def raw(value):
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(5)
                stream.connect(str(control))
                stream.sendall(json.dumps(value).encode() + b"\n")
                return json.loads(stream.makefile().readline())

        def opened(identity):
            assert not snapshot()["fault"], snapshot()["fault"]
            return command("open", identity)["thread"]

        def permission(request, choice):
            result = subprocess.run([str(binary), "session", str(control), "--take-control", "permission", json.dumps(request["id"]), choice], capture_output=True, text=True, timeout=12)
            assert result.returncode == 0, result.stderr
            assert json.loads(result.stdout) == {"answered": True}

        try:
            start()
            identity = command("create", str(root))["thread"]["id"]
            other = command("create", str(root))["thread"]["id"]
            command("send", other, "[permission]")
            other_request = wait(lambda: snapshot()["pending_requests"])[0]
            command("send", identity, "[permission-review]")
            original = None
            old_request = None
            for index, (title, choice) in enumerate([("Review settings 中文", "reject-one"), ("Review plan 中文", "allow-one"), ("Review boolean input", "reject-one")]):
                pending = wait(lambda: (p if len(p := snapshot()["pending_requests"]) == 2 and any(p["params"]["threadId"] == identity and p["params"]["toolCall"]["title"] == title for p in p) else None))
                current = next(p for p in pending if p["params"]["threadId"] == identity)
                observed = opened(identity)
                if original is None:
                    original = observed["turns"][0]["items"]
                    assert original[1]["call"]["rawInput"] == {"command": "old-command", "old": True}
                    assert current["params"]["toolCall"]["rawInput"]["enabled"] is False
                    assert current["params"]["toolCall"]["locations"][0]["line"] == 42
                elif index == 1:
                    assert current["params"]["toolCall"]["rawInput"]["plan"].startswith("# Proposed plan 中文")
                    assert len(current["params"]["toolCall"]["content"][0]["content"]["text"].splitlines()) == 180
                else:
                    assert current["params"]["toolCall"]["rawInput"] is False
                assert observed["turns"][0]["items"] == original
                assert other_request in pending
                previous = snapshot()["owner"]
                command("list")  # Replacement controller preserves exact pending details.
                assert current in snapshot()["pending_requests"]
                stale = raw({"command": "respond", "lease": previous, "request_id": current["id"], "result": {"outcome": {"outcome": "selected", "optionId": choice}}})
                assert stale["response"] == "error" and stale["data"]["code"] == "stale_controller", stale
                permission(current, choice)
                answer_path = root / f"review-{identity}-{index}-answer.json"
                wait(lambda: answer_path.exists())
                assert json.loads(answer_path.read_text()) == {"outcome": {"outcome": "selected", "optionId": choice}}
                assert other_request in snapshot()["pending_requests"]
                if old_request is not None:
                    late = raw({"command": "respond", "lease": snapshot()["owner"], "request_id": old_request["id"], "result": {"outcome": {"outcome": "selected", "optionId": choice}}})
                    assert late["response"] == "error" and late["data"]["code"] == "unknown_request"
                old_request = current
            final = wait(lambda: (t if (t := opened(identity))["turns"][0]["status"] == "completed" else None))
            assert final["turns"][0]["items"] == original
            assert not (root / "review-settings.md").exists(), "Fixture never executes proposed operations"
            permission(other_request, "reject-one")
            wait(lambda: not snapshot()["pending_requests"])
            stop()
            start()
            archived = opened(identity)
            assert archived["status"]["type"] == "archived" and archived["turns"] == final["turns"]
            assert not snapshot()["pending_requests"]
            assert (root / "prompt-count.txt").read_text() == "[permission]\n[permission-review]\n"
            print(json.dumps({"provider": "private ACP fixture", "request_details_and_long_standalone_content": "PASS", "falsy_input": "PASS", "exact_choices_and_stale_controller_fencing": "PASS", "other_session_and_late_answer_isolation": "PASS", "review_does_not_rewrite_tools_or_execute_operations": "PASS", "archive_without_permission_or_prompt_replay": "PASS"}))
        finally:
            stop()


if __name__ == "__main__":
    main()
