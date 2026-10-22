#!/usr/bin/env python3
"""Exact-turn CLI observations with owned Codex/ACP fixtures and optional real Codex."""
import argparse
import json
import os
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"


def until(probe):
    end = time.monotonic() + 15
    while time.monotonic() < end:
        if value := probe():
            return value
        time.sleep(0.025)
    raise AssertionError("owned wait fixture deadline")


def raw(endpoint, request):
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(5)
        stream.connect(str(endpoint))
        stream.sendall(json.dumps(request).encode() + b"\n")
        return json.loads(stream.makefile("rb").readline())


def run(*args, ok=True):
    result = subprocess.run([str(BINARY), *map(str, args)], text=True,
                            capture_output=True, timeout=20, check=False)
    assert (result.returncode == 0) == ok, (args, result.stdout[-1000:], result.stderr[-1000:])
    return json.loads(result.stdout) if ok else result.stderr


def snap(endpoint):
    return raw(endpoint, {"command": "snapshot"})["data"]


def session(endpoint, *args):
    return run("session", endpoint, "--take-control", *args)["result"]


def observe(endpoint, thread, turn, timeout=5000, ok=True):
    return run("session", endpoint, "wait", thread, turn, "--timeout-ms", timeout, ok=ok)


def unchanged(before, after):
    for key in ("host_id", "host_pid", "provider_pid", "owner", "journal_total", "operation_ids", "activity", "pending_requests", "admission"):
        assert before[key] == after[key], key


class Model(BaseHTTPRequestHandler):
    entered = threading.Event()
    release = threading.Event()
    calls = 0

    def do_POST(self):
        json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        type(self).calls += 1
        self.entered.set()
        assert self.release.wait(15), "loopback model completion gate"
        events = [
            {"type": "response.created", "response": {"id": "wait-response"}},
            {"type": "response.output_item.done", "item": {"type": "message", "id": "wait-message", "role": "assistant", "content": [{"type": "output_text", "text": "Wait transport verified."}]}},
            {"type": "response.completed", "response": {"id": "wait-response", "usage": {"input_tokens": 0, "output_tokens": 0, "total_tokens": 0}}},
        ]
        data = "".join(f"event: {e['type']}\ndata: {json.dumps(e)}\n\n" for e in events).encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *_args):
        pass


def check(root, real_codex):
    processes = []
    providers = set()
    observers = []
    evidence = {}
    server = None

    def start(name, acp=False, env=None, real=False):
        home = root / name
        home.mkdir(exist_ok=True)
        endpoint = home / "host/control.sock"
        log = home / f"host-{len(processes)}.log"
        command = [str(BINARY), "serve-acp" if acp else "serve", str(endpoint), str(home)]
        if not real:
            command += [sys.executable, str(ROOT / "scripts" / ("acp-fixture.py" if acp else "provider-fixture.py"))]
        with log.open("w") as output:
            process = subprocess.Popen(command, env=env, stdout=subprocess.DEVNULL,
                                       stderr=output, start_new_session=True)
        processes.append(process)
        until(lambda: process.poll() is not None or (endpoint.exists() and "ready:" in log.read_text()))
        assert process.poll() is None, log.read_text()
        providers.add(snap(endpoint)["provider_pid"])
        return endpoint, process

    def create(endpoint):
        return session(endpoint, "create", str(root))["thread"]["id"]

    def send(endpoint, thread, text):
        return session(endpoint, "send", thread, text)["turn"]["id"]

    def watcher(endpoint, thread, turn):
        process = subprocess.Popen([str(BINARY), "session", str(endpoint), "wait", thread, turn, "--timeout-ms", "5000"],
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        observers.append(process)
        # An active observer need not publish any query reply or advance the
        # Host sequence. Assert that it remains pending before changing the turn.
        try:
            output, error = process.communicate(timeout=0.1)
        except subprocess.TimeoutExpired:
            return process
        raise AssertionError(("wait finished before the gated turn", output, error))

    try:
        codex, _ = start("codex")
        unowned = snap(codex)
        assert "existing controller" in observe(codex, "missing", "missing", ok=False)
        unchanged(unowned, snap(codex))
        thread = create(codex)
        first = send(codex, thread, "Exact wait result")
        result = observe(codex, thread, first)
        assert result["outcome"] == "finished" and result["turn"]["status"] == "completed", result
        before = snap(codex)
        repeated = observe(codex, thread, first)
        for key in ("host_id", "thread_id", "turn_id", "outcome", "turn"):
            assert repeated[key] == result[key], key
        unchanged(before, snap(codex))
        other = create(codex)
        question_turn = send(codex, other, "[questions]")
        until(lambda: snap(codex)["pending_requests"])
        held = send(codex, thread, "[wait]")
        before = snap(codex)
        assert observe(codex, thread, held, 75)["outcome"] == "timed_out"
        unchanged(before, snap(codex))
        attention = observe(codex, other, question_turn)
        assert attention["outcome"] == "attention" and attention["requests"] == before["pending_requests"]
        unchanged(before, snap(codex))
        assert "read-only" in run("session", codex, "--take-control", "wait", thread, held, ok=False)
        assert "timeout" in observe(codex, thread, held, 0, ok=False)
        assert "not found" in observe(codex, thread, "missing-turn", ok=False)
        unchanged(before, snap(codex))
        cancelled = watcher(codex, thread, held)
        cancelled.send_signal(signal.SIGINT)
        cancelled.communicate(timeout=5)
        assert cancelled.returncode == -signal.SIGINT
        unchanged(before, snap(codex))
        following = watcher(codex, thread, held)
        session(codex, "stop", thread, held)
        output, error = following.communicate(timeout=10)
        assert following.returncode == 0, error
        stopped = json.loads(output)
        assert stopped["outcome"] == "finished" and stopped["turn"]["status"] == "interrupted", stopped
        after_stop = snap(codex)
        assert after_stop["owner"] != before["owner"]
        assert after_stop["pending_requests"] == before["pending_requests"]
        assert after_stop["provider_pid"] == before["provider_pid"]
        failed_turn = send(codex, thread, "[fail]")
        failed = observe(codex, thread, failed_turn)
        assert failed["outcome"] == "finished" and failed["turn"]["status"] == "failed" and failed["turn"]["error"]
        # Initial lookup traverses newer turns without confusing their outcome.
        assert observe(codex, thread, first)["turn"]["id"] == first
        doomed = send(codex, thread, "[wait]")
        dying = watcher(codex, thread, doomed)
        provider = snap(codex)["provider_pid"]
        os.kill(provider, signal.SIGKILL)
        output, error = dying.communicate(timeout=10)
        assert dying.returncode != 0 and not output, (output, error)
        evidence["codex_fixture"] = {"completed": result, "attention": attention, "interrupted": stopped,
            "failed": failed, "before_wait": before, "after_stop": after_stop, "provider_loss_error": error}

        acp, acp_process = start("acp", acp=True)
        identity = create(acp)
        separate = create(acp)
        send(acp, separate, "[permission]")
        pending = until(lambda: snap(acp)["pending_requests"])
        refusal = send(acp, identity, "[stop:refusal]")
        refused = observe(acp, identity, refusal)
        assert refused["outcome"] == "finished" and refused["turn"]["stopReason"] == "refusal", refused
        permission_turn = send(acp, identity, "[permission]")
        until(lambda: len(snap(acp)["pending_requests"]) == 2)
        before = snap(acp)
        permission = observe(acp, identity, permission_turn)
        assert permission["outcome"] == "attention" and len(permission["requests"]) == 1
        assert permission["requests"][0]["params"]["threadId"] == identity
        unchanged(before, snap(acp))
        session(acp, "stop", identity, permission_turn)
        until(lambda: len(snap(acp)["pending_requests"]) == 1)
        assert snap(acp)["pending_requests"] == pending
        (root / "acp/fixture-settlement.txt").unlink(missing_ok=True)
        unfinished = send(acp, identity, "[settlement-gate]")
        observer = watcher(acp, identity, unfinished)
        os.killpg(acp_process.pid, signal.SIGKILL)
        acp_process.wait(timeout=5)
        output, error = observer.communicate(timeout=10)
        assert observer.returncode != 0 and not output, (output, error)
        acp, _ = start("acp", acp=True)
        session(acp, "list")  # Explicit new controller; no context resume.
        before_restart_read = snap(acp)
        unknown = observe(acp, identity, unfinished)
        assert unknown["outcome"] == "unknown" and unknown["turn"]["status"] == "unknown"
        unchanged(before_restart_read, snap(acp))
        evidence["acp_fixture"] = {"refusal": refused, "permission": permission, "unknown_after_restart": unknown,
            "host_loss_error": error, "before_wait": before, "restart_read": before_restart_read}

        if real_codex:
            server = ThreadingHTTPServer(("127.0.0.1", 0), Model)
            worker = threading.Thread(target=server.serve_forever, daemon=True)
            worker.start()
            home = root / "real-codex-home"
            home.mkdir(mode=0o700)
            (home / "config.toml").write_text(f'''model = "gpt-6"
model_provider = "fixture"
approval_policy = "never"
sandbox_mode = "workspace-write"
[model_providers.fixture]
name = "Fleetpane wait loopback"
base_url = "http://127.0.0.1:{server.server_port}/v1"
wire_api = "responses"
requires_openai_auth = false
request_max_retries = 0
stream_max_retries = 0
''')
            env = {k: v for k, v in os.environ.items() if not k.startswith(("OPENAI_", "CODEX_"))}
            env["CODEX_HOME"] = str(home)
            actual, _ = start("real", env=env, real=True)
            live = create(actual)
            turn = send(actual, live, "Reply with the loopback fixture result; do not use tools.")
            before_real = snap(actual)
            evidence["installed_codex_initial"] = {"snapshot": before_real,
                "events": raw(actual, {"command": "events", "host_id": before_real["host_id"], "after": 0})}
            assert {"thread_id": live, "turn_id": turn} in before_real["activity"]["turns"]
            immediate = observe(actual, live, turn, 75)
            assert immediate["outcome"] == "timed_out", immediate
            unchanged(before_real, snap(actual))
            # First-turn waiting must work even before Codex starts its HTTP
            # request and materializes the paginated history.
            assert Model.entered.wait(10)
            watching = watcher(actual, live, turn)
            Model.release.set()
            output, error = watching.communicate(timeout=12)
            assert watching.returncode == 0, error
            real_result = json.loads(output)
            assert real_result["outcome"] == "finished" and real_result["turn"]["status"] == "completed", real_result
            settled = observe(actual, live, turn)
            assert settled["outcome"] == "finished" and settled["turn"]["id"] == turn, settled
            assert Model.calls == 1
            after_real = snap(actual)
            for key in ("host_id", "host_pid", "provider_pid", "owner", "journal_total", "operation_ids"):
                assert before_real[key] == after_real[key], key
            evidence["installed_codex"] = {"provider": subprocess.check_output(["codex", "--version"], env=env, text=True).strip(),
                "model": "one gated loopback SSE response; no external model", "immediate": immediate,
                "result": real_result, "settled_history": settled,
                "before": before_real, "after": after_real}
    finally:
        Model.release.set()
        for process in observers:
            if process.poll() is None:
                process.terminate()
                process.communicate(timeout=5)
        cleanup = []
        for process in reversed(processes):
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGINT)
                process.wait(timeout=10)
            cleanup.append({"host_pid": process.pid, "exit_code": process.returncode})
        evidence["owned_cleanup"] = cleanup
        for provider in providers:
            stat = Path("/proc") / str(provider) / "stat"
            assert not stat.exists() or stat.read_text().split(") ")[1].startswith("Z "), provider
        evidence["providers_gone_or_zombie"] = sorted(providers)
        if server is not None:
            server.shutdown()
            server.server_close()
        (root / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
    print(json.dumps({"wait_smoke": "pass", "providers": [k for k in evidence if k not in {"owned_cleanup", "providers_gone_or_zombie"}], "evidence": str(root / "evidence.json")}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-dir", type=Path)
    parser.add_argument("--real-codex", action="store_true")
    args = parser.parse_args()
    if args.evidence_dir:
        args.evidence_dir.mkdir(parents=True, exist_ok=False)
        check(args.evidence_dir.resolve(), args.real_codex)
    else:
        with tempfile.TemporaryDirectory(prefix="fleetpane-wait-") as directory:
            check(Path(directory), args.real_codex)


if __name__ == "__main__":
    main()
