#!/usr/bin/env python3
"""Provider capability paging, validated turn options and failure retention.

Uses only a synthetic provider, a private socket and temporary files.
"""

import json
import os
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path


def main():
    repository = Path(__file__).resolve().parents[1]
    binary = repository / "target/debug/fleetpane-host"
    with tempfile.TemporaryDirectory(prefix="fleetpane-models-") as directory:
        root = Path(directory)
        models = root / "models.json"
        catalog = [
            {
                "id": f"picker-{index}",
                "model": f"wire-{index}",
                "displayName": f"模型 {index}",
                "description": "Synthetic model",
                "hidden": index == 70,
                "isDefault": index == 0,
                "defaultReasoningEffort": "future-effort",
                "supportedReasoningEfforts": [
                    {"reasoningEffort": "future-effort", "description": "Future"},
                    {"reasoningEffort": "low", "description": "Low"},
                ],
            }
            for index in range(72)
        ]
        models.write_text(json.dumps(catalog))
        socket = root / "private/control.sock"
        with (root / "host.log").open("w+") as log:
            process = subprocess.Popen(
                [
                    str(binary),
                    "serve",
                    str(socket),
                    str(root),
                    sys.executable,
                    str(repository / "scripts/provider-fixture.py"),
                    "--models",
                    str(models),
                ],
                stdout=subprocess.DEVNULL,
                stderr=log,
                start_new_session=True,
            )
            try:
                deadline = time.monotonic() + 10
                while (
                    not socket.exists()
                    or "ready:" not in (root / "host.log").read_text()
                ):
                    assert process.poll() is None and time.monotonic() < deadline
                    time.sleep(0.05)

                def raw(payload):
                    reply = subprocess.run(
                        [str(binary), "request", str(socket)],
                        input=json.dumps(payload),
                        capture_output=True,
                        text=True,
                        timeout=8,
                        check=False,
                    )
                    assert reply.returncode == 0, (reply.stdout, reply.stderr)
                    return json.loads(reply.stdout)["data"]

                def run(*args):
                    return subprocess.run(
                        [str(binary), "session", str(socket), "--take-control", *args],
                        capture_output=True,
                        text=True,
                        timeout=25,
                        check=False,
                    )

                def command(*args):
                    reply = run(*args)
                    assert reply.returncode == 0, reply.stderr
                    return json.loads(reply.stdout)["result"]

                initial = raw({"command": "snapshot"})
                first = command("models")
                second = command("models", "--cursor", first["nextCursor"])
                assert len(first["data"]) == 64 and len(second["data"]) == 7
                assert second["nextCursor"] is None
                assert all(
                    not model["hidden"] for model in first["data"] + second["data"]
                )
                thread = command("create", str(root))["thread"]["id"]

                def completed():
                    deadline = time.monotonic() + 8
                    while True:
                        opened = command("open", thread)["thread"]
                        if opened["turns"][-1]["status"] != "inProgress":
                            return opened
                        assert time.monotonic() < deadline, opened
                        time.sleep(0.02)

                command(
                    "send",
                    thread,
                    "[options]",
                    "--model",
                    "wire-71",
                    "--effort",
                    "future-effort",
                )
                opened = completed()
                assert opened["turns"][-1]["fixtureOptions"] == {
                    "model": "wire-71",
                    "effort": "future-effort",
                }
                command("send", thread, "[options]")
                assert completed()["turns"][-1]["fixtureOptions"] == {}
                for flags, error in [
                    (["--model", "missing"], "unavailable"),
                    (["--model", "wire-70"], "unavailable"),
                    (["--model", "wire-0", "--effort", "unsupported"], "not supported"),
                    (["--effort", "low"], "requires --model"),
                    (["--model", "wire-0", "--model", "wire-1"], "duplicate"),
                    (["--model"], "--model requires a value"),
                ]:
                    failed = run("send", thread, "must not send", *flags)
                    assert failed.returncode != 0 and error in failed.stderr, (
                        flags,
                        failed.stderr,
                    )
                assert len(command("open", thread)["thread"]["turns"]) == 2
                failed = run("send", thread, "[options-fail]", "--model", "wire-0")
                assert (
                    failed.returncode != 0
                    and "Synthetic turn options rejection" in failed.stderr
                )
                assert len(command("open", thread)["thread"]["turns"]) == 2
                models.write_text(json.dumps({"error": "Synthetic catalog failure"}))
                failed = run("send", thread, "must not send", "--model", "wire-0")
                assert (
                    failed.returncode != 0
                    and "Synthetic catalog failure" in failed.stderr
                )
                command("send", thread, "session settings still work")
                assert len(completed()["turns"]) == 3
                final = raw({"command": "snapshot"})
                assert (
                    final["host_id"] == initial["host_id"]
                    and final["provider_pid"] == initial["provider_pid"]
                )
                print(
                    json.dumps(
                        {
                            "provider": "synthetic",
                            "visible_models": 71,
                            "paged_capabilities_future_effort_wire_values": "PASS",
                            "unsupported_options_rejected_before_turn": "PASS",
                            "catalog_and_turn_rejections": "PASS",
                            "session_defaults_without_catalog": "PASS",
                        }
                    )
                )
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGINT)
                    process.wait(timeout=10)


if __name__ == "__main__":
    main()
