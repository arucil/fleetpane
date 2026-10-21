#!/usr/bin/env python3
"""Real local catalog and running private Hosts; pinning dispatches no provider work."""

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


def raw(endpoint, request):
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(5)
        stream.connect(str(endpoint))
        stream.sendall((json.dumps(request) + "\n").encode())
        return json.loads(stream.makefile("rb").readline(1024 * 1024))


def run(*args, ok=True):
    result = subprocess.run(
        [str(BINARY), *map(str, args)],
        capture_output=True,
        text=True,
        timeout=30,
        check=False,
    )
    assert (result.returncode == 0) == ok, (args, result.stdout, result.stderr)
    return json.loads(result.stdout) if ok else result.stderr


def main():
    processes = []
    with tempfile.TemporaryDirectory(prefix="fp-pins-", dir="/tmp") as temporary:
        root = Path(temporary)
        store, config = root / "state", root / "hosts.json"
        hosts, snapshots, drafts = [], [], []
        folder = root / "中文 workspace"
        folder.mkdir()

        def snapshot(host):
            return raw(host["socket"], {"command": "snapshot"})["data"]

        def workspace(*args, ok=True):
            return run("workspace", store, *args, ok=ok)

        def register(path, host="Codex"):
            return workspace("add", config, host, "folder", path)

        try:
            for index, name in enumerate(["Codex", "DeepSeek ACP"]):
                home = root / str(index)
                home.mkdir(mode=0o700)
                endpoint, log = home / "host/control.sock", home / "host.log"
                with log.open("w") as output:
                    process = subprocess.Popen(
                        [
                            str(BINARY),
                            "serve-acp" if index else "serve",
                            str(endpoint),
                            str(home),
                            sys.executable,
                            str(
                                ROOT
                                / "scripts"
                                / ("acp-fixture.py" if index else "provider-fixture.py")
                            ),
                        ],
                        stdout=subprocess.DEVNULL,
                        stderr=output,
                        start_new_session=True,
                    )
                processes.append(process)
                deadline = time.monotonic() + 15
                while not endpoint.exists() or "ready:" not in log.read_text():
                    assert process.poll() is None, log.read_text()
                    assert time.monotonic() < deadline, log.read_text()
                    time.sleep(0.025)
                host = {"name": name, "socket": str(endpoint)}
                hosts.append(host)
                thread = run("session", endpoint, "--take-control", "create", folder)[
                    "result"
                ]["thread"]["id"]
                run(
                    "session",
                    endpoint,
                    "--take-control",
                    "send",
                    thread,
                    "[tasks-wait]" if index else "[wait]",
                )
                run("session", endpoint, "--take-control", "open", thread)
                state = snapshot(host)
                key = json.dumps(
                    {
                        "socket": str(endpoint),
                        "host_id": state["host_id"],
                        "thread_id": thread,
                        "cwd": str(folder),
                    }
                )
                draft = run("draft", store, "get", key)
                draft = run(
                    "draft",
                    store,
                    "edit",
                    key,
                    draft["revision"],
                    "未发送的草稿\n  keep exact text\n",
                )
                drafts.append((key, draft))
                snapshots.append(state)
            config.write_text(json.dumps({"version": 1, "hosts": hosts}))
            a, b = register(folder), register(folder, "DeepSeek ACP")
            assert a["id"] != b["id"]
            alias = root / "alias"
            alias.symlink_to(folder, target_is_directory=True)
            assert register(alias)["id"] == a["id"]
            for index in range(34):
                path = root / f"later-{index}"
                path.mkdir()
                register(path)
            before = workspace("list")
            assert len(before) == 32 and all(
                w["id"] not in (a["id"], b["id"]) for w in before
            )
            assert not workspace("pins")
            workspace("pin", a["id"], "on")
            expected = [b["id"], a["id"]]
            assert [w["id"] for w in workspace("pin", b["id"], "on")] == expected
            assert [w["id"] for w in workspace("pin", a["id"], "on")] == expected
            writers = [
                subprocess.Popen(
                    [str(BINARY), "workspace", str(store), "pin", a["id"], "on"],
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                )
                for _ in range(4)
            ]
            try:
                for writer in writers:
                    output, error = writer.communicate(timeout=30)
                    assert writer.returncode == 0, error
                    assert [w["id"] for w in json.loads(output)] == expected
            finally:
                for writer in writers:
                    if writer.poll() is None:
                        writer.kill()
                        writer.wait(timeout=5)
            assert [w["id"] for w in workspace("pins")] == expected
            assert workspace("list") == before
            for state in ["on", "off"]:
                workspace("pin", uuid.uuid4(), state, ok=False)
            workspace("pin", a["id"], "toggle", ok=False)
            # Pin metadata is available even when a saved directory is replaced.
            folder.rename(root / "original-kept")
            folder.mkdir()
            workspace("inspect", a["id"], ok=False)
            workspace("pin", a["id"], "off")
            workspace("pin", a["id"], "on")
            assert workspace("pins")[0]["id"] == a["id"]
            workspace("pin", a["id"], "off")
            assert [w["id"] for w in workspace("pin", a["id"], "off")] == [b["id"]]
            assert workspace("list", before[-1]["position"])[-1]["id"] == a["id"]
            assert len(workspace("list")) == 32
            for (key, draft), host, old in zip(drafts, hosts, snapshots, strict=True):
                assert run("draft", store, "get", key) == draft
                assert snapshot(host) == old
        finally:
            for process in processes:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGINT)
                    process.wait(timeout=10)
            for host in hosts:
                assert not Path(host["socket"]).exists()
        print(
            json.dumps(
                {
                    "ok": True,
                    "older_page_pins_restored": True,
                    "same_path_hosts_isolated": True,
                    "explicit_idempotent_assignment": True,
                    "parallel_same_pin_once": True,
                    "unpin_preserves_catalog_and_drafts": True,
                    "offline_replaced_metadata_access": True,
                    "complete_host_snapshots_unchanged": True,
                    "owned_hosts_stopped": True,
                }
            )
        )


if __name__ == "__main__":
    main()
