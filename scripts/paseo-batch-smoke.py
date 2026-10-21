#!/usr/bin/env python3
"""Synthetic batch migration through the public plan/batch CLI."""
import argparse
import hashlib
import importlib.util
import json
import sqlite3
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / "target/debug/fleetpane-host"


def fixture(root, large=False):
    spec = importlib.util.spec_from_file_location("single", ROOT / "scripts/paseo-import-smoke.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    home, codex, dsh, config = module.fixture(root)
    path = home / "projects/workspaces.json"
    workspaces = json.loads(path.read_text())
    workspaces.append({"workspaceId": "missing", "projectId": "project-one", "cwd": str(root / "missing-directory"), "kind": "directory", "displayName": "Missing directory"})
    (home / "agents/project/missing.json").write_text(json.dumps({"id": "missing-log", "provider": "codex", "cwd": str(root / "missing-directory"), "workspaceId": "missing", "persistence": {"provider": "codex", "sessionId": "missing-log"}}))
    if large:
        cwd = root / "large-workspace"
        cwd.mkdir()
        workspaces.insert(0, {"workspaceId": "large", "projectId": "project-one", "cwd": str(cwd), "kind": "directory", "displayName": "Large batch for Stop"})
        with sqlite3.connect(codex / "state_5.sqlite") as db:
            for n in range(12):
                identity = f"large-{n:02d}"
                (home / f"agents/project/{identity}.json").write_text(json.dumps({"id": identity, "provider": "codex", "cwd": str(cwd), "workspaceId": "large", "title": identity, "persistence": {"provider": "codex", "sessionId": identity}}))
                log = codex / f"{identity}.jsonl"
                with log.open("w") as output:
                    output.write(json.dumps({"type": "session_meta", "payload": {"id": identity, "cwd": str(cwd)}}) + "\n")
                    for i in range(60000):
                        output.write(json.dumps({"type": "response_item", "payload": {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": f"Copied large history {identity} item {i} **durable**"}]}}) + "\n")
                db.execute("INSERT INTO threads VALUES(?,?)", (identity, str(log)))
    path.write_text(json.dumps(workspaces, ensure_ascii=False))
    return home, codex, dsh, config


def check(root):
    home, codex, dsh, config = fixture(root)
    def command(*args):
        p = subprocess.run([str(BIN), "paseo-import", *map(str, args)], capture_output=True, text=True, timeout=40)
        assert p.returncode == 0, p.stderr
        return json.loads(p.stdout)
    def hashes():
        return {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest() for base in (home, codex, dsh) for p in base.rglob("*") if p.is_file()}
    original = hashes()
    plan = command("plan", home, config, "Migration QA", codex, dsh)
    assert len(plan["workspaces"]) == 2 and "SYNTHETIC_DO_NOT_COPY" not in json.dumps(plan)
    path = root / "plan.json"
    path.write_text(json.dumps(plan))
    store = root / "batch-controller"
    first = command("batch", store, path)
    assert first["progress"]["workspaces_done"] == 2 and not first["cancelled"]
    assert first["progress"]["histories_available"] == 2 and first["progress"]["histories_failed"] == 2
    assert next(r for r in first["results"] if r["source_id"] == "missing")["registration_error"]
    assert all(r["histories_finished"] for r in first["results"])
    second = command("batch", store, path)
    assert all(r["reused"] for r in second["results"])
    assert [r["imported_id"] for r in first["results"]] == [r["imported_id"] for r in second["results"]]
    assert hashes() == original and not (root / "unstarted.sock").exists()
    # A selected source change is an explicit failure, independent of successes.
    source = home / "projects/workspaces.json"
    records = json.loads(source.read_text())
    records[0]["title"] = "Changed since plan"
    source.write_text(json.dumps(records))
    stale = command("batch", root / "stale-controller", path)
    assert stale["results"][0]["error"] and stale["results"][0]["imported_id"] is None
    assert stale["results"][1]["imported_id"]
    assert len(command("list", root / "stale-controller")) == 1
    print(json.dumps({"batch_workspace_and_history": "PASS", "repeat_reuses_identities_and_snapshots": "PASS", "missing_directory_and_history_visible": "PASS", "stale_selection_independent_failure": "PASS", "source_unchanged": "PASS", "no_host_required": "PASS"}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture", type=Path)
    parser.add_argument("--large", action="store_true")
    args = parser.parse_args()
    if args.fixture:
        fixture(args.fixture.resolve(), args.large)
        print(args.fixture.resolve())
    else:
        with tempfile.TemporaryDirectory(prefix="fleetpane-paseo-batch-") as directory:
            check(Path(directory))
