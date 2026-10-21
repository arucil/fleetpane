#!/usr/bin/env python3
"""Offline workspace images: exact observed bytes, explicit limits and path confinement."""
import argparse
import base64
import hashlib
import importlib.util
import json
import os
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"
SPEC = importlib.util.spec_from_file_location("image_fixtures", ROOT / "scripts/images-smoke.py")
fixtures = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fixtures)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-dir", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="fp-file-images-") as temporary:
        root = Path(temporary).resolve()
        folder = root / "work 中文"
        folder.mkdir()
        original = fixtures.png(640, 320)
        (folder / "图.data").write_bytes(original)
        (folder / "corrupt.png").write_bytes(b"corrupt image")
        (folder / "truncated").write_bytes(original[:35])
        (folder / "unsupported.gif").write_bytes(b"GIF89a")
        (folder / "huge.png").write_bytes(fixtures.png(8193, 1))
        (folder / "large.png").write_bytes(original + bytes(8 * 1024 * 1024))
        (folder / "large.txt").write_bytes(bytes([65]) * (256 * 1024 + 1))
        (folder / "notes.txt").write_bytes("中文\r\nexact  \n".encode())
        (root / "outside.png").write_bytes(original)
        (folder / "link.png").symlink_to(root / "outside.png")
        os.mkfifo(folder / "pipe.png")
        config = root / "hosts.json"
        config.write_text(json.dumps({"version": 1, "hosts": [{"name": "Offline", "socket": str(root / "absent.sock")}]}))
        store = root / "state"

        def call(*arguments, ok=True):
            result = subprocess.run([str(BINARY), "workspace", str(store), *map(str, arguments)], capture_output=True, text=True, timeout=15)
            assert (result.returncode == 0) == ok, (arguments, result.stderr)
            return json.loads(result.stdout) if ok else result.stderr

        workspace = call("add", config, "Offline", "folder", folder)
        identity = workspace["id"]
        for verb, target in [("read", "图.data"), ("open", folder / "图.data")]:
            result = call(verb, identity, target)
            if verb == "open":
                assert result["kind"] == "file"
                result = result["data"]
            content = result["content"]
            assert content["kind"] == "image", content
            assert base64.b64decode(content["data_base64"], validate=True) == original
            image = content["image"]
            assert (image["width"], image["height"], image["mime_type"]) == (640, 320, "image/png")
            assert image["sha256"] == hashlib.sha256(original).hexdigest()
        notices = {name: call("read", identity, name)["content"] for name in ["corrupt.png", "truncated", "unsupported.gif", "huge.png", "large.png", "large.txt"]}
        for name in ["corrupt.png", "truncated", "unsupported.gif", "huge.png"]:
            assert notices[name]["kind"] == "unavailable" and notices[name]["reason"]
        assert notices["large.png"] == {"kind": "too_large", "limit": 8 * 1024 * 1024}
        assert notices["large.txt"] == {"kind": "too_large", "limit": 256 * 1024}
        assert call("read", identity, "notes.txt")["content"] == {"kind": "text", "text": "中文\r\nexact  \n"}
        refusals = {path: call("open", identity, path, ok=False) for path in ["link.png", "pipe.png", "../outside.png", root / "outside.png"]}
        replacement = fixtures.png(320, 640)
        (folder / "图.data").write_bytes(replacement)
        fresh = call("read", identity, "图.data")["content"]
        assert fresh["image"]["height"] == 640
        assert base64.b64decode(fresh["data_base64"], validate=True) == replacement
        assert (folder / "图.data").read_bytes() == replacement
        (folder / "图.data").unlink()
        assert "Workspace files" in call("read", identity, "图.data", ok=False)
        folder.rename(root / "kept")
        folder.mkdir()
        (folder / "new.png").write_bytes(original)
        assert "replaced" in call("read", identity, "new.png", ok=False)
        receipt = {"offline": True, "exact_bytes": len(original), "exact_sha256": hashlib.sha256(original).hexdigest(), "notices": notices, "refusals": {str(path): error for path, error in refusals.items()}, "reload_preserved": True, "delete_rejected": True, "replaced_root_rejected": True, "host_binary_sha256": hashlib.sha256(BINARY.read_bytes()).hexdigest()}
        if args.evidence_dir:
            args.evidence_dir.mkdir(parents=True, exist_ok=True)
            (args.evidence_dir / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
        print("PASS: offline exact image bytes/identity, reload/delete, explicit corrupt/unsupported/size/pixel notices, confined no-follow reads")


if __name__ == "__main__":
    main()
