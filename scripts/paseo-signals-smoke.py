#!/usr/bin/env python3
"""Cooperative CLI SIGINT/SIGTERM with real copies and durable stopped reports."""
import argparse
import hashlib
import importlib.util
import json
import os
import signal
import sqlite3
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / "target/debug/fleetpane-host"


def command(*args):
    result = subprocess.run([str(BIN), "paseo-import", *map(str, args)], capture_output=True, text=True, timeout=60)
    assert result.returncode == 0, result.stderr
    return json.loads(result.stdout)


def check(root):
    root.mkdir(parents=True, exist_ok=True)
    spec = importlib.util.spec_from_file_location("single", ROOT / "scripts/paseo-import-smoke.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    home, codex, dsh, config = module.fixture(root)
    store = root / "hosts.creations"
    cwd = root / "工作区"
    for identity in ("zz-first", "zy-second"):
        (home / f"agents/project/{identity}.json").write_text(json.dumps({"id": identity, "provider": "codex", "cwd": str(cwd), "workspaceId": "workspace-one", "persistence": {"provider": "codex", "sessionId": identity}}))
        log = codex / f"{identity}.jsonl"
        with log.open("w") as output:
            output.write(json.dumps({"type": "session_meta", "payload": {"id": identity, "cwd": str(cwd)}}) + "\n")
            for i in range(60000):
                output.write(json.dumps({"type": "response_item", "payload": {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": f"{identity} item {i} " + "x" * 120}]}}) + "\n")
        with sqlite3.connect(codex / "state_5.sqlite") as db:
            db.execute("INSERT INTO threads VALUES(?,?)", (identity, str(log)))

    def source_hashes():
        return {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest() for base in (home, codex, dsh) for p in base.rglob("*") if p.is_file()}

    original = source_hashes()
    (root / "source-before.json").write_text(json.dumps(original, indent=2))
    plan = root / "plan.json"
    plan.write_text(json.dumps(command("plan", home, config, "Migration QA", codex, dsh)))

    def stop_copy(args, identity, sig, available):
        prior_ids = {b["id"] for b in command("batches", store)}
        process = subprocess.Popen([str(BIN), "paseo-import", *map(str, args)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            deadline = time.monotonic() + 25
            while True:
                assert process.poll() is None, process.communicate()
                active = [b for b in command("batches", store) if b["id"] not in prior_ids]
                if active and active[0]["progress"]["current_session"] == identity and any((store / "paseo-scratch-v1").glob("*.tmp")):
                    break
                assert time.monotonic() < deadline, "no active copy"
                time.sleep(0.01)
            process.send_signal(signal.SIGSTOP)
            _, status = os.waitpid(process.pid, os.WUNTRACED)
            assert os.WIFSTOPPED(status)
            process.send_signal(sig)
            process.send_signal(sig)
            process.send_signal(signal.SIGCONT)
            stdout, stderr = process.communicate(timeout=60)
            assert process.returncode == 0, (stdout, stderr)
        finally:
            if process.poll() is None:
                process.kill()
                process.communicate(timeout=10)
        report = json.loads(stdout)
        (root / f"{identity}-stdout.json").write_text(stdout)
        (root / f"{identity}-stderr.log").write_text(stderr)
        assert report["cancelled"] and report["progress"]["histories_available"] == available, report
        assert not report["results"][0]["histories_finished"]
        assert "stop requested" in stderr and signal.Signals(sig).name in stderr
        saved = command("batch-status", store, report["batch_id"])
        (root / f"{identity}-saved.json").write_text(json.dumps(saved, indent=2))
        assert saved["summary"]["state"] == "stopped" and not saved["summary"]["writer_active"]
        assert command("batch-results", store, report["batch_id"])["results"] == report["results"]
        assert command("scratch-status", store)["files"] == 0
        sessions = command("sessions", store, report["results"][0]["imported_id"])
        current = next(s for s in sessions if s["agent"]["id"] == identity)
        assert command("history", store, current["id"], 0)["status"]["rows"] == 60000
        return report, saved

    # Create an empty store through a read before starting the owned worker.
    command("batches", store)
    first, first_saved = stop_copy(["batch", store, plan], "zz-first", signal.SIGINT, 1)
    complete_before = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in (store / "paseo-history").glob("*.sqlite")}
    second, second_saved = stop_copy(["retry-batch", store, first["batch_id"]], "zy-second", signal.SIGTERM, 2)
    final = command("retry-batch", store, second["batch_id"])
    (root / "final-report.json").write_text(json.dumps(final, indent=2))
    assert not final["cancelled"] and final["progress"]["histories_available"] == 4 and final["progress"]["histories_failed"] == 1
    assert command("batch-status", store, first["batch_id"]) == first_saved
    assert command("batch-status", store, second["batch_id"]) == second_saved
    assert len(command("batches", store)) == 3
    assert all(hashlib.sha256((store / "paseo-history" / name).read_bytes()).hexdigest() == digest for name, digest in complete_before.items())
    assert source_hashes() == original and not (root / "unstarted.sock").exists()
    (root / "source-after.json").write_text(json.dumps(source_hashes(), indent=2))
    result = {"sigint_finishes_current_copy": True, "sigterm_retry_finishes_current_copy": True, "repeated_signal_preserves_worker": True, "stopped_reports_durable": True, "explicit_retry_new_attempt": True, "original_reports_unchanged": True, "completed_archive_unchanged": True, "source_unchanged": True, "no_scratch_left": True, "host_never_started": True, "first_batch": first["batch_id"], "second_batch": second["batch_id"], "final_batch": final["batch_id"]}
    (root / "signals-receipt.json").write_text(json.dumps(result, indent=2))
    print(json.dumps(result))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--keep", type=Path)
    args = parser.parse_args()
    if args.keep:
        check(args.keep.resolve())
    else:
        with tempfile.TemporaryDirectory(prefix="fp-paseo-signals-") as directory:
            check(Path(directory))
