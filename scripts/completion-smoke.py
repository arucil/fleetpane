#!/usr/bin/env python3
"""Owned Codex/ACP fixtures: terminal results and independent local review receipts."""

import argparse
import json
import os
import signal
import socket
import sqlite3
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"


def wait(probe):
    deadline = time.monotonic() + 12
    while time.monotonic() < deadline:
        if result := probe():
            return result
        time.sleep(0.025)
    raise AssertionError("Owned completion fixture timed out")


def prepare(root, processes, hosts):

    def command(index, *args):
        result = subprocess.run([str(BINARY), "session", hosts[index]["socket"], "--take-control", *args], capture_output=True, text=True, timeout=12)
        assert result.returncode == 0, result.stderr
        return json.loads(result.stdout)["result"]

    def snapshot(index):
        with socket.socket(socket.AF_UNIX) as stream:
            stream.settimeout(5)
            stream.connect(hosts[index]["socket"])
            stream.sendall(b'{"command":"snapshot"}\n')
            return json.loads(stream.makefile().readline())["data"]

    for index, (name, mode, fixture) in enumerate([
        ("Codex fixture", "serve", "provider-fixture.py"),
        ("ACP fixture", "serve-acp", "acp-fixture.py"),
    ]):
        home = root / str(index)
        home.mkdir(parents=True)
        endpoint = {"name": name, "socket": str(home / "host/control.sock")}
        log = home / "host.log"
        with log.open("w") as output:
            process = subprocess.Popen([str(BINARY), mode, endpoint["socket"], str(home), "python3", str(ROOT / "scripts" / fixture)], stdout=subprocess.DEVNULL, stderr=output, start_new_session=True)
        processes.append(process)
        hosts.append(endpoint)
        wait(lambda: process.poll() is not None or "ready:" in log.read_text())
        assert process.poll() is None, log.read_text()
        identity = command(index, "create", str(home))["thread"]["id"]
        (home / "thread.txt").write_text(identity)
        for text in ["Completed output 中文", "[fail]"] + (["[stop:refusal]", "[stop:cancelled]"] if index == 1 else []):
            before = len(snapshot(index)["attention"]["records"])
            command(index, "send", identity, text)
            wait(lambda: len(snapshot(index)["attention"]["records"]) == before + 1)
        pending = command(index, "create", str(home))["thread"]["id"]
        command(index, "send", pending, "[questions]" if index == 0 else "[permission]")
        wait(lambda: snapshot(index)["pending_requests"])
    config = root / "hosts.json"
    config.write_text(json.dumps({"version": 1, "hosts": hosts + [{"name": "Offline", "socket": str(root / "offline.sock")}]}))
    (root / "pids.json").write_text(json.dumps([process.pid for process in processes]))
    return processes, hosts, config, command, snapshot


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", type=Path, help="Keep owned synthetic Hosts for native QA; caller must stop pids.json")
    args = parser.parse_args()
    temporary = None if args.fixture else tempfile.TemporaryDirectory(prefix="fp-completions-")
    root = args.fixture.resolve() if args.fixture else Path(temporary.name)
    root.mkdir(parents=True, exist_ok=True)
    processes, hosts = [], []
    keep = False
    try:
        processes, hosts, config, command, snapshot = prepare(root, processes, hosts)
        store = config.with_suffix(".creations")

        def observe():
            return json.loads(subprocess.check_output([str(BINARY), "attention", str(config), str(store)], text=True, timeout=12))

        def review(items, ok=True):
            path = root / "review-selection.json"
            path.write_text(json.dumps(items))
            result = subprocess.run([str(BINARY), "attention-review", str(store), str(path)], capture_output=True, text=True, timeout=12)
            assert (result.returncode == 0) == ok, result.stderr
            return json.loads(result.stdout) if ok else result.stderr

        report = observe()
        assert [len(host["completions"]) for host in report["hosts"]] == [2, 4, 0]
        assert [host["completions"][0]["record"]["kind"] for host in report["hosts"][:2]] == ["failed", "interrupted"]
        assert report["hosts"][1]["completions"][1]["record"]["stop_reason"] == "refusal"
        assert all(not item["reviewed"] for host in report["hosts"] for item in host["completions"])
        assert report["hosts"][2]["error"] and report["hosts"][2]["completion_evicted"] is None
        before = [snapshot(index) for index in range(2)]
        selected = [report["hosts"][0]["completions"][0]]
        assert review(selected) == {"reviewed": 1}
        assert observe()["hosts"][0]["completions"][0]["reviewed"]
        assert [snapshot(index) for index in range(2)] == before
        assert review(selected) == {"reviewed": 1}  # Idempotent exact receipt.
        with sqlite3.connect(store / "creations.sqlite3") as db:
            db.execute("CREATE TRIGGER reject_review BEFORE INSERT ON attention_reviews BEGIN SELECT RAISE(ABORT,'injected review write failure'); END")
        target = report["hosts"][1]["completions"]
        assert "injected review write failure" in review(target, ok=False)
        assert all(not item["reviewed"] for item in observe()["hosts"][1]["completions"])
        with sqlite3.connect(store / "creations.sqlite3") as db:
            db.execute("DROP TRIGGER reject_review")
        assert review(target) == {"reviewed": 4}
        final = observe()
        assert all(item["reviewed"] for item in final["hosts"][1]["completions"])
        assert final["hosts"][0]["completions"][0]["record"]["kind"] == "failed"
        assert not final["hosts"][0]["completions"][1]["reviewed"]
        assert [snapshot(index) for index in range(2)] == before
        (root / "before.json").write_text(json.dumps(before))
        (root / "report.json").write_text(json.dumps(final))
        if args.fixture:
            # Reset reviews for native interaction without touching either Host.
            with sqlite3.connect(store / "creations.sqlite3") as db:
                db.execute("DELETE FROM attention_reviews")
            keep = True
            print(json.dumps({"fixture": str(root), "hosts": hosts, "pids": [p.pid for p in processes]}))
            return
        print(json.dumps({"explicit_codex_acp_terminal_results": "PASS", "local_review_persistence_and_idempotency": "PASS", "failure_and_refusal_preserved": "PASS", "failed_write_not_marked_reviewed": "PASS", "independent_pending_requests_and_snapshots_unchanged": "PASS", "offline_host_explicit": "PASS"}))
    finally:
        if not keep:
            for process in processes:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGINT)
                    process.wait(timeout=10)
            if temporary:
                temporary.cleanup()


if __name__ == "__main__":
    main()
