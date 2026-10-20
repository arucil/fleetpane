#!/usr/bin/env python3
"""Real installed Codex request_user_input round trip with a loopback SSE model.

Empty CODEX_HOME, synthetic answers, no credentials or external LLM requests.
The local Responses events follow Codex's own request_user_input integration test.
"""

import json
import os
import signal
import subprocess
import tempfile
import threading
import time
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ANSWER = {
    "answers": {
        "language": {"answers": ["Rust (Recommended)", "user_note: 中文\n第二行"]}
    }
}


class Model(BaseHTTPRequestHandler):
    calls = 0
    verified_answer = False

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        type(self).calls += 1
        if self.calls == 1:
            item = {
                "type": "function_call",
                "call_id": "question-call",
                "name": "request_user_input",
                "arguments": json.dumps(
                    {
                        "questions": [
                            {
                                "id": "language",
                                "header": "Language",
                                "question": "选择语言？",
                                "options": [
                                    {
                                        "label": "Rust (Recommended)",
                                        "description": "Native performance",
                                    },
                                    {"label": "Go", "description": "Fast builds"},
                                ],
                            }
                        ]
                    }
                ),
            }
        else:
            outputs = [
                item for item in body["input"] if item["type"] == "function_call_output"
            ]
            type(self).verified_answer = any(
                item.get("call_id") == "question-call"
                and json.loads(item["output"]) == ANSWER
                for item in outputs
            )
            item = {
                "type": "message",
                "id": "final",
                "role": "assistant",
                "content": [
                    {
                        "type": "output_text",
                        "text": "Answer verified; continued"
                        if self.verified_answer
                        else "Answer mismatch",
                    }
                ],
            }
        events = [
            {"type": "response.created", "response": {"id": f"response-{self.calls}"}},
            {"type": "response.output_item.done", "item": item},
            {
                "type": "response.completed",
                "response": {
                    "id": f"response-{self.calls}",
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
        result = probe()
        if result:
            return result
        time.sleep(0.05)
    raise AssertionError(f"timeout: {description}")


def main():
    binary = Path(__file__).resolve().parents[1] / "target/debug/fleetpane-host"
    server = ThreadingHTTPServer(("127.0.0.1", 0), Model)
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    try:
        with tempfile.TemporaryDirectory(
            prefix="fleetpane-codex-questions-"
        ) as directory:
            root = Path(directory)
            home = root / "codex-home"
            home.mkdir(mode=0o700)
            (home / "config.toml").write_text(f"""model = "fixture"
model_provider = "fixture"
[model_providers.fixture]
name = "Fleetpane loopback question fixture"
base_url = "http://127.0.0.1:{server.server_port}/v1"
wire_api = "responses"
requires_openai_auth = false
request_max_retries = 0
stream_max_retries = 0
""")
            env = {
                key: value
                for key, value in os.environ.items()
                if key not in {"OPENAI_API_KEY", "CODEX_API_KEY", "OPENAI_BASE_URL"}
            }
            env["CODEX_HOME"] = str(home)
            socket = root / "private/control.sock"
            with (root / "host.log").open("w+") as log:
                process = subprocess.Popen(
                    [str(binary), "serve", str(socket), str(root)],
                    env=env,
                    stdout=subprocess.DEVNULL,
                    stderr=log,
                    start_new_session=True,
                )
                try:
                    wait(
                        lambda: (
                            socket.exists()
                            and "ready:" in (root / "host.log").read_text()
                        ),
                        "Host ready",
                    )

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

                    def state():
                        return raw({"command": "snapshot"})

                    def command(*args, response=None):
                        result = subprocess.run(
                            [
                                str(binary),
                                "session",
                                str(socket),
                                "--take-control",
                                *args,
                            ],
                            input=json.dumps(response) if response else None,
                            capture_output=True,
                            text=True,
                            timeout=25,
                            check=False,
                        )
                        assert result.returncode == 0, result.stderr
                        return json.loads(result.stdout)

                    initial = state()
                    thread = command("create", str(root))["result"]["thread"]
                    operation_id = str(uuid.uuid4())
                    raw(
                        {
                            "command": "execute",
                            "lease": state()["owner"],
                            "operation_id": operation_id,
                            "method": "turn/start",
                            "params": {
                                "threadId": thread["id"],
                                "input": [
                                    {"type": "text", "text": "Ask a synthetic question"}
                                ],
                                "collaborationMode": {
                                    "mode": "plan",
                                    "settings": {
                                        "model": "fixture",
                                        "reasoning_effort": "medium",
                                        "developer_instructions": None,
                                    },
                                },
                            },
                        }
                    )
                    pending = wait(
                        lambda: state()["pending_requests"], "real Codex question"
                    )[0]
                    assert pending["method"] == "item/tool/requestUserInput", pending
                    assert pending["params"]["isBlocking"] is True
                    command("list")
                    assert state()["pending_requests"][0] == pending
                    assert command(
                        "answer", json.dumps(pending["id"]), response=ANSWER
                    ) == {"answered": True}

                    def completed():
                        opened = command("open", thread["id"])["result"]["thread"]
                        return (
                            opened
                            if opened["turns"]
                            and opened["turns"][-1]["status"] != "inProgress"
                            else None
                        )

                    opened = wait(completed, "real Codex continues after answer")
                    assert opened["turns"][-1]["status"] == "completed", opened
                    assert any(
                        item.get("text") == "Answer verified; continued"
                        for item in opened["turns"][-1]["items"]
                    ), opened
                    assert Model.calls == 2 and Model.verified_answer
                    final = state()
                    assert not final["pending_requests"]
                    assert (
                        initial["host_id"] == final["host_id"]
                        and initial["provider_pid"] == final["provider_pid"]
                    )
                    print(
                        json.dumps(
                            {
                                "provider": subprocess.check_output(
                                    ["codex", "--version"], text=True
                                ).strip(),
                                "real_codex_question_handoff_answer_continuation": "PASS",
                                "unicode_multiline_answer_readback": "PASS",
                                "local_model_requests": Model.calls,
                                "external_llm_requests": 0,
                                "same_host_and_provider": True,
                            }
                        )
                    )
                finally:
                    if process.poll() is None:
                        os.killpg(process.pid, signal.SIGINT)
                        process.wait(timeout=10)
    finally:
        server.shutdown()
        server.server_close()
        worker.join(timeout=2)


if __name__ == "__main__":
    main()
