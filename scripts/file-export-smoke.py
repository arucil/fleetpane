#!/usr/bin/env python3
"""Export current local workspace bytes without a Host or model connection."""
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
    with tempfile.TemporaryDirectory(prefix="fp-export-") as temporary:
        base = Path(temporary).resolve()
        work = base / "work"
        work.mkdir()
        output = base / "output"
        output.mkdir()
        config = base / "hosts.json"
        config.write_text(json.dumps({"version": 1, "hosts": [{"name": "Offline", "socket": str(base / "absent/control.sock") }]}))
        store = base / "state"

        def call(*arguments, ok=True):
            result = subprocess.run([str(BINARY), "workspace", str(store), *map(str, arguments)], capture_output=True, text=True, timeout=15, check=False)
            assert (result.returncode == 0) == ok, (arguments, result.stdout, result.stderr)
            return json.loads(result.stdout) if ok else result.stderr

        workspace = call("add", config, "Offline", "folder", work)
        identity = workspace["id"]
        original = bytes(range(256)) * 4097 + "原样\r\n trailing  \n".encode()
        source = work / "原始 %20#tag:2.bin"
        source.write_bytes(original)
        destination = output / "副本 %20#tag:2.bin"
        result = call("export", identity, source.name, destination)
        assert destination.read_bytes() == original
        assert source.read_bytes() == original
        assert result["sha256"] == hashlib.sha256(original).hexdigest()
        assert result["size"] == len(original)
        assert destination.stat().st_mode & 0o777 == 0o600
        source.write_bytes(b"current bytes differ from preview")
        refusals = {"existing": call("export", identity, source.name, destination, ok=False)}
        assert destination.read_bytes() == original
        assert call("export", identity, source.name, output / "fresh.bin")["size"] == source.stat().st_size
        (work / "empty").write_bytes(b"")
        assert call("export", identity, "empty", output / "empty")["size"] == 0
        os.mkfifo(work / "pipe")
        (work / "link").symlink_to(source)
        (output / "linked-parent").symlink_to(work, target_is_directory=True)
        with (work / "large").open("wb") as large:
            large.truncate(64 * 1024 * 1024 + 1)
        for name in ["pipe", "link", "large", "missing", "../outside", "/etc/passwd"]:
            refusals[name] = call("export", identity, name, output / "refused", ok=False)
            assert not (output / "refused").exists()
        refusals["parent link"] = call("export", identity, source.name, output / "linked-parent/export", ok=False)
        (output / "destination-link").symlink_to(source)
        refusals["destination link"] = call("export", identity, source.name, output / "destination-link", ok=False)
        assert source.read_bytes() == b"current bytes differ from preview"
        # Two independent CLI processes race to the same new destination.
        race = output / "race.bin"
        argv = [str(BINARY), "workspace", str(store), "export", identity, source.name, str(race)]
        contenders = [subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE) for _ in range(2)]
        outcomes = [process.communicate(timeout=15) for process in contenders]
        assert sorted(process.returncode for process in contenders) == [0, 1], outcomes
        assert race.read_bytes() == source.read_bytes()
        work.rename(base / "old-root")
        work.mkdir()
        (work / source.name).write_bytes(b"replacement")
        refusals["replaced root"] = call("export", identity, source.name, output / "replaced", ok=False)
        assert not (output / "replaced").exists()
        assert not list(output.glob(".fleetpane-export-*.tmp"))
        receipt = {"offline": True, "raw_bytes": len(original), "raw_sha256": result["sha256"], "mode": "0600", "concurrent_publish": "one success, one refusal", "empty_and_fresh_source": True, "refusals": refusals, "binary_sha256": hashlib.sha256(BINARY.read_bytes()).hexdigest()}
        if args.evidence_dir:
            args.evidence_dir.mkdir(parents=True, exist_ok=True)
            (args.evidence_dir / "process.json").write_text(json.dumps(receipt, indent=2) + "\n")
        print("PASS: exact raw export, private copy, fresh/empty bytes, no overwrite, concurrent publish and workspace/link/FIFO/size refusal")


if __name__ == "__main__":
    main()
