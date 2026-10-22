#!/usr/bin/env python3
"""Owned Codex/ACP process acceptance for durable, policy-aware reminder claims."""
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
    parser.add_argument("--fixture", type=Path, help="Retain owned synthetic Hosts for native QA; stop pids.json afterwards")
    args = parser.parse_args()
    temporary = None if args.fixture else tempfile.TemporaryDirectory(prefix="fp-alerts-", dir="/tmp")
    root = args.fixture.resolve() if args.fixture else Path(temporary.name)
    root.mkdir(parents=True, exist_ok=True)
    processes, hosts = [], []
    keep = False
    try:
        _, _, config, command, snapshot = fixture["prepare"](root, processes, hosts)
        # Native acceptance uses both available feeds and their original requests.
        config.write_text(json.dumps({"version": 1, "hosts": hosts}))
        prefs = root / "preferences"
        if args.fixture:
            print(json.dumps({"hosts": str(config), "preferences": str(prefs), "pids": [p.pid for p in processes]}))
            keep = True
            return

        def claim(ok=True):
            result = subprocess.run([str(BINARY), "completion-alerts", str(config), str(prefs)], capture_output=True, text=True, timeout=15)
            assert (result.returncode == 0) == ok, result.stderr
            return json.loads(result.stdout) if ok else result.stderr

        def snapshots():
            return [snapshot(i) for i in range(2)]

        def send_results():
            for i in range(2):
                thread = (root / str(i) / "thread.txt").read_text()
                before = len(snapshot(i)["attention"]["records"])
                command(i, "send", thread, "[fail]" if i == 0 else "Reminder result 中文")
                fixture["wait"](lambda: len(snapshot(i)["attention"]["records"]) == before + 1)

        before = snapshots()
        assert claim()["items"] == [], "old retained results must baseline silently"
        assert snapshots() == before
        send_results()
        before = snapshots()
        argv = [str(BINARY), "completion-alerts", str(config), str(prefs)]
        a = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        b = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        batches = []
        for process in [a, b]:
            out, err = process.communicate(timeout=15)
            assert process.returncode == 0, err
            batches.append(json.loads(out))
        records = [item for batch in batches for item in batch["items"]]
        assert len(records) == 2 and {i["record"]["kind"] for i in records} == {"failed", "completed"}
        assert all(not i["reviewed"] for i in records)
        assert claim()["items"] == [] and snapshots() == before

        def policy(muted):
            current = json.loads(subprocess.check_output([str(BINARY), "notification-policy", str(prefs), "get"], text=True))
            current["value"]["muted"] = muted
            subprocess.run([str(BINARY), "notification-policy", str(prefs), "set", str(current["revision"])], input=json.dumps(current["value"]), text=True, check=True, capture_output=True, timeout=15)

        policy(True)
        send_results()
        before = snapshots()
        assert claim()["items"] == []
        policy(False)
        assert claim()["items"] == [] and snapshots() == before, "unmute replayed an old result"
        send_results()
        before = snapshots()
        database = prefs / "creations.sqlite3"
        with sqlite3.connect(database) as db:
            assert db.execute("PRAGMA user_version").fetchone()[0] == 17
            cursors = db.execute("SELECT * FROM completion_alert_cursors ORDER BY socket").fetchall()
            db.execute("CREATE TRIGGER refuse_claim BEFORE UPDATE ON completion_alert_cursors BEGIN SELECT RAISE(ABORT,'owned claim refusal'); END")
        assert "owned claim refusal" in claim(False)
        assert snapshots() == before
        raw = json.loads(subprocess.check_output([str(BINARY), "attention", str(config)], text=True, timeout=15))
        assert sum(len(h["items"]) for h in raw["hosts"]) == 2
        assert all(not i["reviewed"] for h in raw["hosts"] for i in h["completions"])
        with sqlite3.connect(database) as db:
            assert db.execute("SELECT * FROM completion_alert_cursors ORDER BY socket").fetchall() == cursors
            db.execute("DROP TRIGGER refuse_claim")
        assert len(claim()["items"]) == 2 and claim()["items"] == []
        assert snapshots() == before
        print(json.dumps({"process_claims": [len(b["items"]) for b in batches], "global_mute_consumes": True, "unmute_replay": False, "failure_rolls_back": True, "full_host_snapshots_unchanged": True, "pending": 2, "schema": 17}))
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
