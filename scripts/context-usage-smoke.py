#!/usr/bin/env python3
"""Context usage over real private Host/provider processes, synthetic only."""
import argparse
import hashlib
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
BINARY = ROOT / "target/debug/fleetpane-host"


def codex(identity, used, capacity=100, omit_capacity=False):
    def breakdown(n):
        return {"totalTokens": n, "inputTokens": 1, "outputTokens": 0,
                "cachedInputTokens": 0, "reasoningOutputTokens": 0}
    usage = {"last": breakdown(used), "total": breakdown(999999), "future": {"opaque": [False, None, "中文\n"]}}
    if not omit_capacity:
        usage["modelContextWindow"] = capacity
    return {"method": "thread/tokenUsage/updated", "params": {"threadId": identity, "turnId": "observed-turn", "tokenUsage": usage}}


def acp(identity, used, capacity=100):
    return {"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": identity, "update": {
        "sessionUpdate": "usage_update", "used": used, "size": capacity,
        "cost": {"amount": 0.125, "currency": "USD"}, "_meta": {"opaque": [False, None, "中文\n"]}}}}


def wait(probe):
    end = time.monotonic() + 12
    while time.monotonic() < end:
        if value := probe():
            return value
        time.sleep(0.025)
    raise AssertionError("context fixture deadline")


def verify(root):
    receipt = {"synthetic_only": True, "host_sha256": hashlib.sha256(BINARY.read_bytes()).hexdigest(), "providers": [], "cleanup": []}
    for provider, event in (("codex", codex), ("acp", acp)):
        home = root / provider
        home.mkdir(mode=0o700)
        control = home / "host/control.sock"
        inbox = home / "context-inbox.jsonl"
        inbox.touch()
        wire = home / "provider-wire.jsonl"
        process = None
        generation = 0

        def raw(value):
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(5)
                stream.connect(str(control))
                stream.sendall((json.dumps(value) + "\n").encode())
                return json.loads(stream.makefile("rb").readline(1024 * 1024))

        def snapshot():
            response = raw({"command": "snapshot"})
            assert response["response"] == "snapshot", response
            return response["data"]

        def claim():
            s = snapshot()
            response = raw({"command": "claim", "protocol": s["protocol"], "host_id": s["host_id"],
                            "expected_generation": s["owner"]["generation"] if s["owner"] else 0,
                            "controller_id": str(uuid.uuid4())})
            assert response["response"] == "lease", response
            return response["data"]

        def query(identity, lease=None, expect=True):
            s = snapshot()
            response = raw({"command": "query", "lease": lease or s["owner"],
                            "method": "fleetpane/contextUsage/read", "params": {"threadId": identity}})
            if not expect:
                return response
            assert response["response"] == "query", response
            view = response["data"]["message"]["result"]
            assert view["sequence"] == response["data"]["sequence"]
            return view

        def cli(*args):
            result = subprocess.run([str(BINARY), "session", str(control), "--take-control", *args],
                                    text=True, capture_output=True, timeout=12)
            assert result.returncode == 0, result.stderr
            return json.loads(result.stdout)["result"]

        def inject(command):
            with inbox.open("a") as output:
                output.write(json.dumps(command, ensure_ascii=False) + "\n")

        def observe(frame):
            identity = frame["params"].get("threadId", frame["params"].get("sessionId"))
            before_ms = time.time_ns() // 1_000_000
            inject({"emit": frame})
            view = wait(lambda: (v if (v := query(identity))["observation"] and v["observation"]["raw"] == frame else None))
            assert before_ms <= view["observation"]["observedAtUnixMs"] <= time.time_ns() // 1_000_000
            return view

        def start():
            nonlocal process, generation
            generation += 1
            log = home / f"host-{generation}.log"
            with log.open("w") as output:
                process = subprocess.Popen([str(BINARY), "serve-acp" if provider == "acp" else "serve", str(control), str(home),
                    "python3", str(ROOT / "scripts/context-usage-fixture.py"), "--provider", provider,
                    "--inbox", str(inbox), "--wire-log", str(wire)],
                    stdout=subprocess.DEVNULL, stderr=output, start_new_session=True)
            wait(lambda: process.poll() is not None or (control.exists() and "ready:" in log.read_text()))
            assert process.poll() is None, log.read_text()

        def stop():
            if process is not None and process.poll() is None:
                os.killpg(process.pid, signal.SIGINT)
                process.wait(timeout=10)
                receipt["cleanup"].append({"pid": process.pid, "exit_code": process.returncode})

        try:
            start()
            one = cli("create", str(home))["thread"]["id"]
            other = cli("create", str(home))["thread"]["id"]
            assert query(one)["observation"] is None
            assert query(other)["observation"] is None
            v = observe(event(one, 125))
            assert v["observation"]["usedTokens"] == 125
            assert v["observation"]["capacityTokens"] == 100
            assert query(other)["observation"] is None
            before = snapshot()
            wire_before = wire.read_bytes()
            for _ in range(4):
                assert query(one) == v
            assert snapshot() == before
            assert wire.read_bytes() == wire_before  # shared query does not reach provider
            old_lease = snapshot()["owner"]
            claim()
            assert query(one) == v
            assert query(one, old_lease, expect=False)["data"]["code"] == "stale_controller"
            assert cli("open", one)["thread"]["turns"] == []
            assert query(one)["observation"] == v["observation"]
            # Usage does not settle or create activity. Start stays active across telemetry.
            cli("send", one, "Synthetic held prompt")
            wait(lambda: snapshot()["activity"]["turns"])
            assert query(one)["observation"] == v["observation"]
            v = observe(event(one, 0, 0))
            assert v["observation"]["usedTokens"] == 0 and v["observation"]["capacityTokens"] == 0
            assert snapshot()["activity"]["turns"]
            inject({"settle": one})
            wait(lambda: not snapshot()["activity"]["turns"])
            assert query(one)["observation"] == v["observation"]
            if provider == "codex":
                for frame in (codex(one, 7, None), codex(one, 0, omit_capacity=True)):
                    v = observe(frame)
                    assert v["observation"]["capacityTokens"] is None
            else:
                v = observe(acp(one, 18446744073709551615, 18446744073709551615))
                assert v["observation"]["usedTokens"] == 18446744073709551615
            # Expire the event ring using valid unchanged usage for the other thread.
            expired = snapshot()["sequence"]
            for i in range(140):
                inject({"emit": event(other, i, 100)})
            wait(lambda: snapshot()["sequence"] >= expired + 140)
            assert query(one)["observation"] == v["observation"]
            evicted = raw({"command": "events", "host_id": snapshot()["host_id"], "after": 0})
            assert evicted["response"] == "error"
            (home / "latest-observation.json").write_text(json.dumps(v, ensure_ascii=False, indent=2))
            # Host restart explicitly differentiates ACP archive retention and Codex in-memory retention.
            stop()
            inbox.write_text("")
            start()
            claim()
            if provider == "acp":
                assert query(one)["observation"] == v["observation"]
                assert cli("open", one)["thread"]["status"]["type"] == "archived"
                cli("inspect", one, "--resume")
                assert query(one)["observation"] == v["observation"]
            else:
                assert query(one)["observation"] is None
            good = observe(event(one, 23, 100))
            bad = event(one, None, 100)
            inject({"emit": bad})
            fault = wait(lambda: snapshot()["fault"])
            assert "context usage" in fault, fault
            assert snapshot()["fault_scope"] == "provider"
            assert query(one)["observation"] == good["observation"]
            for payload in ({"threadId": ""}, {"threadId": one, "extra": True}):
                response = raw({"command": "query", "lease": snapshot()["owner"], "method": "fleetpane/contextUsage/read", "params": payload})
                assert response["response"] == "error" and response["data"]["code"] == "invalid_context_usage_read", response
            receipt["providers"].append({"provider": provider, "unknown_zero_null_overcapacity_exact_raw": True,
                "other_thread_and_atomic_replacement": True, "read_no_provider_or_owner_mutation": True,
                "takeover_stale_fencing_reload_event_expiry": True, "interturn_no_inferred_completion": True,
                "restart_retention": "archive" if provider == "acp" else "unavailable", "invalid_known_payload_visible_fault": fault})
        finally:
            stop()
    (root / "process-receipt.json").write_text(json.dumps(receipt, indent=2))
    print(json.dumps(receipt, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-dir", type=Path)
    args = parser.parse_args()
    if args.evidence_dir:
        args.evidence_dir.mkdir(mode=0o700, parents=True)
        verify(args.evidence_dir.resolve())
    else:
        with tempfile.TemporaryDirectory(prefix="fp-context-") as directory:
            verify(Path(directory))


if __name__ == "__main__":
    main()
