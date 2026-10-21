#!/usr/bin/env python3
"""Synthetic MCP/ACP resources through real Host I/O, reload and ACP archive restart."""
import argparse
import hashlib
import json
import os
import signal
import subprocess
import tempfile
import time
from pathlib import Path

from resource_fixture import blocks

ROOT = Path(__file__).resolve().parents[1]
CASES = ("text", "long", "markdown", "empty", "binary", "link", "invalid", "future")


def verify(root):
    binary = ROOT / "target/debug/fleetpane-host"
    receipt = {"synthetic_only": True, "host_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
               "cases": [], "cleanup": []}
    for provider in ("codex", "acp"):
        home = root / provider
        home.mkdir(mode=0o700)
        control = home / "host/control.sock"
        process = None
        generation = 0

        def command(*args):
            result = subprocess.run([str(binary), "session", str(control), "--take-control", *args],
                                    capture_output=True, text=True, timeout=15)
            assert result.returncode == 0, result.stderr
            return json.loads(result.stdout)["result"]

        def wait(probe):
            deadline = time.monotonic() + 12
            while True:
                value = probe()
                if value:
                    return value
                assert time.monotonic() < deadline, "resource fixture timed out"
                time.sleep(0.025)

        def start():
            nonlocal process, generation
            generation += 1
            log = home / f"host-{generation}.log"
            fixture = ROOT / "scripts" / ("acp-fixture.py" if provider == "acp" else "provider-fixture.py")
            with log.open("w") as output:
                process = subprocess.Popen([str(binary), "serve-acp" if provider == "acp" else "serve",
                                            str(control), str(home), "python3", str(fixture)],
                                           stdout=subprocess.DEVNULL, stderr=output, start_new_session=True)
            wait(lambda: process.poll() is None and control.exists() and "ready:" in log.read_text())

        def stop():
            if process is not None and process.poll() is None:
                os.killpg(process.pid, signal.SIGINT)
                process.wait(timeout=10)
                receipt["cleanup"].append({"provider": provider, "pid": process.pid, "exit_code": process.returncode})

        try:
            start()
            threads = []
            for case in CASES:
                identity = command("create", str(home))["thread"]["id"]
                command("send", identity, f"[resource-{case}]")
                def completed():
                    thread = command("open", identity)["thread"]
                    return thread if thread["turns"] and thread["turns"][0]["status"] == "completed" else None
                thread = wait(completed)
                tool = next(item for item in thread["turns"][0]["items"]
                            if item["type"] == ("acpToolCall" if provider == "acp" else "mcpToolCall"))
                content = ([entry["content"] for entry in tool["call"]["content"]]
                           if provider == "acp" else tool["result"]["content"])
                assert content == blocks(case), (provider, case, content)
                assert command("open", identity)["thread"] == thread
                (home / f"{case}.json").write_text(json.dumps(thread, ensure_ascii=False, indent=2))
                threads.append(thread)
                receipt["cases"].append({"provider": provider, "case": case, "thread_id": identity,
                                          "exact_reload": True})
            (home / "history.json").write_text(json.dumps(threads, ensure_ascii=False))
            if provider == "acp":
                stop()
                start()
                for thread in threads:
                    recovered = command("open", thread["id"])["thread"]
                    assert recovered["turns"] == thread["turns"], thread["id"]
                receipt["acp_archive_restart"] = {"threads": len(threads), "exact_turns": True}
        finally:
            stop()
    (root / "process-receipt.json").write_text(json.dumps(receipt, indent=2))
    print(json.dumps({"cases": len(receipt["cases"]), "acp_archive_restart": receipt["acp_archive_restart"],
                      "cleaned_processes": len(receipt["cleanup"])}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-dir", type=Path)
    args = parser.parse_args()
    if args.evidence_dir:
        args.evidence_dir.mkdir(mode=0o700, parents=True)
        verify(args.evidence_dir.resolve())
    else:
        with tempfile.TemporaryDirectory(prefix="fp-resources-") as directory:
            verify(Path(directory))


if __name__ == "__main__":
    main()
