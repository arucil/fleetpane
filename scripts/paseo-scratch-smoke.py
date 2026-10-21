#!/usr/bin/env python3
"""Actual paused/killed copy and explicit bounded scratch collection."""
import argparse
import fcntl
import hashlib
import importlib.util
import json
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def hashes(directory):
    return {str(p.relative_to(directory)): hashlib.sha256(p.read_bytes()).hexdigest() for p in directory.rglob("*") if p.is_file()}


def check(root):
    spec = importlib.util.spec_from_file_location("receipts", ROOT / "scripts/paseo-receipts-smoke.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.check(root, check_scratch=True)
    command = module.command
    store = root / "hosts.creations"
    scratch = store / "paseo-scratch-v1"
    # Simulate a previous version's unowned temporary files: they cannot be
    # safely collected using the new lock, even though no new writer is active.
    for directory in (store / "paseo-history", store / "paseo-forks"):
        directory.mkdir(mode=0o700, exist_ok=True)
        (directory / "legacy.tmp").write_bytes(b"SYNTHETIC_UNOWNED_OLD_COPY")
    stable = {name: hashes(root / name) for name in ("paseo", "codex", "dsh")}
    published = {name: hashes(store / name) for name in ("paseo-history", "paseo-forks")}
    before = hashes(scratch)
    assert before, "SIGKILL left abandoned scratch files"
    status = command("scratch-status", store)
    assert status["files"] == len(before) and status["bytes"] > 0
    assert status["legacy_files_retained"] == 2 and not status["removed"]
    assert hashes(scratch) == before
    # A lock-owning external process also fences both operations.
    with (store / "paseo-scratch-v1.lock").open("rb") as lock:
        fcntl.flock(lock, fcntl.LOCK_SH)
        assert "active" in command("clean-scratch", store, ok=False)
    assert hashes(scratch) == before
    # An unexpected entry blocks the entire preflight, without partial removal.
    unexpected = scratch / "unexpected.txt"
    unexpected.write_bytes(b"DO_NOT_DELETE")
    assert "unrecognized" in command("clean-scratch", store, ok=False)
    assert unexpected.read_bytes() == b"DO_NOT_DELETE"
    unexpected.unlink()
    assert hashes(scratch) == before
    removed = command("clean-scratch", store)
    assert removed["removed"] and removed["files"] == status["files"] and removed["bytes"] == status["bytes"]
    assert not hashes(scratch) and command("scratch-status", store)["files"] == 0
    assert command("clean-scratch", store)["files"] == 0
    assert {name: hashes(store / name) for name in published} == published
    assert {name: hashes(root / name) for name in stable} == stable
    sessions = command("sessions", store, command("list", store)[0]["id"])
    for session in sessions:
        if session["agent"]["provider"] != "claude":
            assert command("history", store, session["id"], 0)["status"]["rows"] > 0
    result = {"paused_writer_retained": True, "killed_copy_collected": True, "read_only_size_preview": True, "unexpected_entry_no_partial_deletion": True, "repeat_collection_empty": True, "legacy_files_retained": True, "published_histories_readable_and_unchanged": True, "source_unchanged": True, "status": status, "removed": removed}
    (root / "scratch-receipt.json").write_text(json.dumps(result, indent=2))
    print(json.dumps(result))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--keep", type=Path)
    args = parser.parse_args()
    if args.keep:
        check(args.keep.resolve())
    else:
        with tempfile.TemporaryDirectory(prefix="fp-paseo-scratch-") as directory:
            check(Path(directory))
