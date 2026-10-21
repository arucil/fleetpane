#!/usr/bin/env python3
"""Shared ACP commands, prompt lifecycle, exact permissions and Host handoff."""

import json
import os
import signal
import socket
import subprocess
import tempfile
import time
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


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
    with tempfile.TemporaryDirectory(prefix="fleetpane-acp-smoke-") as directory:
        root = Path(directory)
        control = root / "private/control.sock"
        log = root / "host.log"

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

        def run(*arguments):
            return subprocess.run(
                [str(binary), "session", str(control), "--take-control", *arguments],
                capture_output=True,
                text=True,
                timeout=12,
                check=False,
            )

        def command(*arguments):
            result = run(*arguments)
            assert result.returncode == 0, result.stderr
            return json.loads(result.stdout).get("result")

        def start():
            output = log.open("w")
            process = subprocess.Popen(
                [
                    str(binary),
                    "serve-acp",
                    str(control),
                    str(root),
                    "python3",
                    str(ROOT / "scripts/acp-fixture.py"),
                ],
                stderr=output,
                stdout=subprocess.DEVNULL,
                start_new_session=True,
            )
            wait(
                lambda: (
                    process.poll() is None
                    and control.exists()
                    and "ready:" in log.read_text()
                ),
                "Host ready",
            )
            output.close()
            return process

        def stop(process):
            os.killpg(process.pid, signal.SIGINT)
            process.wait(timeout=10)
            assert not control.exists(), "owned socket cleanup"

        process = start()
        try:
            state = snapshot()
            assert state["provider_info"]["fleetpaneProvider"] == "deepseek-acp"
            thread = command("create", str(root))["thread"]
            identity = thread["id"]
            assert thread["historyMode"] == "acp-observed"
            assert len(thread["configOptions"]) == 2
            assert command("list")["data"][0]["id"] == identity
            assert (
                command("config", identity, "model", "model-b")["configOptions"][0][
                    "currentValue"
                ]
                == "model-b"
            )
            assert (
                command("config", identity, "reasoning_effort", "high")[
                    "configOptions"
                ][1]["currentValue"]
                == "high"
            )
            assert run("config", identity, "model", "missing").returncode != 0
            assert run("rename", identity, "unsupported").returncode != 0
            before = snapshot()
            send = {
                "command": "execute",
                "lease": before["owner"],
                "operation_id": str(uuid.uuid4()),
                "method": "turn/start",
                "params": {
                    "threadId": identity,
                    "input": [{"type": "text", "text": "你好 ACP"}],
                },
            }
            for _ in range(2):
                accepted = raw(send)
                assert accepted["response"] == "operation", accepted

            def ended():
                opened = command("open", identity)["thread"]
                return (
                    opened
                    if opened["turns"] and opened["turns"][-1]["status"] != "inProgress"
                    else None
                )

            opened = wait(ended, "prompt completes")
            assert len(opened["turns"]) == 1
            items = opened["turns"][0]["items"]
            message = next(i for i in items if i["type"] == "acpMessage")
            assert message["content"] == [
                {"type": "text", "text": "**ACP** Echo: 你好 ACP"}
            ]
            tool = next(i for i in items if i["type"] == "acpToolCall")
            assert (
                tool["call"]["title"] == "Synthetic tool"
                and tool["call"]["status"] == "completed"
            )
            assert (root / "prompt-count.txt").read_text() == "你好 ACP\n"
            command("send", identity, "[permission]")
            pending = wait(lambda: snapshot()["pending_requests"], "permission")
            assert (
                len(pending) == 1
                and pending[0]["method"] == "session/request_permission"
            )
            old = snapshot()["owner"]
            command(
                "list"
            )  # Explicit controller replacement leaves the request unchanged.
            current = snapshot()
            assert current["pending_requests"] == pending
            stale = raw(
                {
                    "command": "respond",
                    "lease": old,
                    "request_id": pending[0]["id"],
                    "result": {
                        "outcome": {"outcome": "selected", "optionId": "allow-one"}
                    },
                }
            )
            assert stale["data"]["code"] == "stale_controller", stale
            invalid = raw(
                {
                    "command": "respond",
                    "lease": current["owner"],
                    "request_id": pending[0]["id"],
                    "result": {
                        "outcome": {"outcome": "selected", "optionId": "invented"}
                    },
                }
            )
            assert invalid["data"]["code"] == "invalid_permission", invalid
            command("permission", json.dumps(pending[0]["id"]), "reject-one")
            wait(ended, "permission settles")
            assert not snapshot()["pending_requests"]
            saved = json.loads((root / "fixture-acp-sessions.json").read_text())
            assert saved[identity]["permission"]["outcome"]["optionId"] == "reject-one"
            command("send", identity, "[slow]")
            slow = command("open", identity)["thread"]["turns"][-1]["id"]
            command("stop", identity, slow)
            assert wait(ended, "cancel settles")["turns"][-1]["status"] == "interrupted"
            command("send", identity, "[fail]")
            assert wait(ended, "failure settles")["turns"][-1]["status"] == "failed"
            assert snapshot()["provider_pid"] == state["provider_pid"]
            assert snapshot()["host_pid"] == state["host_pid"]
            before_restart = command("open", identity)["thread"]
            before_prompts = (root / "prompt-count.txt").read_text()
        finally:
            stop(process)
        process = start()
        try:
            listed = command("list")["data"]
            assert (
                listed[0]["id"] == identity
                and listed[0]["status"]["type"] == "archived"
            )
            archived = command("open", identity)["thread"]
            assert archived["status"]["type"] == "archived"
            assert archived["turns"] == before_restart["turns"]
            assert run("send", identity, "must not run before resume").returncode != 0
            assert (root / "prompt-count.txt").read_text() == before_prompts
            resumed = command("inspect", identity, "--resume")["thread"]
            assert resumed["turns"] == [] and resumed["historyMode"] == "acp-observed"
            assert resumed["configOptions"][0]["currentValue"] == "model-b"
            assert command("open", identity)["thread"]["turns"] == before_restart["turns"]
            command("send", identity, "after resume")
            assert len(wait(ended, "resumed prompt")["turns"]) == len(before_restart["turns"]) + 1
        finally:
            stop(process)
        print(
            json.dumps(
                {
                    "provider": "synthetic-acp-v1",
                    "shared_commands_stream_tool_config": "PASS",
                    "exact_permission_handoff_fencing": "PASS",
                    "cancel_failure_and_resume_preserve_observed_history": "PASS",
                    "same_prompt_id_executes_once": True,
                }
            )
        )


if __name__ == "__main__":
    main()
