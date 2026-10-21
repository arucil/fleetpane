#!/usr/bin/env python3
"""Shared CLI file/directory navigation works without a provider connection."""
import argparse
import hashlib
import json
import os
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-dir", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="fp-navigation-") as temporary:
        root = Path(temporary)
        folder = root / "work"
        folder.mkdir()
        (folder / "目录").mkdir()
        source = folder / "目录/file space:2.txt"
        exact = "one\r\n二行  \n" + "中" * 20_000
        source.write_text(exact)
        (folder / "outside").symlink_to(root, target_is_directory=True)
        os.mkfifo(folder / "pipe")
        config = root / "hosts.json"
        config.write_text(json.dumps({"version": 1, "hosts": [{"name": "Offline", "socket": str(root / "unavailable/control.sock") }]}))
        store = root / "state"

        def call(*arguments, ok=True):
            result = subprocess.run([str(BINARY), "workspace", str(store), *map(str, arguments)], capture_output=True, text=True, timeout=15)
            assert (result.returncode == 0) == ok, (arguments, result.stderr)
            return json.loads(result.stdout) if ok else result.stderr

        workspace = call("add", config, "Offline", "folder", folder)
        for path in [source.relative_to(folder), source, "./目录/file space:2.txt"]:
            opened = call("open", workspace["id"], path)
            assert opened["kind"] == "file"
            assert opened["data"]["content"]["text"] == exact
        assert call("open", workspace["id"], "目录")["kind"] == "directory"
        assert call("open", workspace["id"], folder)["data"]["path"] == ""
        refusals = {path: call("open", workspace["id"], path, ok=False) for path in ["../work", "/etc/passwd", "outside", "outside/hosts.json", "pipe", "missing"]}
        folder.rename(root / "old-root")
        folder.mkdir()
        refusals["replaced root"] = call("open", workspace["id"], "", ok=False)
        receipt = {"offline_endpoint": True, "exact_bytes": len(exact.encode()), "exact_sha256": hashlib.sha256(exact.encode()).hexdigest(), "refusals": refusals, "host_binary_sha256": hashlib.sha256(BINARY.read_bytes()).hexdigest()}
        if args.evidence_dir:
            args.evidence_dir.mkdir(parents=True, exist_ok=True)
            (args.evidence_dir / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
        print("PASS: offline typed file/directory navigation, exact Unicode/CRLF, traversal/link/FIFO/missing/root identity refusal")


if __name__ == "__main__":
    main()
