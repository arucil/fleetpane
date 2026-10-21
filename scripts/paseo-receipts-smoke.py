#!/usr/bin/env python3
"""Kill a synthetic batch worker; read/retry its receipt through public CLI."""
import argparse
import hashlib
import importlib.util
import json
import signal
import sqlite3
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / "target/debug/fleetpane-host"


def command(*args, ok=True):
    p = subprocess.run([str(BIN), "paseo-import", *map(str, args)], capture_output=True, text=True, timeout=60)
    if not ok:
        assert p.returncode != 0, p.stdout
        return p.stderr
    assert p.returncode == 0, p.stderr
    return json.loads(p.stdout)


def check(root, check_scratch=False):
    root.mkdir(parents=True, exist_ok=True)
    spec = importlib.util.spec_from_file_location("single", ROOT / "scripts/paseo-import-smoke.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    home, codex, dsh, config = module.fixture(root)
    cwd = root / "工作区"
    # Sorted last -> first copied; stop after observing its committed checkpoint
    # while the following large history is still being parsed.
    for identity, count in (("zz-complete", 2), ("zy-inflight", 60000)):
        (home / f"agents/project/{identity}.json").write_text(json.dumps({"id": identity, "provider": "codex", "cwd": str(cwd), "workspaceId": "workspace-one", "persistence": {"provider": "codex", "sessionId": identity}}))
        log = codex / f"{identity}.jsonl"
        with log.open("w") as output:
            output.write(json.dumps({"type": "session_meta", "payload": {"id": identity, "cwd": str(cwd)}}) + "\n")
            for i in range(count):
                output.write(json.dumps({"type": "response_item", "payload": {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": f"durable item {i} " + "x" * 120}]}}) + "\n")
        with sqlite3.connect(codex / "state_5.sqlite") as db:
            db.execute("INSERT INTO threads VALUES(?,?)", (identity, str(log)))
    def source_hashes():
        return {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest() for base in (home, codex, dsh) for p in base.rglob("*") if p.is_file()}
    original = source_hashes()
    plan = command("plan", home, config, "Migration QA", codex, dsh)
    path = root / "plan.json"
    path.write_text(json.dumps(plan))
    store = config.with_suffix(".creations")
    process = subprocess.Popen([str(BIN), "paseo-import", "batch", str(store), str(path)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        deadline = time.monotonic() + 25
        while True:
            assert process.poll() is None, process.communicate()
            if (store / "creations.sqlite3").exists():
                batches = command("batches", store)
                if batches and batches[0]["progress"]["current_session"] == "zy-inflight":
                    break
            assert time.monotonic() < deadline, "no committed in-flight checkpoint"
            time.sleep(0.01)
        prior = batches[0]
        identity = prior["id"]
        assert prior["writer_active"] and prior["progress"]["histories_available"] == 1
        assert "active worker" in command("retry-batch", store, identity, ok=False)
        active = command("batch-results", store, identity)
        assert active["summary"]["writer_active"]
        assert active["results"][0]["histories_available"] >= 1
        if check_scratch:
            # Observe creation before pausing: the receipt is published before
            # its copy worker starts, and is not itself proof a file is open.
            while not any((store / "paseo-scratch-v1").glob("*.tmp")):
                assert process.poll() is None, process.communicate()
                assert time.monotonic() < deadline, "copy never created scratch"
                time.sleep(0.01)
            # SIGSTOP freezes the actual writer while retaining its locks.
            process.send_signal(signal.SIGSTOP)
            assert "active" in command("scratch-status", store, ok=False)
            assert "active" in command("clean-scratch", store, ok=False)
        # Full process death, unlike cooperative Stop; the file lock must drop.
        process.kill()
        process.communicate(timeout=10)
    finally:
        if process.poll() is None:
            process.kill()
            process.communicate(timeout=10)
    interrupted = command("batch-status", store, identity)
    assert interrupted["summary"]["state"] == "running" and not interrupted["summary"]["writer_active"]
    assert len(command("batches", store)) == 1
    saved = command("list", store)
    sessions = command("sessions", store, saved[0]["id"])
    complete = next(s for s in sessions if s["agent"]["id"] == "zz-complete")
    assert command("history", store, complete["id"], 0)["status"]["rows"] == 2
    snapshots = {str(p.relative_to(store)): hashlib.sha256(p.read_bytes()).hexdigest() for p in store.rglob("*") if p.is_file() and "paseo-history" in str(p) and ".tmp" not in str(p)}
    assert snapshots, "a complete snapshot was retained"
    # Reading the interrupted report never starts a second attempt.
    assert command("batch-status", store, identity) == interrupted
    again = command("retry-batch", store, identity)
    assert again["batch_id"] != identity and again["results"][0]["reused"]
    assert again["progress"]["histories_available"] == 4 and again["progress"]["histories_failed"] == 1
    assert again["results"][0]["histories_finished"]
    assert command("batch-status", store, identity) == interrupted
    assert command("batch-results", store, again["batch_id"])["results"] == again["results"]
    assert [s["id"] for s in command("sessions", store, saved[0]["id"])] == [s["id"] for s in sessions]
    assert all(hashlib.sha256((store / p).read_bytes()).hexdigest() == h for p, h in snapshots.items())
    assert source_hashes() == original and not (root / "unstarted.sock").exists()
    receipt = {"active_writer_retry_refused": True, "killed_worker_report_retained": True, "read_does_not_resume": True, "explicit_retry_new_receipt": True, "stable_session_and_snapshot_ids": True, "source_unchanged": True, "host_never_started": True, "store": str(store), "config": str(config), "interrupted": interrupted, "retried": again}
    (root / "receipt.json").write_text(json.dumps(receipt, ensure_ascii=False, indent=2))
    print(json.dumps(receipt, ensure_ascii=False))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--keep", type=Path)
    args = parser.parse_args()
    if args.keep:
        check(args.keep.resolve())
    else:
        with tempfile.TemporaryDirectory(prefix="fp-paseo-receipts-") as directory:
            check(Path(directory))
