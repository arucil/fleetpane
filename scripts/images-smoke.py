#!/usr/bin/env python3
"""Local immutable images, exact provider payloads and conservative draft recovery."""

import hashlib
import json
import os
import random
import signal
import socket
import sqlite3
import struct
import subprocess
import tempfile
import time
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"


def png(width, height, large=False):
    def chunk(kind, data):
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data))
        )

    pixels = (
        random.Random(123).randbytes(width * height * 3)
        if large
        else bytes([180, 40, 80]) * width * height
    )
    scanlines = b"".join(
        b"\0" + pixels[y * width * 3 : (y + 1) * width * 3] for y in range(height)
    )
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(scanlines))
        + chunk(b"IEND", b"")
    )


def wait(probe):
    deadline = time.monotonic() + 15
    while True:
        result = probe()
        if result:
            return result
        assert time.monotonic() < deadline, "image fixture deadline"
        time.sleep(0.025)


def exercise(root, acp, supported):
    root.mkdir()
    control = root / "host/control.sock"
    local = root / "drafts"
    process = None
    generation = 0

    def raw(message):
        with socket.socket(socket.AF_UNIX) as stream:
            stream.settimeout(10)
            stream.connect(str(control))
            stream.sendall((json.dumps(message) + "\n").encode())
            return json.loads(stream.makefile().readline(1024 * 1024))

    def snapshot():
        state = raw({"command": "snapshot"})["data"]
        assert state["fault"] is None, state["fault"]
        return state

    def run(*args, ok=True):
        result = subprocess.run(
            [str(BINARY), *map(str, args)],
            check=False,
            capture_output=True,
            text=True,
            timeout=25,
        )
        assert (result.returncode == 0) == ok, (args[:3], result.stderr[-1000:])
        return json.loads(result.stdout) if ok else result.stderr

    def session(*args):
        return run("session", control, "--take-control", *args)["result"]

    def draft(*args, ok=True):
        return run("draft", local, *args, ok=ok)

    def start():
        nonlocal process, generation
        generation += 1
        log = root / f"host-{generation}.log"
        provider = (
            ["python3", str(ROOT / "scripts/acp-fixture.py")]
            if acp
            else [
                "python3",
                str(ROOT / "scripts/provider-fixture.py"),
                "--send-effects",
                str(root / "wire.jsonl"),
            ]
        )
        with log.open("w") as output:
            process = subprocess.Popen(
                [
                    str(BINARY),
                    "serve-acp" if acp else "serve",
                    str(control),
                    str(root),
                    *provider,
                ],
                stdout=subprocess.DEVNULL,
                stderr=output,
                start_new_session=True,
                env={
                    **os.environ,
                    "FLEETPANE_FIXTURE_IMAGES": "1" if supported else "0",
                },
            )
        wait(
            lambda: (
                process.poll() is not None
                or (control.exists() and "ready:" in log.read_text())
            )
        )
        assert process.poll() is None, log.read_text()

    def stop():
        if process is not None and process.poll() is None:
            os.killpg(process.pid, signal.SIGINT)
            process.wait(timeout=10)

    def provider_prompts():
        if acp:
            return json.loads((root / "fixture-acp-sessions.json").read_text())[
                identity
            ]["prompts"]
        return [
            json.loads(line) for line in (root / "wire.jsonl").read_text().splitlines()
        ]

    try:
        start()
        identity = session("create", root)["thread"]["id"]
        state = snapshot()
        key = json.dumps(
            {
                "socket": str(control),
                "host_id": state["host_id"],
                "thread_id": identity,
                "cwd": str(root),
            }
        )
        source = root / "screenshot.wrong-extension"
        original = png(7, 5)
        source.write_bytes(original)
        saved = draft("attach", key, 0, source)
        assert saved["images"][0]["mime_type"] == "image/png" and saved["text"] == ""
        image = saved["images"][0]
        assert image["sha256"] == hashlib.sha256(original).hexdigest()
        # Failed multi-selection must not partially change a saved draft.
        draft("attach", key, saved["revision"], source, ok=False)
        assert draft("get", key) == saved
        source.unlink()
        assert draft("get", key) == saved
        if not supported:
            error = draft("send", key, saved["revision"], "--take-control", ok=False)
            assert "unsupported_images" in error
            restored = draft("get", key)
            assert restored["images"] == saved["images"] and restored["attempt"] is None
            assert provider_prompts() == []
            snapshot()
            return {
                "provider": "acp-without-image-capability",
                "unsupported_preserved": True,
            }

        draft("send", key, saved["revision"], "--take-control")
        wait(lambda: len(provider_prompts()) == 1)
        cleared = draft("get", key)
        assert (
            cleared["text"] == ""
            and "images" not in cleared
            and cleared["attempt"] is None
        )
        wait(lambda: not snapshot()["activity"]["turns"])
        opened = session("open", identity)["thread"]
        user = next(
            item
            for item in opened["turns"][0]["items"]
            if item["type"] == "userMessage"
        )
        assert len(user["content"]) == 1 and user["content"][0]["type"] == "localImage"
        asset = Path(user["content"][0]["path"])
        assert asset.read_bytes() == original

        db = sqlite3.connect(Path(str(control) + ".journal") / "operations.sqlite3")
        operation, params = db.execute(
            "SELECT id,params FROM operations WHERE method='turn/start'"
        ).fetchone()
        params = json.loads(params)
        assert (
            params["input"][0]["type"] == "fleetpaneImage"
            and len(json.dumps(params)) < 4096
        )
        db.close()
        # A recorded receipt is authoritative even if its source asset later vanishes.
        asset.unlink()
        duplicate = raw(
            {
                "command": "execute",
                "lease": snapshot()["owner"],
                "operation_id": operation,
                "method": "turn/start",
                "params": params,
            }
        )
        assert duplicate["response"] == "operation"
        assert len(provider_prompts()) == 1
        source.write_bytes(original)
        saved = draft("attach", key, cleared["revision"], source)
        asset.chmod(0o600)
        asset.write_bytes(b"tampered")
        asset.chmod(0o400)
        error = draft("send", key, saved["revision"], "--take-control", ok=False)
        assert "invalid_images" in error
        failed = draft("get", key)
        assert failed["images"] == saved["images"] and failed["attempt"] is None
        assert len(provider_prompts()) == 1
        snapshot()
        asset.chmod(0o600)
        asset.write_bytes(original)
        asset.chmod(0o400)
        # Explicit removal and text-only editing preserve the same revision contract.
        removed = draft("remove-image", key, failed["revision"], 0)
        assert "images" not in removed
        large = root / "large.png"
        data = png(1024, 1024, True)
        assert len(data) > 1024 * 1024
        large.write_bytes(data)
        saved = draft("attach", key, removed["revision"], large)
        text = "  中文\n  preserve trailing spaces  "
        saved = draft("edit", key, saved["revision"], text)
        assert len(saved["images"]) == 1
        draft("send", key, saved["revision"], "--take-control")
        wait(lambda: len(provider_prompts()) == 2)
        wait(lambda: not snapshot()["activity"]["turns"])
        if acp:
            observed = json.loads((root / "fixture-acp-sessions.json").read_text())[
                identity
            ]
            assert observed["prompts"] == ["", text]
            assert observed["imagePrompts"] == [
                [
                    {
                        "sha256": image["sha256"],
                        "bytes": len(original),
                        "mimeType": "image/png",
                    }
                ],
                [
                    {
                        "sha256": hashlib.sha256(data).hexdigest(),
                        "bytes": len(data),
                        "mimeType": "image/png",
                    }
                ],
            ]
            stop()
            start()
            reopened = session("open", identity)["thread"]
            assert len(reopened["turns"]) == 2
            for turn in reopened["turns"]:
                user = next(
                    item for item in turn["items"] if item["type"] == "userMessage"
                )
                path = next(
                    part["path"]
                    for part in user["content"]
                    if part["type"] == "localImage"
                )
                assert Path(path).exists()
        else:
            sent = provider_prompts()
            assert sent[0]["params"]["input"] == [
                {"type": "localImage", "path": str(asset)}
            ]
            assert sent[1]["params"]["input"][0] == {"type": "text", "text": text}
            assert Path(sent[1]["params"]["input"][1]["path"]).read_bytes() == data
        return {
            "provider": "acp" if acp else "codex-fixture",
            "image_only": True,
            "large_image_bytes": len(data),
            "one_execution_per_uuid": True,
            "tamper_refused_without_fault": True,
            "exact_text": True,
            "archive_reopen": acp,
        }
    finally:
        stop()


def main():
    with tempfile.TemporaryDirectory(prefix="fp-images-") as temporary:
        root = Path(temporary)
        results = [
            exercise(root / name, acp, supported)
            for name, acp, supported in [
                ("acp", True, True),
                ("codex", False, True),
                ("unsupported", True, False),
            ]
        ]
        print(json.dumps({"passed": True, "cases": results}))


if __name__ == "__main__":
    main()
