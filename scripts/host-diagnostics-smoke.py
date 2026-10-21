#!/usr/bin/env python3
"""Observe real fixture Hosts plus incompatible/stalled endpoints without changing them."""
import argparse
import copy
import hashlib
import json
import os
import signal
import socket
import subprocess
import tempfile
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-dir", type=Path)
    parser.add_argument("--native-root", type=Path, help="Keep owned fixtures alive until SIGTERM for isolated GUI QA")
    args = parser.parse_args()
    temporary = tempfile.TemporaryDirectory(prefix="fp-diagnostics-") if not args.native_root else None
    root = args.native_root or Path(temporary.name)
    root.mkdir(mode=0o700, parents=True, exist_ok=True)
    processes, endpoints, servers = [], [], []
    stop = threading.Event()

    def raw(endpoint, request):
        with socket.socket(socket.AF_UNIX) as stream:
            stream.settimeout(8)
            stream.connect(endpoint["socket"])
            stream.sendall(json.dumps(request).encode() + b"\n")
            return json.loads(stream.makefile().readline())

    def snapshot(index):
        value = raw(endpoints[index], {"command": "snapshot"})
        assert value["response"] == "snapshot", value
        return value["data"]

    def command(index, *arguments):
        value = subprocess.run([str(BINARY), "session", endpoints[index]["socket"], "--take-control", *arguments], capture_output=True, text=True, timeout=15)
        assert value.returncode == 0, value.stderr
        return json.loads(value.stdout)

    def wait(probe):
        deadline = time.monotonic() + 15
        while not probe():
            assert time.monotonic() < deadline, "owned diagnostic fixture timed out"
            time.sleep(.025)

    def fake(name, response):
        endpoint = {"name": name, "socket": str(root / f"fake-{len(servers)}.sock")}
        server = socket.socket(socket.AF_UNIX)
        server.bind(endpoint["socket"])
        server.listen()
        server.settimeout(.1)
        requests = []
        servers.append((server, requests))
        endpoints.append(endpoint)

        def listen():
            while not stop.is_set():
                try:
                    stream, _ = server.accept()
                except socket.timeout:
                    continue
                except OSError:
                    if stop.is_set():
                        return
                    raise
                with stream:
                    stream.settimeout(8)
                    request = json.loads(stream.makefile().readline())
                    requests.append(request)
                    if response is None:
                        stop.wait(6)
                    else:
                        stream.sendall(json.dumps(response).encode() + b"\n")

        threading.Thread(target=listen, daemon=True).start()

    try:
        for index, name in enumerate(["Codex 开发", "DeepSeek permission", "Draining", "Provider fault"]):
            home = root / str(index)
            home.mkdir()
            endpoint = {"name": name, "socket": str(home / "host/control.sock")}
            endpoints.append(endpoint)
            log = home / "host.log"
            with log.open("w") as output:
                process = subprocess.Popen([str(BINARY), "serve-acp" if index == 1 else "serve", endpoint["socket"], str(home), "python3", str(ROOT / "scripts" / ("acp-fixture.py" if index == 1 else "provider-fixture.py"))], stdout=subprocess.DEVNULL, stderr=output, start_new_session=True)
            processes.append(process)
            wait(lambda: process.poll() is not None or "ready:" in log.read_text())
            assert process.poll() is None, log.read_text()
            if index < 2:
                thread = command(index, "create", str(home))["result"]["thread"]["id"]
                if index == 1:
                    command(index, "send", thread, "[permission]")
                    wait(lambda: snapshot(index)["pending_requests"])
            elif index == 2:
                command(index, "drain")
            else:
                os.kill(snapshot(index)["provider_pid"], signal.SIGTERM)
                wait(lambda: snapshot(index)["fault"] is not None)
        # Deliberately colliding identities must not replace configured endpoint binding.
        incompatible = copy.deepcopy(snapshot(0))
        incompatible["protocol"] = 999
        fake("Incompatible", {"response": "snapshot", "data": incompatible})
        fake("Rejected", {"response": "error", "data": {"code": "fixture_refused", "message": "explicit refusal"}})
        endpoints.append({"name": "Offline", "socket": str(root / "missing/control.sock")})
        fake("Stalled", None)
        config = root / "hosts.json"
        config.write_text(json.dumps({"version": 1, "hosts": endpoints}))
        before = [snapshot(index) for index in range(4)]
        (root / "before.json").write_text(json.dumps(before))
        started = time.monotonic()
        result = subprocess.run([str(BINARY), "host-diagnostics", str(config)], capture_output=True, text=True, timeout=12)
        elapsed = time.monotonic() - started
        assert result.returncode == 0, result.stderr
        reports = json.loads(result.stdout)
        assert [r["endpoint"] for r in reports] == endpoints
        assert [r["status"] for r in reports] == ["reachable"] * 5 + ["unavailable"] * 3
        assert reports[1]["observation"]["pending_requests"] == 1
        assert reports[2]["observation"]["admission"]["mode"] == "draining"
        assert reports[3]["observation"]["fault_scope"] == "provider"
        assert reports[4]["observation"]["protocol"] == 999
        assert "fixture_refused" in reports[5]["error"]
        assert all(r["checked_at_unix_ms"] > 0 for r in reports)
        assert reports[0]["elapsed_ms"] < 2000 and 4.5 <= elapsed < 10, (elapsed, reports)
        assert [snapshot(index) for index in range(4)] == before, "diagnostics changed a Host"
        assert all("provider_info" not in r.get("observation", {}) for r in reports)
        assert all("params" not in r.get("observation", {}) for r in reports)
        one = json.loads(subprocess.check_output([str(BINARY), "host-diagnostics", str(config), endpoints[1]["name"]], text=True, timeout=10))
        assert len(one) == 1 and one[0]["endpoint"] == endpoints[1]
        bad = subprocess.run([str(BINARY), "host-diagnostics", str(config), "absent"], capture_output=True, text=True, timeout=10)
        assert bad.returncode != 0 and "not found" in bad.stderr
        assert [snapshot(index) for index in range(4)] == before
        assert all(all(request == {"command": "snapshot"} for request in requests) for _, requests in servers)
        receipt = {"binary_sha256": hashlib.sha256(BINARY.read_bytes()).hexdigest(), "elapsed_seconds": elapsed, "reports": reports, "before": before, "after": [snapshot(index) for index in range(4)], "fake_requests": [requests for _, requests in servers]}
        if args.evidence_dir:
            args.evidence_dir.mkdir(parents=True, exist_ok=True)
            (args.evidence_dir / "process.json").write_text(json.dumps(receipt, indent=2) + "\n")
        print(f"PASS: eight ordered endpoints, exact single check, timeout isolation, protocol/drain/provider fault, unchanged full snapshots ({elapsed:.2f}s)", flush=True)
        if args.native_root:
            signal.signal(signal.SIGTERM, lambda *_: stop.set())
            signal.signal(signal.SIGINT, lambda *_: stop.set())
            print(f"READY {config}", flush=True)
            stop.wait()
    finally:
        stop.set()
        for process in processes:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                process.wait(timeout=8)
        for server, _ in servers:
            server.close()
        if temporary:
            temporary.cleanup()


if __name__ == "__main__":
    main()
