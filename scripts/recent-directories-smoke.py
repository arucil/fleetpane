#!/usr/bin/env python3
"""Exercise device-local recent directory CLI; deliberately absent Host sockets."""
import json
import sqlite3
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

def main():
    with tempfile.TemporaryDirectory(prefix="fleetpane-recent-") as directory:
        root = Path(directory)
        store = root / "controller"
        first, second = root / "missing-a.sock", root / "missing-b.sock"
        def command(*args, ok=True):
            result = subprocess.run([str(ROOT / "target/debug/fleetpane-host"), "recent", str(store), *map(str,args)],capture_output=True,text=True,timeout=8)
            assert (result.returncode == 0) == ok, result.stderr
            return json.loads(result.stdout) if ok else result.stderr
        exact = str(root / "中文 directory\nwith trailing space ")
        assert command("remember",first,exact) == [exact]
        command("remember",first,root/"two")
        command("remember",first,root/"three")
        command("remember",first,root/"four")
        assert command("remember",first,root/"two") == [str(root/"two"),str(root/"four"),str(root/"three")]
        assert command("remember",second,exact) == [exact]
        assert "absolute" in command("remember",first,"relative",ok=False)
        assert command("list",second) == [exact]
        assert command("clear",first) == []
        assert command("list",second) == [exact]
        database = store / "creations.sqlite3"
        with sqlite3.connect(database) as connection:
            assert connection.execute("PRAGMA user_version").fetchone()[0] == 18
            connection.execute("UPDATE recent_directories SET cwd='broken-relative' WHERE socket=?",(str(second),))
        assert "absolute" in command("list",second,ok=False)
        assert "absolute" in command("remember",second,root/"new",ok=False)
        with sqlite3.connect(database) as connection:
            assert connection.execute("SELECT cwd FROM recent_directories").fetchone()[0] == "broken-relative"
            assert connection.execute("SELECT count(*) FROM creations").fetchone()[0] == 0
            assert connection.execute("SELECT count(*) FROM workspaces").fetchone()[0] == 0
        assert command("clear",second) == []
        assert not first.exists() and not second.exists()
        print(json.dumps({"cli_mru_round_trip": "PASS", "exact_unicode_and_host_isolation": "PASS", "corruption_visible_without_overwrite": "PASS", "no_host_or_session_needed": "PASS"}))

if __name__ == "__main__":
    main()
