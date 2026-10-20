#!/usr/bin/env python3
"""Optional real Codex history/error check against a local rejecting HTTP fixture.

Uses an empty CODEX_HOME and a loopback model provider. No account or LLM calls.
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


class RejectingProvider(BaseHTTPRequestHandler):
    calls = 0
    requests: ClassVar[list[dict]] = []

    def do_POST(self):
        type(self).requests.append(
            json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        )
        type(self).calls += 1
        body = json.dumps(
            {
                "error": {
                    "message": "Fleetpane synthetic rejection",
                    "type": "invalid_request_error",
                    "code": "fixture_rejection",
                }
            }
        ).encode()
        self.send_response(400)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_args):
        pass


def main():
    binary = Path(__file__).resolve().parents[1] / "target/debug/fleetpane-host"
    server = ThreadingHTTPServer(("127.0.0.1", 0), RejectingProvider)
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    try:
        with tempfile.TemporaryDirectory(
            prefix="fleetpane-codex-session-"
        ) as directory:
            root = Path(directory)
            home = root / "codex-home"
            home.mkdir(mode=0o700)
            (home / "config.toml").write_text(f"""model = "fixture"
model_provider = "fixture"
[model_providers.fixture]
name = "Fleetpane synthetic rejecting provider"
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
                    deadline = time.monotonic() + 20
                    while "ready:" not in (root / "host.log").read_text():
                        assert process.poll() is None, (root / "host.log").read_text()
                        assert time.monotonic() < deadline, "Host initialize timeout"
                        time.sleep(0.05)

                    def command(*args):
                        reply = subprocess.run(
                            [
                                str(binary),
                                "session",
                                str(socket),
                                "--take-control",
                                *args,
                            ],
                            capture_output=True,
                            text=True,
                            timeout=25,
                            check=False,
                        )
                        assert reply.returncode == 0, reply.stderr
                        return json.loads(reply.stdout)["result"]

                    thread = command("create", str(root))["thread"]
                    assert thread["historyMode"] == "paginated"
                    command(
                        "send", thread["id"], "Synthetic local-provider failure check"
                    )
                    # A failed turn still materializes history; do not read before it exists.
                    deadline = time.monotonic() + 15
                    while not RejectingProvider.calls:
                        assert time.monotonic() < deadline, (
                            "local provider did not receive the turn"
                        )
                        time.sleep(0.05)
                    while True:
                        opened = command("open", thread["id"])["thread"]
                        if (
                            opened["turns"]
                            and opened["turns"][-1]["status"] == "failed"
                        ):
                            break
                        assert time.monotonic() < deadline, "failed turn not persisted"
                        time.sleep(0.05)
                    assert opened["turns"][-1]["error"] is not None
                    assert any(
                        item["type"] == "userMessage"
                        for item in opened["turns"][-1]["items"]
                    )
                    assert any(
                        item["id"] == thread["id"] for item in command("list")["data"]
                    )
                    command("rename", thread["id"], "  Fleetpane 中文目录验证  ")
                    renamed = command("open", thread["id"])["thread"]
                    assert renamed["name"] == "Fleetpane 中文目录验证", renamed
                    filtered = command(
                        "list", "--search", "中文目录", "--cwd", str(root)
                    )
                    assert [entry["id"] for entry in filtered["data"]] == [
                        thread["id"]
                    ], filtered
                    assert command("list", "--search", "missing-title")["data"] == []
                    assert command("list", "--cwd", str(root / "other"))["data"] == []
                    page = command("models")
                    models = page["data"]
                    while page["nextCursor"]:
                        page = command("models", "--cursor", page["nextCursor"])
                        models.extend(page["data"])
                    assert models and all(not model["hidden"] for model in models), (
                        models
                    )
                    selected = next(
                        model for model in models if model["supportedReasoningEfforts"]
                    )
                    effort = selected["defaultReasoningEffort"]
                    command(
                        "send",
                        thread["id"],
                        "Synthetic model option check",
                        "--model",
                        selected["model"],
                        "--effort",
                        effort,
                    )
                    deadline = time.monotonic() + 15
                    while True:
                        opened = command("open", thread["id"])["thread"]
                        if (
                            len(opened["turns"]) == 2
                            and opened["turns"][-1]["status"] == "failed"
                        ):
                            break
                        assert time.monotonic() < deadline, (
                            "model check turn not persisted"
                        )
                        time.sleep(0.05)
                    request = RejectingProvider.requests[-1]
                    assert request["model"] == selected["model"], request
                    assert request["reasoning"]["effort"] == effort, request
                    assert RejectingProvider.calls == 2, (
                        "fixture rejects without automatic HTTP replay"
                    )
                    print(
                        json.dumps(
                            {
                                "provider": subprocess.check_output(
                                    ["codex", "--version"], text=True
                                ).strip(),
                                "historyMode": "paginated",
                                "create_send_failed_read_list": "PASS",
                                "rename_global_title_workspace_filters": "PASS",
                                "model_catalog_explicit_model_effort_http_readback": "PASS",
                                "local_model_requests": RejectingProvider.calls,
                                "real_llm_turn": False,
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
