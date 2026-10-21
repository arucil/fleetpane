#!/usr/bin/env python3
"""Observe colliding session IDs across Hosts, with bounded paging and no takeover."""
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


def wait(probe):
    deadline = time.monotonic() + 15
    while True:
        value = probe()
        if value:
            return value
        assert time.monotonic() < deadline, "directory fixture deadline"
        time.sleep(.025)


class Fixture:
    def __init__(self, root):
        self.root = root
        self.processes = []
        self.hosts = []
        self.config = root / "hosts.json"

    def start(self):
        for index, name in enumerate(["Codex A", "Codex B", "DeepSeek", "No owner"]):
            home = self.root / str(index)
            home.mkdir()
            self.hosts.append({"name": name, "socket": str(home / "host/control.sock")})
            history = home / "history.json"
            threads = [{
                "id": f"same-{i:03}", "cwd": str(home), "preview": f"{name} message {i}",
                "name": f"{name} 共享 {i:03}", "status": {"type": "idle"},
                "turns": [{"id": "saved-turn", "status": "completed", "items": [{"id": "saved-text", "type": "agentMessage", "text": f"History from {name} {i:03}"}]}],
            } for i in range(237)]
            threads.extend({
                "id": f"archived-{i:03}", "cwd": str(home if i < 137 else home / "second-workspace"),
                "preview": f"{name} archived message {i}", "name": f"{name} 归档 {i:03}",
                "fixtureArchived": True, "status": {"type": "idle"},
                "turns": [{"id": "archived-turn", "status": "completed", "items": [{"id": "archived-text", "type": "agentMessage", "text": f"Archived history from {name} {i:03}"}]}],
            } for i in range(237))
            history.write_text(json.dumps(threads))
            log = home / "host.log"
            provider = ["python3", str(ROOT / "scripts/acp-fixture.py")] if index == 2 else ["python3", str(ROOT / "scripts/provider-fixture.py"), "--history", str(history)]
            with log.open("w") as output:
                process = subprocess.Popen([str(BINARY), "serve-acp" if index == 2 else "serve", self.hosts[-1]["socket"], str(home), *provider], stdout=subprocess.DEVNULL, stderr=output, start_new_session=True)
            self.processes.append(process)
            wait(lambda: process.poll() is not None or "ready:" in log.read_text())
            assert process.poll() is None, log.read_text()
            if index < 2:
                self.session(index, "list")
            elif index == 2:
                thread = self.session(index, "create", str(home))["result"]["thread"]["id"]
                self.session(index, "send", thread, "[permission]")
                wait(lambda: self.snapshot(index)["pending_requests"])
        self.hosts.append({"name": "Offline", "socket": str(self.root / "offline/control.sock")})
        self.config.write_text(json.dumps({"version": 1, "hosts": self.hosts}))

    def raw(self, index, value):
        with socket.socket(socket.AF_UNIX) as stream:
            stream.settimeout(10)
            stream.connect(self.hosts[index]["socket"])
            stream.sendall(json.dumps(value).encode() + b"\n")
            return json.loads(stream.makefile().readline(1024 * 1024))

    def snapshot(self, index):
        return self.raw(index, {"command": "snapshot"})["data"]

    def session(self, index, *args):
        return json.loads(subprocess.check_output([str(BINARY), "session", self.hosts[index]["socket"], "--take-control", *args], text=True, timeout=20))

    def observe(self, *args, ok=True):
        result = subprocess.run([str(BINARY), "sessions", str(self.config), *args], text=True, capture_output=True, timeout=20)
        assert (result.returncode == 0) == ok, result.stderr
        return json.loads(result.stdout) if ok else result.stderr

    def close(self):
        for process in self.processes:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGINT)
                process.wait(timeout=10)
        assert all(not Path(host["socket"]).exists() for host in self.hosts)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-dir", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="fp-fleet-dir-", dir="/tmp") as temporary:
        fixture = Fixture(Path(temporary))
        try:
            fixture.start()
            before = [fixture.snapshot(i) for i in range(4)]
            report = fixture.observe()
            assert [r["endpoint"]["name"] for r in report] == [h["name"] for h in fixture.hosts]
            assert [len(r["observation"]["page"]["data"]) for r in report[:3]] == [100, 100, 1]
            assert all(r["error"] and r["observation"] is None for r in report[3:])
            assert "No controller lease" in report[3]["error"]
            assert report[0]["observation"]["page"]["data"][0]["id"] == report[1]["observation"]["page"]["data"][0]["id"]
            assert report[0]["observation"]["host_id"] != report[1]["observation"]["host_id"]
            assert all(not t["turns"] for r in report[:3] for t in r["observation"]["page"]["data"])
            first = report[0]["observation"]
            second = fixture.observe("--host", "Codex A", "--cursor", first["page"]["nextCursor"], "--host-id", first["host_id"])[0]["observation"]
            third = fixture.observe("--host", "Codex A", "--cursor", second["page"]["nextCursor"], "--host-id", first["host_id"])[0]["observation"]
            assert len(second["page"]["data"]) == 100 and len(third["page"]["data"]) == 37 and third["page"]["nextCursor"] is None
            filtered = fixture.observe("--search", "共享 236")
            assert [r["observation"]["page"]["data"][0]["id"] for r in filtered[:2]] == ["same-236"] * 2
            assert "does not support server-side title filtering" in filtered[2]["error"]
            assert fixture.observe("--search", "absent")[0]["observation"]["page"]["data"] == []
            assert fixture.observe("--cwd", str(fixture.root / "1"))[0]["observation"]["page"]["data"] == []
            assert fixture.observe("--search", "[list-fail]")[0]["error"]
            assert "Host changed" in fixture.observe("--host", "Codex A", "--host-id", str(uuid.uuid4()))[0]["error"]
            assert "Paging requires" in fixture.observe("--host", "Codex A", "--cursor", "orphan", ok=False)
            fixture.observe("--cwd", "relative", ok=False)
            fixture.observe("--host", "Codex A", "--cwd", "relative", ok=False)
            for flags in [("--archived", "--archived"), ("--search",), ("--host-id",), ("--cwd", str(fixture.root), "--cwd", str(fixture.root))]:
                fixture.observe(*flags, ok=False)
            archived = fixture.observe("--archived")
            assert [len(r["observation"]["page"]["data"]) for r in archived[:2]] == [100, 100]
            assert archived[2]["error"] and all(t["id"].startswith("archived-") and not t["turns"] for r in archived[:2] for t in r["observation"]["page"]["data"])
            page = archived[0]["observation"]
            archive_pages = [len(page["page"]["data"])]
            while page["page"]["nextCursor"]:
                page = fixture.observe("--host", "Codex A", "--cursor", page["page"]["nextCursor"], "--archived", "--host-id", page["host_id"])[0]["observation"]
                archive_pages.append(len(page["page"]["data"]))
            assert archive_pages == [100, 100, 37]
            scoped = fixture.observe("--cwd", str(fixture.root / "1"), "--archived")
            assert scoped[0]["observation"]["page"]["data"] == []
            scoped_b = scoped[1]["observation"]
            scoped_tail = fixture.observe("--archived", "--host", "Codex B", "--cwd", str(fixture.root / "1"), "--cursor", scoped_b["page"]["nextCursor"], "--host-id", scoped_b["host_id"])[0]["observation"]
            assert len(scoped_tail["page"]["data"]) == 37 and scoped_tail["page"]["nextCursor"] is None
            assert fixture.observe("--search", "归档 236", "--archived")[0]["observation"]["page"]["data"][0]["id"] == "archived-236"
            after = [fixture.snapshot(i) for i in range(4)]
            assert [{k: v for k, v in state.items() if k != "sequence"} for state in after] == [{k: v for k, v in state.items() if k != "sequence"} for state in before], "observation changed Host state, lease, history or pending requests"
            assert all(a["sequence"] >= b["sequence"] for a, b in zip(after, before))
            receipt = {"binary_sha256": hashlib.sha256(BINARY.read_bytes()).hexdigest(), "initial_report": report, "pages": [100, 100, 37], "archive_pages": archive_pages, "archive_cwd_pages": [100, 37], "archive_search": "archived-236", "colliding_ids_isolated": True, "snapshots_before": before, "snapshots_after": after, "query_response_sequences_excluded": True, "failed_query_explicit": True, "wrong_host_refused": True}
        finally:
            fixture.close()
        receipt["owned_hosts_stopped"] = True
        if args.evidence_dir:
            args.evidence_dir.mkdir(parents=True, exist_ok=True)
            (args.evidence_dir / "process-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print("PASS: cross-Host metadata, archive/workspace/Unicode filters and paging, explicit errors and unchanged leases/requests")


if __name__ == "__main__":
    main()
