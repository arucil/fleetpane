#!/usr/bin/env python3
"""Independent Codex/ACP Hosts, colliding provider IDs, permissions and one-host loss."""

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
    while True:
        value = probe()
        if value:
            return value
        assert time.monotonic() < deadline, label
        time.sleep(0.025)


def main():
    with tempfile.TemporaryDirectory(prefix="fp-multihost-") as directory:
        root = Path(directory)
        hosts = []
        processes = []

        def raw(index, value):
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(8)
                stream.connect(hosts[index]["socket"])
                stream.sendall(json.dumps(value).encode() + b"\n")
                return json.loads(stream.makefile().readline())

        def snapshot(index):
            value = raw(index, {"command": "snapshot"})
            assert value["response"] == "snapshot", value
            assert value["data"]["fault"] is None, value
            return value["data"]

        def command(index, *arguments):
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
                timeout=20,
                check=False,
            )
            assert result.returncode == 0, result.stderr
            payload = json.loads(result.stdout)
            if arguments[0] == "permission":
                assert payload == {"answered": True}, payload
                return payload
            return payload["result"]

        def report():
            return json.loads(
                subprocess.check_output(
                    [str(BINARY), "hosts", str(config)], text=True, timeout=15
                )
            )

        def settled(index, identity, turn):
            page = command(index, "history-turns", identity)
            return next(
                (
                    t
                    for t in page["data"]
                    if t["id"] == turn and t["status"] != "inProgress"
                ),
                None,
            )

        try:
            for index in range(4):
                work = root / f"work-{index}"
                work.mkdir()
                acp = index >= 2
                control = root / f"host-{index}/control.sock"
                log_path = root / f"host-{index}.log"
                with log_path.open("w") as log:
                    process = subprocess.Popen(
                        [
                            str(BINARY),
                            "serve-acp" if acp else "serve",
                            str(control),
                            str(work),
                            "python3",
                            str(
                                ROOT
                                / "scripts"
                                / ("acp-fixture.py" if acp else "provider-fixture.py")
                            ),
                        ],
                        stdout=subprocess.DEVNULL,
                        stderr=log,
                        start_new_session=True,
                    )
                processes.append(process)
                wait(
                    lambda process=process, log_path=log_path: (
                        process.poll() is not None or "ready:" in log_path.read_text()
                    ),
                    "host startup",
                )
                assert process.poll() is None, log_path.read_text()
                hosts.append(
                    {
                        "name": f"{'ACP' if acp else 'Codex'} {index}",
                        "socket": str(control),
                    }
                )
            hosts.append(
                {"name": "Offline", "socket": str(root / "offline/control.sock")}
            )
            config = root / "hosts.json"
            config.write_text(json.dumps({"version": 1, "hosts": hosts}))
            initial = [snapshot(i) for i in range(4)]
            first = report()
            assert [row["endpoint"]["name"] for row in first] == [
                row["name"] for row in hosts
            ]
            assert [row["status"] for row in first] == ["available"] * 4 + [
                "unavailable"
            ]
            assert all(row["snapshot"]["owner"] is None for row in first[:4]), (
                "probe must not claim"
            )
            assert len({row["snapshot"]["host_id"] for row in first[:4]}) == 4
            identities = [
                command(i, "create", str(root / f"work-{i}"))["thread"]["id"]
                for i in range(4)
            ]
            assert identities[0] == identities[1], (
                "fixture must exercise colliding provider thread IDs"
            )
            turns = [
                command(i, "send", identities[i], f"only Host {i}")["turn"]["id"]
                for i in range(4)
            ]
            assert turns[0] == turns[1], "fixture must exercise colliding turn IDs"
            for i, identity in enumerate(identities):
                wait(
                    lambda i=i, identity=identity: settled(i, identity, turns[i]),
                    "turn settlement",
                )
                items = command(i, "history-items", identity, turns[i])["data"]
                text = json.dumps(items)
                assert f"only Host {i}" in text
                assert all(
                    f"only Host {other}" not in text for other in range(4) if other != i
                )
            # ACP request IDs intentionally collide at 10. Answer on one Host only.
            for i in [2, 3]:
                command(i, "send", identities[i], "[permission]")
            pending = [
                wait(lambda i=i: snapshot(i)["pending_requests"], "ACP permission")[0]
                for i in [2, 3]
            ]
            assert pending[0]["id"] == pending[1]["id"]
            command(2, "permission", json.dumps(pending[0]["id"]), "reject-one")
            assert snapshot(3)["pending_requests"] == [pending[1]], (
                "answer crossed Host boundary"
            )
            # A lease cannot be used on another Host, even when provider IDs match.
            rejected = raw(
                3,
                {
                    "command": "respond",
                    "lease": snapshot(2)["owner"],
                    "request_id": pending[1]["id"],
                    "result": {
                        "outcome": {"outcome": "selected", "optionId": "allow-one"}
                    },
                },
            )
            assert (
                rejected["response"] == "error"
                and rejected["data"]["code"] == "stale_controller"
            ), rejected
            command(3, "permission", json.dumps(pending[1]["id"]), "allow-one")
            wait(
                lambda: (
                    not snapshot(2)["pending_requests"]
                    and not snapshot(3)["pending_requests"]
                ),
                "permission clearance",
            )
            before = [snapshot(i) for i in range(4)]
            report()
            assert all(snapshot(i)["owner"] == before[i]["owner"] for i in range(4))
            os.killpg(processes[0].pid, signal.SIGINT)
            processes[0].wait(timeout=15)
            after_loss = report()
            assert [r["status"] for r in after_loss] == [
                "unavailable",
                "available",
                "available",
                "available",
                "unavailable",
            ]
            next_turn = command(
                1, "send", identities[1], "surviving Host remains usable"
            )["turn"]["id"]
            wait(
                lambda: settled(1, identities[1], next_turn),
                "surviving Host settlement",
            )
            for i in range(1, 4):
                current = snapshot(i)
                assert (current["host_pid"], current["provider_pid"]) == (
                    initial[i]["host_pid"],
                    initial[i]["provider_pid"],
                )
            for bad in [
                {"version": 7, "hosts": hosts},
                {"version": 1, "hosts": [hosts[0], hosts[0]]},
            ]:
                config.write_text(json.dumps(bad))
                result = subprocess.run(
                    [str(BINARY), "hosts", str(config)],
                    capture_output=True,
                    text=True,
                    check=False,
                )
                assert result.returncode != 0 and "Host configuration" in result.stderr
            print(
                json.dumps(
                    {
                        "providers": {"synthetic Codex": 2, "synthetic ACP": 2},
                        "colliding_thread_turn_and_permission_ids_isolated": True,
                        "cross_host_lease_rejected": True,
                        "probe_never_claims": True,
                        "one_host_loss_other_hosts_continue_same_processes": True,
                        "invalid_configuration_rejected": True,
                    }
                )
            )
        finally:
            for process in processes:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGINT)
                    process.wait(timeout=15)
            for host in hosts[:4]:
                assert not Path(host["socket"]).exists(), "owned socket cleanup"


if __name__ == "__main__":
    main()
