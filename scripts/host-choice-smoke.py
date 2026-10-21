#!/usr/bin/env python3
"""Configuration-local Host choice, lost writes, conflict and schema upgrade."""
import json
import sqlite3
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"


def main():
    with tempfile.TemporaryDirectory(prefix="fp-host-choice-") as temp:
        root = Path(temp)
        config = root / "hosts.json"
        hosts = [{"name": name, "socket": str(root / f"{name}.sock")} for name in ["A", "开发 B"]]
        config.write_text(json.dumps({"version": 1, "hosts": hosts}))
        store = root / "controller"

        def command(*args, ok=True):
            result = subprocess.run([str(BINARY), "host-choice", str(store), *map(str,args)], capture_output=True, text=True, timeout=15)
            assert (result.returncode == 0) == ok, result.stderr
            return json.loads(result.stdout) if ok else result.stderr

        assert command("get") == {"revision": 0, "socket": None}
        saved = command("set", 0, config, "开发 B")
        assert saved == {"revision": 1, "socket": hosts[1]["socket"]}
        assert command("get") == saved
        assert "another window" in command("set", 0, config, "A", ok=False)
        assert "absent" in command("set", 1, config, "missing", ok=False)
        database = store / "creations.sqlite3"
        with sqlite3.connect(database) as db:
            db.execute("CREATE TRIGGER reject_selection BEFORE UPDATE ON host_selection BEGIN SELECT RAISE(ABORT,'injected choice write failure'); END")
        assert "injected choice write failure" in command("set", 1, config, "A", ok=False)
        assert command("get") == saved
        with sqlite3.connect(database) as db:
            db.execute("DROP TRIGGER reject_selection")
            db.execute("INSERT INTO drafts VALUES('retained',7,'exact legacy draft')")
            db.execute("DROP TABLE attention_reviews")
            db.execute("DROP TABLE host_selection")
            db.execute("PRAGMA user_version=10")
        assert command("get") == {"revision": 0, "socket": None}
        with sqlite3.connect(database) as db:
            assert db.execute("PRAGMA user_version").fetchone()[0] == 12
            assert db.execute("SELECT data FROM drafts").fetchone()[0] == "exact legacy draft"
        command("set", 0, config, "A")
        with sqlite3.connect(database) as db:
            db.execute("UPDATE host_selection SET socket='corrupt'")
        assert command("get", ok=False)
        assert command("set", 1, config, "开发 B", ok=False)
        with sqlite3.connect(database) as db:
            assert db.execute("SELECT socket FROM host_selection").fetchone()[0] == "corrupt"
        assert all(not Path(host["socket"]).exists() for host in hosts)
    print("PASS: exact Unicode Host choice, stale write/refusal, failed commit, v10 upgrade and corruption preservation; no Host started")


if __name__ == "__main__":
    main()
