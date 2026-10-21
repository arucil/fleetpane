#!/usr/bin/env python3
"""Reject invalid session directories at the Host boundary, before dispatch."""

import argparse
import json
import os
import signal
import socket
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"


def exercise(root, acp):
    root.mkdir()
    control = root / "private/control.sock"
    log = root / "host.log"
    arguments = [sys.executable, str(ROOT / "scripts" / ("acp-fixture.py" if acp else "provider-fixture.py"))]
    with log.open("w") as output:
        process = subprocess.Popen(
            [str(BINARY), "serve-acp" if acp else "serve", str(control), str(root), *arguments],
            stderr=output, stdout=subprocess.DEVNULL, start_new_session=True,
        )

    def raw(value):
        with socket.socket(socket.AF_UNIX) as stream:
            stream.settimeout(8)
            stream.connect(str(control))
            stream.sendall(json.dumps(value).encode() + b"\n")
            return json.loads(stream.makefile().readline())

    def snapshot():
        result = raw({"command": "snapshot"})
        assert result["response"] == "snapshot", result
        return result["data"]

    def send(identity, cwd):
        return raw({"command": "execute", "lease": lease, "operation_id": str(identity),
                    "method": "thread/start", "params": {"cwd": cwd, "historyMode": "paginated"}})

    try:
        deadline = time.monotonic() + 10
        while "ready:" not in log.read_text():
            assert process.poll() is None, log.read_text()
            assert time.monotonic() < deadline, "Host initialization deadline"
            time.sleep(0.025)
        before = snapshot()
        claim = raw({"command": "claim", "protocol": before["protocol"],
                     "host_id": before["host_id"], "expected_generation": 0,
                     "controller_id": str(uuid.uuid4())})
        assert claim["response"] == "lease", claim
        lease = claim["data"]
        source = root / "file.txt"
        source.write_text("not a directory")
        pipe = root / "pipe"
        os.mkfifo(pipe)
        (root / "broken").symlink_to(root / "absent")
        invalid = [str(root / "absent"), str(source), str(pipe), str(root / "broken"),
                   "relative", "", None, 42, str(root) + "\0bad", "/" + "a" * 4096]
        errors = []
        for cwd in invalid:
            response = send(uuid.uuid4(), cwd)
            assert response["response"] == "error", (cwd, response)
            assert response["data"]["code"] == "invalid_workspace_directory", response
            errors.append(response["data"]["message"])
        refused = snapshot()
        for key in ["host_id", "host_pid", "provider_pid", "owner", "sequence", "journal_total",
                    "operation_ids", "admission", "activity", "pending_requests", "fault"]:
            expected = lease if key == "owner" else before[key]
            assert refused[key] == expected, (key, refused[key], expected)

        workspace = root / "工作区 exact "
        workspace.mkdir()
        alias = root / "workspace-link"
        alias.symlink_to(workspace, target_is_directory=True)
        accepted = []
        for directory in [alias, workspace]:
            identity = uuid.uuid4()
            admitted = send(identity, str(directory))
            assert admitted["response"] == "operation", admitted
            deadline = time.monotonic() + 8
            while True:
                operation = raw({"command": "operation", "operation_id": str(identity)})["data"]
                if operation["outcome"]["state"] == "finished":
                    break
                assert time.monotonic() < deadline, "creation completion"
                time.sleep(0.025)
            thread = operation["outcome"]["response"]["result"]["thread"]
            assert thread["cwd"] == str(directory), thread
            total = snapshot()["journal_total"]
            directory.unlink() if directory.is_symlink() else directory.rmdir()
            # Saved receipt takes precedence over current filesystem observations.
            assert send(identity, str(directory))["data"] == operation
            assert snapshot()["journal_total"] == total
            accepted.append(thread)
        after = snapshot()
        assert after["fault"] is None and after["journal_total"] == 2, after
        assert all(before[key] == after[key] for key in ["host_id", "host_pid", "provider_pid"])
        return {"provider": "acp" if acp else "codex", "refusals": errors,
                "created_cwds": [item["cwd"] for item in accepted], "journal_total": 2,
                "duplicate_after_directory_removal": "exact saved receipt; no new dispatch",
                "same_processes": True, "fault": None}
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGINT)
        process.wait(timeout=10)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-dir", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="fp-create-directory-") as temporary:
        root = Path(temporary)
        results = [exercise(root / "codex", False), exercise(root / "acp", True)]
    if args.evidence_dir:
        args.evidence_dir.mkdir(parents=True, exist_ok=True)
        (args.evidence_dir / "process.json").write_text(json.dumps(results, ensure_ascii=False, indent=2) + "\n")
    print("PASS: Host directory validation for Codex/ACP, exact cwd, no rejected admission, dedup after removal")


if __name__ == "__main__":
    main()
