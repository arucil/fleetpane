#!/usr/bin/env python3
"""Device-local appearance persistence and faults through the shared CLI."""
import json
import sqlite3
import stat
import subprocess
import tempfile
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main():
    binary = ROOT / "target/debug/fleetpane-host"
    with tempfile.TemporaryDirectory(prefix="fleetpane-appearance-") as directory:
        root = Path(directory)
        store = root / "preferences"

        def run(*args):
            return subprocess.run([str(binary), *map(str, args)],
                                  text=True, capture_output=True, timeout=12)

        def command(*args):
            result = run(*args)
            assert result.returncode == 0, result.stderr
            return json.loads(result.stdout)

        def appearance(*args):
            return command("appearance", store, *args)

        assert appearance("get") == {"revision": 0, "value": {"theme": "system"}}
        first = appearance("set", 0, "dark")
        assert first == {"revision": 1, "value": {"theme": "dark"}}
        assert appearance("get") == first
        stale = run("appearance", store, "set", 0, "light")
        assert stale.returncode != 0 and "another window" in stale.stderr
        for bad in ["future", "Dark", ""]:
            assert run("appearance", store, "set", 1, bad).returncode != 0
        assert appearance("get") == first
        database = store / "creations.sqlite3"
        with sqlite3.connect(database) as db:
            db.execute("CREATE TRIGGER reject_appearance BEFORE UPDATE ON appearance BEGIN SELECT RAISE(ABORT,'injected appearance write failure'); END")
        failed = run("appearance", store, "set", 1, "system")
        assert failed.returncode != 0 and "injected appearance write failure" in failed.stderr
        assert appearance("get") == first
        with sqlite3.connect(database) as db:
            db.execute("DROP TRIGGER reject_appearance")
        second = appearance("set", 1, "system")
        assert second == {"revision": 2, "value": {"theme": "system"}}

        key = json.dumps({"socket": str(root / "unstarted.sock"),
                          "host_id": str(uuid.uuid4()), "thread_id": "synthetic",
                          "cwd": "/synthetic/workspace"})
        draft = command("draft", store, "edit", key, 0, "保留草稿\nexact text")
        with sqlite3.connect(database) as db:
            db.execute("DROP TABLE session_selection")
            db.execute("DROP TABLE attention_reviews")
            db.execute("DROP TABLE host_selection")
            db.execute("DROP TABLE paseo_batch_results")
            db.execute("DROP TABLE paseo_batches")
            db.execute("DROP TABLE paseo_continuations")
            db.execute("DROP TABLE paseo_sessions")
            db.execute("DROP TABLE paseo_workspaces")
            db.execute("DROP TABLE recent_directories")
            db.execute("DROP TABLE appearance")
            db.execute("PRAGMA user_version=5")
        assert appearance("get") == {"revision": 0, "value": {"theme": "system"}}
        assert command("draft", store, "get", key) == draft
        with sqlite3.connect(database) as db:
            assert db.execute("PRAGMA user_version").fetchone()[0] == 13
        appearance("set", 0, "light")
        corrupt = '{"theme":"unrecognized"}'
        with sqlite3.connect(database) as db:
            db.execute("UPDATE appearance SET data=?", [corrupt])
        assert run("appearance", store, "get").returncode != 0
        assert run("appearance", store, "set", 1, "dark").returncode != 0
        with sqlite3.connect(database) as db:
            assert db.execute("SELECT data FROM appearance").fetchone()[0] == corrupt
            db.execute("DROP TABLE appearance")
        assert run("appearance", store, "get").returncode != 0
        alias = root / "alias"
        alias.symlink_to(store, target_is_directory=True)
        assert run("appearance", alias, "get").returncode != 0
        assert stat.S_IMODE(store.stat().st_mode) == 0o700
        assert stat.S_IMODE(database.stat().st_mode) == 0o600
        assert not (root / "unstarted.sock").exists()
        print(json.dumps({
            "provider_process_started": False,
            "cli_round_trip_and_reopen": "PASS",
            "stale_writer_refused": "PASS",
            "failed_commit_keeps_saved_preference": "PASS",
            "schema_5_upgrade_preserves_exact_draft": "PASS",
            "corrupt_unknown_or_missing_current_data_not_defaulted": "PASS",
            "private_permissions_and_symlink_refusal": "PASS",
        }))


if __name__ == "__main__":
    main()
