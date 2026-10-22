#!/usr/bin/env python3
"""Display-name CAS through public CLI; private Codex/ACP fixtures stay unchanged."""
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
    temporary = None if args.fixture else tempfile.TemporaryDirectory(prefix="fp-names-", dir="/tmp")
    root = args.fixture.resolve() if args.fixture else Path(temporary.name).resolve()
    root.mkdir(parents=True, exist_ok=True)
    processes, hosts = [], []
    keep = False
    try:
        _, _, config, command, snapshot = fixture["prepare"](root, processes, hosts)
        store = config.with_suffix(".creations")

        def call(*argv, ok=True):
            p = subprocess.run([str(BINARY), *map(str, argv)], capture_output=True, text=True, timeout=15)
            assert (p.returncode == 0) == ok, (argv, p.stdout, p.stderr)
            return json.loads(p.stdout) if ok else p.stderr

        def workspace(*argv, ok=True):
            return call("workspace", store, *argv, ok=ok)

        shared = root / "shared 中文"
        shared.mkdir()
        (shared / "notes.md").write_text("# Workspace names\n\nExact file stays here.\n")
        records = [workspace("add", config, host["name"], "folder", shared) for host in hosts]
        draft = "workspace rename 草稿\nkeep exact spaces  "
        threads, keys = [], []
        for i, record in enumerate(records):
            thread = command(i, "create", shared)["thread"]["id"]
            command(i, "send", thread, "[questions]" if i == 0 else "[permission]")
            fixture["wait"](lambda: any(r.get("params", {}).get("threadId") == thread or r.get("params", {}).get("sessionId") == thread for r in snapshot(i)["pending_requests"]))
            threads.append(thread)
            key = {"socket": hosts[i]["socket"], "host_id": snapshot(i)["host_id"], "thread_id": thread, "cwd": str(shared)}
            keys.append(key)
            call("draft", store, "edit", json.dumps(key), "0", draft)
            workspace("pin", record["id"], "on")
        # A name can be edited without a running Host or a current directory.
        offline = workspace("add", config, "Offline", "folder", shared)
        records.append(offline)
        setup = {"config": str(config), "store": str(store), "hosts": hosts, "workspaces": records, "threads": threads, "draft_keys": keys, "draft": draft, "pids": [p.pid for p in processes]}
        (root / "setup.json").write_text(json.dumps(setup, ensure_ascii=False, indent=2))
        if args.fixture:
            keep = True
            print(json.dumps({"fixture": str(root), "setup": str(root / "setup.json")}))
            return

        before = [snapshot(i) for i in range(2)]
        a, b = records[:2]
        assert not any(w["id"] == a["id"] for w in workspace("toggle-pin", a["id"]))
        assert next(w for w in workspace("toggle-pin", a["id"]) if w["id"] == a["id"])["name"] == a["name"]
        renamed = workspace("rename", a["id"], 0, "研究 / release 🚀")
        assert renamed["name"] == {"revision": 1, "value": "研究 / release 🚀"}
        assert {k: v for k, v in renamed.items() if k != "name"} == {k: v for k, v in a.items() if k != "name"}
        assert workspace("add", config, hosts[0]["name"], "folder", shared) == renamed
        assert next(w for w in workspace("pins") if w["id"] == a["id"])["name"] == renamed["name"]
        assert next(w for w in workspace("list") if w["id"] == b["id"])["name"] == {"revision": 0, "value": None}
        assert "changed in another window" in workspace("rename", a["id"], 0, "stale", ok=False)
        for invalid in ["", "  spaced", "line\nbreak", "x" * 257]:
            assert "workspace name must" in workspace("rename", a["id"], 1, invalid, ok=False)
        with sqlite3.connect(store / "creations.sqlite3") as db:
            db.execute("CREATE TRIGGER refuse_name BEFORE UPDATE ON workspace_names BEGIN SELECT RAISE(ABORT,'owned name refusal'); END")
        assert "owned name refusal" in workspace("reset-name", a["id"], 1, ok=False)
        with sqlite3.connect(store / "creations.sqlite3") as db:
            db.execute("DROP TRIGGER refuse_name")
        argv = [str(BINARY), "workspace", str(store), "rename", a["id"], "1"]
        contenders = [subprocess.Popen([*argv, name], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) for name in ["First", "Second"]]
        replies = [p.communicate(timeout=15) for p in contenders]
        assert sorted(p.returncode for p in contenders) == [0, 1], replies
        assert workspace("reset-name", a["id"], 2)["name"] == {"revision": 3, "value": None}
        moved = root / "moved 中文"
        shared.rename(moved)
        assert workspace("rename", offline["id"], 0, "Offline name")["name"]["value"] == "Offline name"
        assert [snapshot(i) for i in range(2)] == before
        assert (moved / "notes.md").read_text() == "# Workspace names\n\nExact file stays here.\n"
        for key in keys:
            saved = call("draft", store, "get", json.dumps(key))
            assert saved["text"] == draft and saved["revision"] == 1
        print(json.dumps({"exact_identity": True, "same_path_other_host_independent": True, "pin_and_register_keep_name": True, "concurrent_winners": 1, "stale_refused": True, "failed_write_rolled_back": True, "reset_revision": 3, "offline_missing_directory": True, "host_snapshots_unchanged": True, "exact_drafts": True}))
    finally:
        if not keep:
            for process in processes:
                if process.poll() is None:
                    process.send_signal(signal.SIGINT)
                    process.wait(timeout=10)
            if temporary:
                temporary.cleanup()


if __name__ == "__main__":
    main()
