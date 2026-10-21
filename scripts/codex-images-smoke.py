#!/usr/bin/env python3
"""Installed Codex image transport with private CODEX_HOME and a loopback model.

No external model, credentials, existing sessions or shared services are used.
"""

import base64
import hashlib
import json
import os
import runpy
import signal
import socket
import subprocess
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import ClassVar

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"
FIXTURE = runpy.run_path(str(ROOT / "scripts/images-smoke.py"))


class Model(BaseHTTPRequestHandler):
    requests: ClassVar[list[list[dict]]] = []

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        images = []
        for item in body["input"]:
            content = item.get("content")
            if not isinstance(content, list):
                continue
            for part in content:
                if part.get("type") == "input_image":
                    header, data = part["image_url"].split(",", 1)
                    decoded = base64.b64decode(data, validate=True)
                    images.append(
                        {
                            "sha256": hashlib.sha256(decoded).hexdigest(),
                            "bytes": len(decoded),
                            "header": header,
                        }
                    )
        type(self).requests.append(images)
        identity = f"response-{len(self.requests)}"
        item = {
            "type": "message",
            "id": f"message-{len(self.requests)}",
            "role": "assistant",
            "content": [
                {
                    "type": "output_text",
                    "text": "Image received by the local transport fixture",
                }
            ],
        }
        events = [
            {"type": "response.created", "response": {"id": identity}},
            {"type": "response.output_item.done", "item": item},
            {
                "type": "response.completed",
                "response": {
                    "id": identity,
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


def main():
    server = ThreadingHTTPServer(("127.0.0.1", 0), Model)
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    process = None
    temporary = None
    try:
        temporary = tempfile.TemporaryDirectory(prefix="fp-codex-images-")
        root = Path(temporary.name)
        home = root / "home"
        home.mkdir(mode=0o700)
        (home / "config.toml").write_text(f"""model = "gpt-6"
model_provider = "fixture"
approval_policy = "never"
sandbox_mode = "workspace-write"
[model_providers.fixture]
name = "Fleetpane image loopback"
base_url = "http://127.0.0.1:{server.server_port}/v1"
wire_api = "responses"
requires_openai_auth = false
request_max_retries = 0
stream_max_retries = 0
""")
        env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith(("OPENAI_", "CODEX_"))
        }
        env["CODEX_HOME"] = str(home)
        control = root / "host/control.sock"
        store = root / "drafts"

        def run(*args):
            result = subprocess.run(
                [str(BINARY), *map(str, args)],
                env=env,
                text=True,
                capture_output=True,
                timeout=25,
                check=False,
            )
            assert result.returncode == 0, (
                result.stdout[-1000:],
                result.stderr[-1000:],
            )
            return json.loads(result.stdout)

        def snapshot():
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(5)
                stream.connect(str(control))
                stream.sendall(b'{"command":"snapshot"}\n')
                result = json.loads(stream.makefile().readline())["data"]
                assert result["fault"] is None, result["fault"]
                return result

        def session(*args):
            return run("session", control, "--take-control", *args)["result"]

        log = root / "host.log"
        with log.open("w") as output:
            process = subprocess.Popen(
                [str(BINARY), "serve", str(control), str(root)],
                env=env,
                stdout=subprocess.DEVNULL,
                stderr=output,
                start_new_session=True,
            )
        FIXTURE["wait"](lambda: control.exists() and "ready:" in log.read_text())
        state = snapshot()
        identity = session("create", root)["thread"]["id"]
        key = json.dumps(
            {
                "socket": str(control),
                "host_id": state["host_id"],
                "thread_id": identity,
                "cwd": str(root),
            }
        )
        originals = []
        for index, large in enumerate([False, True]):
            source = root / f"image-{index}.png"
            data = FIXTURE["png"](1024 if large else 128, 1024 if large else 128, large)
            source.write_bytes(data)
            originals.append(
                {
                    "sha256": hashlib.sha256(data).hexdigest(),
                    "bytes": len(data),
                    "header": "data:image/png;base64",
                }
            )
            draft = run("draft", store, "get", key)
            draft = run("draft", store, "attach", key, draft["revision"], source)
            source.unlink()
            run("draft", store, "send", key, draft["revision"], "--take-control")
            FIXTURE["wait"](
                lambda index=index: (
                    not snapshot()["activity"]["turns"] and len(Model.requests) > index
                )
            )
            assert Model.requests[index] == originals
            assert snapshot()["provider_pid"] == state["provider_pid"]
            draft = run("draft", store, "get", key)
            assert (
                draft["text"] == ""
                and not draft.get("images")
                and draft["attempt"] is None
            )
            history = session("open", identity)["thread"]
            assert all(turn["status"] == "completed" for turn in history["turns"])
            users = [
                item
                for turn in history["turns"]
                for item in turn["items"]
                if item["type"] == "userMessage"
            ]
            assert len(users) == index + 1
            for original, user in zip(originals, users, strict=True):
                [image] = user["content"]
                assert image["type"] == "localImage"
                assert (
                    hashlib.sha256(Path(image["path"]).read_bytes()).hexdigest()
                    == original["sha256"]
                )
            assert len(json.dumps(history)) < 16 * 1024
        assert len(Model.requests) == 2
        print(
            json.dumps(
                {
                    "passed": True,
                    "provider": "installed Codex app-server",
                    "model": "loopback SSE fixture",
                    "requests": Model.requests,
                    "bounded_history": True,
                    "original_sources_removed": True,
                }
            )
        )
    finally:
        if process is not None and process.poll() is None:
            os.killpg(process.pid, signal.SIGINT)
            process.wait(timeout=10)
        server.shutdown()
        server.server_close()
        worker.join(timeout=5)
        if temporary is not None:
            temporary.cleanup()


if __name__ == "__main__":
    main()
