#!/usr/bin/env python3
"""Exact CLI prompt sources against a private Host and synthetic provider."""

import argparse
import hashlib
import json
import os
import signal
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"


def snapshot(endpoint):
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(5)
        stream.connect(str(endpoint))
        stream.sendall(b'{"command":"snapshot"}\n')
        return json.loads(stream.makefile("rb").readline(1024 * 1024))["data"]


def wait(probe):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        if result := probe():
            return result
        time.sleep(.025)
    raise AssertionError("prompt fixture deadline")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-dir", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="fp-prompt-", dir="/tmp") as temporary:
        root = Path(temporary)
        endpoint = root / "host/control.sock"
        effects = root / "sends.jsonl"
        log = root / "host.log"

        def session(*arguments, data=None, ok=True, target=endpoint):
            result = subprocess.run(
                [str(BINARY), "session", str(target), "--take-control", *arguments],
                input=data, capture_output=True, timeout=15,
            )
            assert (result.returncode == 0) == ok, (arguments, result.stderr)
            return json.loads(result.stdout) if ok else result.stderr.decode()

        with log.open("w") as output:
            process = subprocess.Popen(
                [str(BINARY), "serve", str(endpoint), str(root), sys.executable,
                 str(ROOT / "scripts/provider-fixture.py"), "--send-effects", str(effects)],
                stdout=subprocess.DEVNULL, stderr=output, start_new_session=True,
            )
        try:
            wait(lambda: process.poll() is not None or "ready:" in log.read_text())
            assert process.poll() is None, log.read_text()
            thread = session("create", str(root))["result"]["thread"]["id"]
            prompt = "  原文中文\r\n    keep indentation\ntrailing spaces  \n"
            path = root / "prompt with spaces.txt"
            path.write_bytes(prompt.encode())
            invalid_utf8 = root / "invalid.txt"
            invalid_utf8.write_bytes(b"\xff")
            oversized = root / "large.txt"
            oversized.write_bytes(b"x" * 65537)
            fifo = root / "pipe"
            os.mkfifo(fifo)
            cases = [
                ([], None),
                (["--stdin", "--prompt-file", str(path)], b"unused"),
                (["--prompt-file", str(root)], None),
                (["--prompt-file", str(fifo)], None),
                (["--prompt-file", str(invalid_utf8)], None),
                (["--prompt-file", str(oversized)], None),
                (["--prompt-file", str(root / "missing")], None),
                (["--stdin"], b"\xff"),
                (["--stdin"], b"x" * 65537),
                (["--stdin"], b" \r\n\t"),
                (["--stdin"], b""),
                (["one", "two"], None),
                (["--prompt"], None),
                (["--unknown"], None),
                (["--stdin", "--model", "a", "--model", "b"], b"unused"),
            ]
            before = snapshot(endpoint)
            for flags, data in cases:
                message = session("send", thread, *flags, data=data, ok=False)
                assert "Host already" not in message and "host transport" not in message
            after = snapshot(endpoint)
            assert before == after, "invalid input contacted Host or replaced controller"
            assert not effects.exists(), "invalid input reached provider"
            message = session("steer", thread, "turn", "--stdin", data=b" ", ok=False,
                              target=root / "absent.sock")
            assert "non-whitespace" in message and "host transport" not in message

            sent = []
            for flags, data, expected in [
                (["--prompt-file", str(path)], None, prompt),
                (["--stdin"], prompt.encode(), prompt),
                (["--prompt", prompt], None, prompt),
                ([prompt], None, prompt),
                (["--", "--literal leading dash\n"], None, "--literal leading dash\n"),
                (["--stdin"], b"x" * 65536, "x" * 65536),
            ]:
                turn = session("send", thread, *flags, data=data)["result"]["turn"]["id"]
                wait(lambda: not any(t["turn_id"] == turn
                                     for t in snapshot(endpoint)["activity"]["turns"]))
                history = session("open", thread)["result"]["thread"]["turns"]
                user = next(i for t in history if t["id"] == turn
                            for i in t["items"] if i["type"] == "userMessage")
                assert user["content"] == [{"type": "text", "text": expected}]
                sent.append({"bytes": len(expected.encode()), "sha256": hashlib.sha256(expected.encode()).hexdigest()})
            running = session("send", thread, "[wait]")["result"]["turn"]["id"]
            session("steer", thread, running, "--prompt-file", str(path))
            history = session("open", thread)["result"]["thread"]["turns"]
            last = next(t for t in history if t["id"] == running)
            assert any(i.get("content") == [{"type": "text", "text": prompt}] for i in last["items"])
            session("stop", thread, running)
            assert len(effects.read_text().splitlines()) == 7
            receipt = {
                "binary_sha256": hashlib.sha256(BINARY.read_bytes()).hexdigest(),
                "invalid_sources_refused": len(cases), "fifo_without_writer_refused": True,
                "invalid_input_kept_host_snapshot": before == after,
                "exact_prompt_sends": sent, "exact_file_followup": True,
                "provider_start_effects": 7, "provider": "synthetic Codex fixture",
                "limits": "No GUI, external model, macOS or Windows acceptance.",
            }
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGINT)
                process.wait(timeout=10)
        receipt["owned_host_stopped"] = not endpoint.exists()
        assert receipt["owned_host_stopped"]
        if args.evidence_dir:
            args.evidence_dir.mkdir(parents=True, exist_ok=True)
            (args.evidence_dir / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print("PASS: exact file/stdin/text sends, file follow-up, input bounds, FIFO refusal and unchanged Host on invalid input")


if __name__ == "__main__":
    main()
