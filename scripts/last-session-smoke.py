#!/usr/bin/env python3
"""Private last-session choices: exact identities, CAS, migration, failures; no RPC."""
import json
import sqlite3
import subprocess
import tempfile
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"


def main():
    with tempfile.TemporaryDirectory(prefix="fp-last-session-") as temporary:
        root = Path(temporary)
        store = root / "controller"
        a, b = root / "a.sock", root / "开发 B.sock"

        def run(socket, *args, ok=True):
            result = subprocess.run([str(BINARY), "last-session", str(store), str(socket), *map(str, args)], capture_output=True, text=True, timeout=15)
            assert (result.returncode == 0) == ok, result.stderr
            return json.loads(result.stdout) if ok else result.stderr

        assert run(a, "get") == {"revision": 0, "target": None}
        target = {"host_id": str(uuid.uuid4()), "thread_id": "same/会话:1", "cwd": "/example/目录 with spaces", "archived": True}
        saved = run(a, "set", 0, json.dumps(target))
        assert saved == {"revision": 1, "target": target}
        assert run(a, "get") == saved
        assert run(b, "get") == {"revision": 0, "target": None}
        assert "another window" in run(a, "set", 0, json.dumps(target), ok=False)
        assert run(a, "set", 1, json.dumps({**target, "cwd": "relative"}), ok=False)
        assert run(a, "get") == saved
        database = store / "creations.sqlite3"
        with sqlite3.connect(database) as db:
            db.execute("CREATE TRIGGER reject_last_session BEFORE UPDATE ON session_selection BEGIN SELECT RAISE(ABORT,'injected last-session write failure'); END")
        assert "injected last-session write failure" in run(a, "set", 1, json.dumps(target), ok=False)
        assert run(a, "get") == saved
        with sqlite3.connect(database) as db:
            db.execute("DROP TRIGGER reject_last_session")
            db.execute("INSERT INTO drafts VALUES('retained',7,'exact legacy draft')")
            db.execute("DROP TABLE workspace_names")
            db.execute("DROP TABLE pending_alert_cursors")
            db.execute("DROP TABLE completion_alert_cursors")
            db.execute("DROP TABLE notification_preferences")
            db.execute("DROP TABLE workspace_layout")
            db.execute("DROP TABLE session_selection")
            db.execute("PRAGMA user_version=12")
        assert run(a, "get") == {"revision": 0, "target": None}
        with sqlite3.connect(database) as db:
            assert db.execute("PRAGMA user_version").fetchone()[0] == 18
            assert db.execute("SELECT data FROM drafts").fetchone()[0] == "exact legacy draft"
        run(a, "set", 0, json.dumps(target))
        with sqlite3.connect(database) as db:
            db.execute("UPDATE session_selection SET data='corrupt'")
        assert run(a, "get", ok=False)
        assert run(a, "set", 1, json.dumps(target), ok=False)
        assert not a.exists() and not b.exists()
    print("PASS: exact Unicode session choice, endpoint isolation, stale/failed writes, schema-12 upgrade and corruption preservation; no Host started or contacted")


if __name__ == "__main__":
    main()
