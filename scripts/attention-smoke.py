#!/usr/bin/env python3
"""Read-only global attention, long pending permissions and exact provider answers."""
import argparse
import hashlib
import json
import os
import signal
import socket
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--hold-seconds", type=int, default=125)
    parser.add_argument("--evidence-dir", type=Path)
    args = parser.parse_args()
    assert 1 <= args.hold_seconds <= 600
    with tempfile.TemporaryDirectory(prefix="fp-attention-") as directory:
        root = Path(directory)
        processes, hosts, provider_pids = [], [], []

        def wait(probe):
            deadline = time.monotonic() + 15
            while True:
                value = probe()
                if value:
                    return value
                assert time.monotonic() < deadline, "owned attention fixture timed out"
                time.sleep(.025)

        def raw(index, value):
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(5)
                stream.connect(hosts[index]["socket"])
                stream.sendall(json.dumps(value).encode() + b"\n")
                return json.loads(stream.makefile().readline())

        def snapshot(index):
            response = raw(index, {"command": "snapshot"})
            assert response["response"] == "snapshot", response
            state = response["data"]
            assert state["fault"] is None, state
            return state

        def command(index, *arguments):
            result = subprocess.run([str(BINARY), "session", hosts[index]["socket"], "--take-control", *arguments], capture_output=True, text=True, timeout=15)
            assert result.returncode == 0, result.stderr
            return json.loads(result.stdout)

        def observe():
            return json.loads(subprocess.check_output([str(BINARY), "attention", str(config)], text=True, timeout=15))

        receipt = {"host_binary_sha256": hashlib.sha256(BINARY.read_bytes()).hexdigest()}
        try:
            for index, name in enumerate(["Questions", "Approval A", "Approval B"]):
                home = root / str(index)
                home.mkdir()
                endpoint = {"name": name, "socket": str(home / "host/control.sock")}
                log = home / "host.log"
                with log.open("w") as output:
                    process = subprocess.Popen([str(BINARY), "serve" if index == 0 else "serve-acp", endpoint["socket"], str(home), "python3", str(ROOT / "scripts" / ("provider-fixture.py" if index == 0 else "acp-fixture.py"))], stdout=subprocess.DEVNULL, stderr=output, start_new_session=True)
                processes.append(process)
                hosts.append(endpoint)
                wait(lambda: process.poll() is not None or "ready:" in log.read_text())
                assert process.poll() is None, log.read_text()
                thread = command(index, "create", str(home))["result"]["thread"]["id"]
                command(index, "send", thread, "[questions]" if index == 0 else "[permission]")
                wait(lambda: snapshot(index)["pending_requests"])
            config = root / "hosts.json"
            config.write_text(json.dumps({"version": 1, "hosts": hosts + [{"name": "Offline", "socket": str(root / "offline/control.sock") }]}))
            before = [snapshot(index) for index in range(3)]
            provider_pids = [state["provider_pid"] for state in before]
            assert before[1]["pending_requests"][0]["id"] == before[2]["pending_requests"][0]["id"]
            started = time.monotonic()
            report = observe()
            assert [len(host["items"]) for host in report["hosts"]] == [1, 1, 1, 0]
            assert [host["items"][0]["kind"] for host in report["hosts"][:3]] == ["question", "approval", "approval"]
            assert report["hosts"][3]["error"] is not None
            assert [snapshot(index) for index in range(3)] == before, "attention collection claimed or mutated a Host"
            while True:
                remaining_seconds = args.hold_seconds - (time.monotonic() - started)
                if remaining_seconds <= 0:
                    break
                assert all(process.poll() is None for process in processes)
                time.sleep(min(.5, remaining_seconds))
            elapsed = time.monotonic() - started
            assert [snapshot(index) for index in range(3)] == before, "a pending request changed without a decision"
            held_report = observe()
            assert [host["items"] for host in held_report["hosts"]] == [host["items"] for host in report["hosts"]]
            request = before[1]["pending_requests"][0]
            old_lease = before[1]["owner"]
            command(1, "list")  # An explicit replacement controller changes the fence.
            stale = raw(1, {"command": "respond", "lease": old_lease, "request_id": request["id"], "result": {"outcome": {"outcome": "selected", "optionId": "allow-one"}}})
            assert stale["response"] == "error" and stale["data"]["code"] == "stale_controller", stale
            assert command(1, "permission", json.dumps(request["id"]), "allow-one") == {"answered": True}
            provider_store = root / "1/fixture-acp-sessions.json"
            original_thread = request["params"]["threadId"]
            expected = {"outcome": {"outcome": "selected", "optionId": "allow-one"}}
            wait(lambda: json.loads(provider_store.read_text()).get(original_thread, {}).get("permission") == expected)
            wait(lambda: not snapshot(1)["pending_requests"])
            assert snapshot(2) == before[2], "colliding request ID on another Host was changed"
            assert snapshot(0) == before[0], "unrelated question was changed"
            duplicate = raw(1, {"command": "respond", "lease": snapshot(1)["owner"], "request_id": request["id"], "result": expected})
            assert duplicate["response"] == "error" and duplicate["data"]["code"] == "unknown_request", duplicate
            remaining = observe()
            assert [len(host["items"]) for host in remaining["hosts"]] == [1, 0, 1, 0]
            assert [snapshot(index)["provider_pid"] for index in range(3)] == provider_pids
            receipt.update(hold_seconds=elapsed, snapshots_unchanged_while_waiting=True, read_only_collection=True,
                           same_request_ids_isolated=True, original_provider_answer=expected, stale_controller_rejected=True,
                           duplicate_rejected=True, initial_report=report, held_report=held_report, remaining_report=remaining)
        finally:
            for process in processes:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGINT)
                    process.wait(timeout=10)
            assert all(not Path(host["socket"]).exists() for host in hosts)
            owned = [p.pid for p in processes] + provider_pids
            def ended(pid):
                path = Path(f"/proc/{pid}/stat")
                try: return path.read_text().split(") ", 1)[1].split()[0] == "Z"
                except FileNotFoundError: return True
            wait(lambda: all(ended(pid) for pid in owned))
            receipt["owned_processes_stopped"] = True
            receipt["owned_sockets_removed"] = True
        if args.evidence_dir:
            args.evidence_dir.mkdir(parents=True, exist_ok=True)
            (args.evidence_dir / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
        print(f"PASS: read-only attention, {elapsed:.3f}s pending preservation, fenced original provider answer and cross-Host isolation")


if __name__ == "__main__":
    main()
