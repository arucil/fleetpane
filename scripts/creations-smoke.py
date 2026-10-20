#!/usr/bin/env python3
"""Durable creation binding, controller loss and Host restart; synthetic only."""

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


def wait(probe, label):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        if result := probe():
            return result
        time.sleep(0.025)
    raise AssertionError(label)


def main():
    with tempfile.TemporaryDirectory(prefix="fp-creations-") as temporary:
        root = Path(temporary)
        config = root / "hosts.json"
        store = root / "creations"
        processes = []
        endpoints = []
        logs = []

        def start(index):
            home = root / str(index)
            home.mkdir(mode=0o700, exist_ok=True)
            endpoint = home / "host/control.sock"
            log = root / f"{index}-{len(logs)}.log"
            logs.append(log)
            arguments = [
                sys.executable,
                str(ROOT / "scripts/provider-fixture.py"),
                "--creation-effects",
                str(home / "effects.jsonl"),
            ]
            if index == 1:
                arguments.append("--hold-creation")
            if index == 2:
                arguments = [sys.executable, str(ROOT / "scripts/acp-fixture.py")]
            with log.open("w") as output:
                process = subprocess.Popen(
                    [
                        str(BINARY),
                        "serve-acp" if index == 2 else "serve",
                        str(endpoint),
                        str(home),
                        *arguments,
                    ],
                    stdout=subprocess.DEVNULL,
                    stderr=output,
                    start_new_session=True,
                    env={
                        **os.environ,
                        "HOME": str(home),
                        "CODEX_HOME": str(home / "codex"),
                        "DSH_HOME": str(home / "dsh"),
                    },
                )
            processes.append(process)
            wait(
                lambda: process.poll() is not None or "ready:" in log.read_text(),
                "Host startup",
            )
            assert process.poll() is None, log.read_text()
            return process, {
                "name": ["Old", "New", "DeepSeek"][index],
                "socket": str(endpoint),
            }

        def snapshot(index):
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(5)
                stream.connect(endpoints[index]["socket"])
                stream.sendall(b'{"command":"snapshot"}\n')
                response = json.loads(stream.makefile("rb").readline(1024 * 1024))
                assert response["response"] == "snapshot", response
                return response["data"]

        def call(*arguments, succeeds=True):
            result = subprocess.run(
                [str(BINARY), "creation", str(store), *arguments],
                capture_output=True,
                text=True,
                timeout=25,
                check=False,
            )
            assert (result.returncode == 0) == succeeds, (
                arguments,
                result.stdout,
                result.stderr,
            )
            return json.loads(result.stdout) if succeeds else result.stderr

        def plan(identity, group="Codex"):
            return call("plan", str(config), group, str(root), str(identity))

        def set_mode(index, mode):
            subprocess.run(
                [
                    str(BINARY),
                    "session",
                    endpoints[index]["socket"],
                    "--take-control",
                    mode,
                ],
                check=True,
                capture_output=True,
                timeout=10,
            )

        def effects(index):
            path = root / str(index) / "effects.jsonl"
            return (
                [json.loads(line) for line in path.read_text().splitlines()]
                if path.exists()
                else []
            )

        def stop(process):
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=5)

        try:
            for index in range(3):
                _, endpoint = start(index)
                endpoints.append(endpoint)
            configuration = {
                "version": 1,
                "hosts": endpoints,
                "routes": [
                    {"name": "Codex", "hosts": ["Old", "New"]},
                    {"name": "DeepSeek", "hosts": ["DeepSeek"]},
                ],
            }
            config.write_text(json.dumps(configuration))
            first = uuid.uuid4()
            bound = plan(first)
            assert (
                bound["target"]["endpoint"]["name"] == "Old" and not bound["attempted"]
            )
            assert snapshot(0)["owner"] is None and snapshot(0)["journal_total"] == 0
            configuration["routes"][0]["hosts"].reverse()
            config.write_text(json.dumps(configuration))
            assert plan(first)["target"] == bound["target"], (
                "priority change rebound a UUID"
            )
            call("send", str(first))
            old = snapshot(0)
            assert call("check", str(first))["status"] == "created"
            call("send", str(first), "--take-control", succeeds=False)
            assert snapshot(0)["owner"] == old["owner"], (
                "duplicate send claimed control"
            )
            assert len(effects(0)) == 1 and snapshot(1)["journal_total"] == 0
            call("plan", str(config), "DeepSeek", str(root), str(first), succeeds=False)

            # Kill the controller after the provider records its side effect,
            # while the result is held. No timing-based response-loss simulation.
            held = uuid.uuid4()
            assert plan(held)["target"]["endpoint"]["name"] == "New"
            sender = subprocess.Popen(
                [str(BINARY), "creation", str(store), "send", str(held)],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            processes.append(sender)
            wait(lambda: effects(1), "held creation reached provider")
            sender.kill()
            sender.wait(timeout=5)
            assert call("check", str(held))["status"] == "pending"
            call("send", str(held), "--take-control", succeeds=False)
            assert len(effects(1)) == 1
            stop(processes[1])
            # Same endpoint, new incarnation, durable accepted receipt -> unknown.
            start(1)
            assert call("check", str(held))["status"] == "unknown"
            call("send", str(held), "--take-control", succeeds=False)
            assert snapshot(1)["owner"] is None and len(effects(1)) == 1
            assert call("check", str(first))["status"] == "created"

            # A prepared binding also cannot follow changing admission epochs.
            configuration["routes"][0]["hosts"] = ["Old", "New"]
            config.write_text(json.dumps(configuration))
            stale = uuid.uuid4()
            assert plan(stale)["target"]["endpoint"]["name"] == "Old"
            set_mode(0, "drain")
            set_mode(0, "ready")
            owner = snapshot(0)["owner"]
            call("send", str(stale), "--take-control", succeeds=False)
            assert snapshot(0)["owner"] == owner
            assert call("check", str(stale))["status"] == "prepared"
            assert len(effects(0)) == 1 and len(effects(1)) == 1

            acp = uuid.uuid4()
            assert plan(acp, "DeepSeek")["target"]["endpoint"]["name"] == "DeepSeek"
            call("send", str(acp))
            assert call("check", str(acp))["status"] == "created"
            owner = snapshot(2)["owner"]
            call("send", str(acp), "--take-control", succeeds=False)
            assert snapshot(2)["owner"] == owner and snapshot(2)["journal_total"] == 1
            assert len(call("list")) == 4
            print(
                "PASS: sticky UUID/Host, no duplicate claim/dispatch, killed-controller pending, restarted-Host unknown, stale admission refusal, Codex and ACP receipts"
            )
        finally:
            for process in processes:
                if process.poll() is None:
                    # The controller subprocess is already reaped on the normal path.
                    if process.args[1] == "creation":
                        process.kill()
                        process.wait(timeout=5)
                    else:
                        stop(process)


if __name__ == "__main__":
    main()
