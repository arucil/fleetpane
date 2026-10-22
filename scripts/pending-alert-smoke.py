#!/usr/bin/env python3
"""Task-owned Codex/ACP request occurrence and durable claim acceptance."""
import argparse
import json
import runpy
import signal
import sqlite3
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"
fixture = runpy.run_path(str(ROOT / "scripts/completion-smoke.py"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", type=Path)
    args = parser.parse_args()
    temporary = None if args.fixture else tempfile.TemporaryDirectory(prefix="fp-pending-", dir="/tmp")
    root = args.fixture.resolve() if args.fixture else Path(temporary.name)
    root.mkdir(parents=True, exist_ok=True)
    processes, hosts = [], []
    keep = False
    try:
        _, _, config, command, snapshot = fixture["prepare"](root, processes, hosts)
        prefs = root / "preferences"
        if args.fixture:
            keep = True
            print(json.dumps({"hosts": str(config), "preferences": str(prefs), "pids": [p.pid for p in processes]}))
            return

        argv = [str(BINARY), "pending-alerts", str(config), str(prefs)]

        def claim(ok=True):
            result = subprocess.run(argv, text=True, capture_output=True, timeout=15)
            assert (result.returncode == 0) == ok, result.stderr
            return json.loads(result.stdout) if ok else result.stderr

        def snapshots():
            return [snapshot(i) for i in range(2)]

        def requests():
            for i in range(2):
                thread = command(i, "create", str(root / str(i)))["thread"]["id"]
                before = len(snapshot(i)["pending_requests"])
                command(i, "send", thread, "[questions]" if i == 0 else "[permission]")
                fixture["wait"](lambda: len(snapshot(i)["pending_requests"]) == before + 1)

        before = snapshots()
        assert claim()["items"] == [] and snapshots() == before
        requests()
        before = snapshots()
        a = subprocess.Popen(argv, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        b = subprocess.Popen(argv, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        batches = []
        for process in [a, b]:
            out, err = process.communicate(timeout=15)
            assert process.returncode == 0, err
            batches.append(json.loads(out))
        items = [i for batch in batches for i in batch["items"]]
        assert len(items) == 2 and {i["kind"] for i in items} == {"approval", "question"}
        assert all(i["request_sequence"] > 0 and i["observed_unix_ms"] > 0 for i in items)
        assert claim()["items"] == [] and snapshots() == before

        def mute(value):
            current = json.loads(subprocess.check_output([str(BINARY), "notification-policy", str(prefs), "get"], text=True))
            current["value"]["muted"] = value
            subprocess.run([str(BINARY), "notification-policy", str(prefs), "set", str(current["revision"])], input=json.dumps(current["value"]), text=True, capture_output=True, check=True, timeout=15)

        mute(True)
        requests()
        before = snapshots()
        assert claim()["items"] == []
        mute(False)
        assert claim()["items"] == [] and snapshots() == before
        requests()
        before = snapshots()
        with sqlite3.connect(prefs / "creations.sqlite3") as db:
            cursors = db.execute("SELECT * FROM pending_alert_cursors ORDER BY socket").fetchall()
            db.execute("CREATE TRIGGER refuse_pending BEFORE UPDATE ON pending_alert_cursors BEGIN SELECT RAISE(ABORT,'owned pending refusal'); END")
        assert "owned pending refusal" in claim(False)
        with sqlite3.connect(prefs / "creations.sqlite3") as db:
            assert db.execute("SELECT * FROM pending_alert_cursors ORDER BY socket").fetchall() == cursors
            db.execute("DROP TRIGGER refuse_pending")
        assert len(claim()["items"]) == 2 and claim()["items"] == []
        assert snapshots() == before
        original = snapshot(1)["pending_requests"][0]
        answered = subprocess.run([str(BINARY), "session", hosts[1]["socket"], "--take-control", "permission", json.dumps(original["id"]), "allow-one"], text=True, capture_output=True, check=True, timeout=15)
        assert json.loads(answered.stdout) == {"answered": True}
        fixture["wait"](lambda: len(snapshot(1)["pending_requests"]) == 3)
        assert all(o["id"] != original["id"] for o in snapshot(1)["pending_observations"])
        assert claim()["items"] == []
        print(json.dumps({"process_claims": [len(b["items"]) for b in batches], "kinds": sorted(i["kind"] for i in items), "host_observation_time": True, "muted_consumed": True, "unmute_replay": False, "failure_rolls_back": True, "host_snapshots_unchanged_by_claims": True, "answered_occurrence_removed": True}))
    finally:
        if not keep:
            for process in processes:
                if process.poll() is None:
                    process.send_signal(signal.SIGINT)
                    process.wait(timeout=12)
            if temporary:
                temporary.cleanup()


if __name__ == "__main__":
    main()
