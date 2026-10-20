#!/usr/bin/env python3
"""Explicit groups recommend ready Hosts without claiming, mutating or failover."""

import json
import os
import signal
import socket
import subprocess
import tempfile
import time
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
    with tempfile.TemporaryDirectory(prefix="fp-route-") as directory:
        root = Path(directory)
        hosts = []
        processes = []

        def snapshot(index):
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(5)
                stream.connect(hosts[index]["socket"])
                stream.sendall(b'{"command":"snapshot"}\n')
                response = json.loads(stream.makefile("rb").readline(1024 * 1024))
                assert response["response"] == "snapshot", response
                return response["data"]

        def command(index, *arguments, succeeds=True):
            result = subprocess.run(
                [
                    str(BINARY),
                    "session",
                    hosts[index]["socket"],
                    "--take-control",
                    *arguments,
                ],
                capture_output=True,
                text=True,
                timeout=25,
                check=False,
            )
            assert (result.returncode == 0) == succeeds, result.stderr
            return json.loads(result.stdout) if succeeds else result.stderr

        config = root / "hosts.json"

        def select(group="Codex", succeeds=True):
            before = [snapshot(i) for i in range(3)]
            result = subprocess.run(
                [str(BINARY), "route", str(config), group],
                capture_output=True,
                text=True,
                timeout=15,
                check=False,
            )
            assert (result.returncode == 0) == succeeds, result.stderr
            for index, old in enumerate(before):
                current = snapshot(index)
                for field in [
                    "owner",
                    "journal_total",
                    "admission",
                    "pending_requests",
                    "host_id",
                    "host_pid",
                    "provider_pid",
                ]:
                    assert current[field] == old[field], (index, field)
            return json.loads(result.stdout)

        try:
            for index, name in enumerate(["Old", "New", "DeepSeek"]):
                work = root / str(index)
                work.mkdir(mode=0o700)
                socket_path = work / "host/control.sock"
                log_path = work / "host.log"
                fixture = "acp-fixture.py" if index == 2 else "provider-fixture.py"
                with log_path.open("w") as log:
                    process = subprocess.Popen(
                        [
                            str(BINARY),
                            "serve-acp" if index == 2 else "serve",
                            str(socket_path),
                            str(work),
                            "python3",
                            str(ROOT / "scripts" / fixture),
                        ],
                        stdout=subprocess.DEVNULL,
                        stderr=log,
                        start_new_session=True,
                        env={
                            **os.environ,
                            "HOME": str(work),
                            "CODEX_HOME": str(work / "codex"),
                            "DSH_HOME": str(work / "dsh"),
                        },
                    )
                processes.append(process)
                wait(
                    lambda process=process, log_path=log_path: (
                        process.poll() is not None or "ready:" in log_path.read_text()
                    ),
                    "Host startup",
                )
                assert process.poll() is None, log_path.read_text()
                hosts.append({"name": name, "socket": str(socket_path)})
            hosts.append(
                {"name": "Offline", "socket": str(root / "offline/control.sock")}
            )
            value = {
                "version": 1,
                "hosts": hosts,
                "routes": [
                    {"name": "Codex", "hosts": ["Old", "Offline", "New"]},
                    {"name": "DeepSeek", "hosts": ["DeepSeek"]},
                ],
            }
            config.write_text(json.dumps(value))
            assert select()["target"]["endpoint"]["name"] == "Old"
            assert select("DeepSeek")["target"]["endpoint"]["name"] == "DeepSeek"
            identity = command(0, "create", str(root / "0"))["result"]["thread"]["id"]
            turn = command(0, "send", identity, "[wait]")["result"]["turn"]["id"]
            wait(lambda: snapshot(0)["activity"]["turns"], "old active turn")
            old = snapshot(0)
            command(0, "drain")
            report = select()
            assert report["target"]["endpoint"]["name"] == "New"
            assert [entry["status"] for entry in report["candidates"]] == [
                "draining",
                "unavailable",
                "ready",
            ]
            # Creation is a separate explicit action through the selected Host.
            fresh = command(1, "create", str(root / "1"))["result"]["thread"]["id"]
            new_turn = command(1, "send", fresh, "new Host completes work")["result"][
                "turn"
            ]["id"]
            wait(lambda: not snapshot(1)["activity"]["turns"], "new turn completion")
            history = command(1, "open", fresh)["result"]["thread"]
            assert (
                history["turns"][-1]["id"] == new_turn
                and history["turns"][-1]["status"] == "completed"
            ), history
            assert snapshot(0)["activity"]["turns"] == old["activity"]["turns"]
            assert snapshot(0)["host_pid"] == old["host_pid"]
            assert snapshot(0)["provider_pid"] == old["provider_pid"]
            command(1, "drain")
            assert select(succeeds=False)["target"] is None, (
                "must not leave Codex group"
            )
            before = [snapshot(i)["journal_total"] for i in range(3)]
            assert "host_draining" in command(
                1, "create", str(root / "1"), succeeds=False
            )
            assert [snapshot(i)["journal_total"] for i in range(3)] == before, (
                "no cross-Host replay"
            )
            command(0, "stop", identity, turn)
            wait(lambda: not snapshot(0)["activity"]["turns"], "old interrupted turn")
            command(0, "ready")
            command(1, "ready")
            # Configured priority wins over tab order and remains across CLI restarts.
            value["routes"][0]["hosts"] = ["New", "Old"]
            config.write_text(json.dumps(value))
            assert select()["target"]["endpoint"]["name"] == "New"
            # Alias paths must not be presented as independent upgrade candidates.
            alias = root / "alias.sock"
            alias.symlink_to(hosts[1]["socket"])
            value["hosts"].append({"name": "Alias", "socket": str(alias)})
            value["routes"][0]["hosts"] = ["New", "Alias"]
            config.write_text(json.dumps(value))
            duplicate = subprocess.run(
                [str(BINARY), "route", str(config), "Codex"],
                capture_output=True,
                text=True,
                timeout=15,
                check=False,
            )
            assert duplicate.returncode != 0 and "same Host" in duplicate.stderr, (
                duplicate.stderr
            )
            os.kill(snapshot(2)["provider_pid"], signal.SIGTERM)
            wait(lambda: snapshot(2)["fault"], "provider fault")
            assert (
                select("DeepSeek", succeeds=False)["candidates"][0]["status"]
                == "unavailable"
            )
            print(
                json.dumps(
                    {
                        "read_only_selection": True,
                        "explicit_group_boundary": True,
                        "ordered_priority": True,
                        "drain_offline_fault_skipped": True,
                        "old_turn_same_processes": True,
                        "explicit_new_host_turn_completed": True,
                        "drain_race_rejected_without_cross_host_retry": True,
                        "duplicate_alias_rejected": True,
                    }
                )
            )
        finally:
            for process in processes:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGINT)
                process.wait(timeout=15)
            for host in hosts[:3]:
                assert not Path(host["socket"]).exists(), "owned socket cleanup"


if __name__ == "__main__":
    main()
