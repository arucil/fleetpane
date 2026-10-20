#!/usr/bin/env python3
"""Real Codex code-mode command history across reload and provider restart.

An isolated CODEX_HOME and loopback SSE model execute one synthetic command.
No credentials, external model requests, or existing sessions are used.
"""

import json
import os
import signal
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import ClassVar

NONCE = "FP-PERSIST-7C42"
SCRIPT = "cat QA_INPUT.txt; printf 'run\\n' >> tool-runs.txt"


class Model(BaseHTTPRequestHandler):
    calls = 0
    outputs: ClassVar[list[str]] = []

    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        type(self).calls += 1
        outputs = [
            item
            for item in request["input"]
            if item["type"] == "custom_tool_call_output"
        ]
        if outputs:
            type(self).outputs.extend(json.dumps(item["output"]) for item in outputs)
            item = {
                "type": "message",
                "id": "history-final",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "History tool finished"}],
            }
        else:
            item = {
                "type": "custom_tool_call",
                "id": "history-exec",
                "call_id": "history-exec-call",
                "name": "exec",
                "input": "text(await tools.exec_command("
                + json.dumps({"cmd": SCRIPT, "max_output_tokens": 1000})
                + "));",
            }
        events = [
            {"type": "response.created", "response": {"id": f"r-{self.calls}"}},
            {"type": "response.output_item.done", "item": item},
            {
                "type": "response.completed",
                "response": {
                    "id": f"r-{self.calls}",
                    "usage": {"input_tokens": 0, "output_tokens": 0, "total_tokens": 0},
                },
            },
        ]
        encoded = "".join(
            f"event: {event['type']}\ndata: {json.dumps(event)}\n\n" for event in events
        ).encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def log_message(self, *_args):
        pass


def wait(probe, description):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        value = probe()
        if value:
            return value
        time.sleep(0.05)
    raise AssertionError(f"timeout: {description}")


def main():
    binary = Path(__file__).resolve().parents[1] / "target/debug/fleetpane-host"
    server = ThreadingHTTPServer(("127.0.0.1", 0), Model)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        with tempfile.TemporaryDirectory(
            prefix="fleetpane-codex-history-"
        ) as directory:
            root = Path(directory)
            home = root / "home"
            home.mkdir(mode=0o700)
            (root / "QA_INPUT.txt").write_text(NONCE + "\n")
            (home / "config.toml").write_text(f"""model = "fixture"
model_provider = "fixture"
approval_policy = "never"
sandbox_mode = "workspace-write"
[features]
code_mode = true
code_mode_only = true
[model_providers.fixture]
name = "Fleetpane loopback history fixture"
base_url = "http://127.0.0.1:{server.server_port}/v1"
wire_api = "responses"
requires_openai_auth = false
request_max_retries = 0
stream_max_retries = 0
""")
            env = {
                key: value
                for key, value in os.environ.items()
                if key
                not in {
                    "OPENAI_API_KEY",
                    "CODEX_API_KEY",
                    "OPENAI_BASE_URL",
                    "CODEX_THREAD_ID",
                    "CODEX_SESSION_ID",
                    "CODEX_TOOL_CALL_ID",
                }
            }
            env["CODEX_HOME"] = str(home)
            socket = root / "private/control.sock"

            def raw(payload):
                result = subprocess.run(
                    [str(binary), "request", str(socket)],
                    input=json.dumps(payload),
                    capture_output=True,
                    text=True,
                    timeout=8,
                    check=False,
                )
                assert result.returncode == 0, (result.stdout, result.stderr)
                return json.loads(result.stdout)["data"]

            def command(*args):
                result = subprocess.run(
                    [str(binary), "session", str(socket), "--take-control", *args],
                    capture_output=True,
                    text=True,
                    timeout=25,
                    check=False,
                )
                assert result.returncode == 0, result.stderr
                return json.loads(result.stdout)["result"]

            saved_command = None
            first = None
            thread_id = None
            for generation in range(2):
                log_path = root / f"host-{generation}.log"
                with log_path.open("w") as log:
                    process = subprocess.Popen(
                        [str(binary), "serve", str(socket), str(root)],
                        env=env,
                        stdout=subprocess.DEVNULL,
                        stderr=log,
                        start_new_session=True,
                    )
                    try:
                        wait(
                            lambda log_path=log_path: (
                                socket.exists() and "ready:" in log_path.read_text()
                            ),
                            "Host ready",
                        )
                        state = raw({"command": "snapshot"})
                        if generation == 0:
                            first = state
                            thread = command("create", str(root))["thread"]
                            thread_id = thread["id"]
                            command(
                                "send", thread_id, "Run the synthetic history command"
                            )
                            cursor = state["sequence"]
                            seen = []

                            def completed(host_id=state["host_id"], seen=seen):
                                nonlocal cursor
                                events = raw(
                                    {
                                        "command": "events",
                                        "host_id": host_id,
                                        "after": cursor,
                                    }
                                )
                                seen.extend(events)
                                if events:
                                    cursor = events[-1]["sequence"]
                                return any(
                                    event["message"].get("method") == "turn/completed"
                                    for event in events
                                )

                            wait(completed, "real Codex turn completion")
                            saved_command = next(
                                event["message"]["params"]["item"]
                                for event in seen
                                if event["message"].get("method") == "item/completed"
                                and event["message"]
                                .get("params", {})
                                .get("item", {})
                                .get("type")
                                == "commandExecution"
                            )
                            assert saved_command["exitCode"] == 0, saved_command
                            assert NONCE in saved_command["aggregatedOutput"]
                        else:
                            assert state["host_id"] != first["host_id"]
                            assert state["provider_pid"] != first["provider_pid"]
                            assert state["operation_ids"] == []
                            startup = raw(
                                {
                                    "command": "events",
                                    "host_id": state["host_id"],
                                    "after": 0,
                                }
                            )
                            assert not any(
                                event["message"]
                                .get("params", {})
                                .get("item", {})
                                .get("id")
                                == saved_command["id"]
                                for event in startup
                            ), (
                                "new Host must not have the old command in its replay buffer"
                            )
                        opened = command("open", thread_id)["thread"]
                        assert opened["turns"][0]["status"] == "completed", opened
                        commands = [
                            item
                            for turn in opened["turns"]
                            for item in turn["items"]
                            if item["type"] == "commandExecution"
                        ]
                        assert len(commands) == 1, (
                            "history lost or duplicated nested command"
                        )
                        assert commands[0] == saved_command, (
                            commands[0],
                            saved_command,
                        )
                        assert opened["historyMode"] == "paginated"
                        metadata_args = ["inspect", thread_id]
                        if generation == 1:
                            metadata_args.append("--resume")
                        assert command(*metadata_args)["thread"]["turns"] == []
                        turns = command("history-turns", thread_id)
                        assert len(turns["data"]) == 1 and not turns["data"][0]["items"]
                        paged = []
                        page_args = ["history-items", thread_id, turns["data"][0]["id"]]
                        while True:
                            page = command(*page_args)
                            paged.extend(entry["item"] for entry in page["data"])
                            if page["nextCursor"] is None:
                                break
                            page_args = [*page_args[:3], "--cursor", page["nextCursor"]]
                        assert [
                            item for item in paged if item["type"] == "commandExecution"
                        ] == [saved_command]
                        assert (root / "tool-runs.txt").read_text() == "run\n"
                        assert Model.calls == 2 and any(
                            NONCE in output for output in Model.outputs
                        )
                        if generation == 1:
                            before_reads = raw({"command": "snapshot"})
                            reads = [
                                ("list",),
                                ("models",),
                                ("inspect", thread_id),
                                ("history-turns", thread_id),
                                ("history-items", thread_id, turns["data"][0]["id"]),
                            ]
                            for index in range(160):
                                command(*reads[index % len(reads)])
                            after_reads = raw({"command": "snapshot"})
                            assert (
                                after_reads["operation_ids"]
                                == before_reads["operation_ids"]
                            )
                            command("rename", thread_id, "Synthetic query verification")
                            assert (
                                command("inspect", thread_id)["thread"]["name"]
                                == "Synthetic query verification"
                            )
                            assert (
                                len(raw({"command": "snapshot"})["operation_ids"])
                                == len(before_reads["operation_ids"]) + 1
                            )
                            assert Model.calls == 2
                    finally:
                        if process.poll() is None:
                            os.killpg(process.pid, signal.SIGINT)
                            process.wait(timeout=10)
            print(
                json.dumps(
                    {
                        "provider": subprocess.check_output(
                            ["codex", "--version"], text=True
                        ).strip(),
                        "nested_command_reload_and_provider_restart": "PASS",
                        "stable_item_output_exit_status": "PASS",
                        "metadata_resume_and_paged_command_identity": "PASS",
                        "reads_before_another_mutation": 160,
                        "command_executions": 1,
                        "local_model_requests": Model.calls,
                        "real_llm_turn": False,
                    }
                )
            )
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
