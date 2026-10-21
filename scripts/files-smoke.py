#!/usr/bin/env python3
"""Bounded, read-only real filesystem observations beside running Codex/ACP Hosts."""

import hashlib
import errno
import json
import os
import signal
import socket
import subprocess
import sys
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"


def main():
    processes, hosts, before = [], [], []
    with tempfile.TemporaryDirectory(prefix="fp-files-", dir="/tmp") as temporary:
        root = Path(temporary)
        folder, store, config = root / "work 中文", root / "state", root / "hosts.json"
        folder.mkdir()

        def call(*args, ok=True):
            result = subprocess.run(
                [str(BINARY), *map(str, args)],
                capture_output=True,
                text=True,
                check=False,
                timeout=20,
            )
            assert (result.returncode == 0) == ok, (args, result.stdout, result.stderr)
            return json.loads(result.stdout) if ok else result.stderr

        def snapshot(host):
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(5)
                stream.connect(host["socket"])
                stream.sendall(b'{"command":"snapshot"}\n')
                return json.loads(stream.makefile("rb").readline(1024 * 1024))["data"]

        def observe(workspace, verb, *args, ok=True):
            return call("workspace", store, verb, workspace["id"], *args, ok=ok)

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
                        start_new_session=True,
                        stdout=subprocess.DEVNULL,
                        stderr=output,
                    )
                processes.append(process)
                deadline = time.monotonic() + 15
                while not endpoint.exists() or "ready:" not in log.read_text():
                    assert process.poll() is None, log.read_text()
                    assert time.monotonic() < deadline
                    time.sleep(0.025)
                host = {"name": name, "socket": str(endpoint)}
                hosts.append(host)
                thread = call("session", endpoint, "--take-control", "create", folder)[
                    "result"
                ]["thread"]["id"]
                call(
                    "session",
                    endpoint,
                    "--take-control",
                    "send",
                    thread,
                    "[tasks-wait]" if index else "[wait]",
                )
                call("session", endpoint, "--take-control", "open", thread)
                before.append(snapshot(host))
            config.write_text(json.dumps({"version": 1, "hosts": hosts}))
            subprocess.run(
                ["git", "init", "-q", "-b", "main", str(folder)], check=True, timeout=10
            )
            (folder / "目录").mkdir()
            exact = "中文\r\n  exact trailing space \n"
            (folder / "目录" / "line\nfile.txt").write_bytes(exact.encode())
            (folder / "binary").write_bytes(b"\x00\xff")
            (folder / "large").write_bytes(b"x" * (256 * 1024 + 1))
            (folder / "empty").touch()
            (folder / "long").write_text("中" * 20_000)
            (folder / "outside").symlink_to(root, target_is_directory=True)
            (folder / "dangling").symlink_to("missing")
            os.mkfifo(folder / "pipe")
            (folder / "many").mkdir()
            for index in range(501):
                (folder / "many" / str(index)).touch()
            (folder / "non-utf8").mkdir()
            invalid_name_created = False
            try:
                with open(os.fsencode(folder / "non-utf8") + b"/\xff", "wb") as file:
                    file.write(b"data")
                invalid_name_created = True
            except OSError as error:
                if sys.platform != "darwin" or error.errno != errno.EILSEQ:
                    raise
            initial = {
                str(p.relative_to(folder)): hashlib.sha256(p.read_bytes()).hexdigest()
                for p in folder.rglob("*")
                if not p.is_symlink() and p.is_file()
            }
            workspaces = [
                call("workspace", store, "add", config, h["name"], "git", folder)
                for h in hosts
            ]
            assert workspaces[0]["id"] != workspaces[1]["id"]
            for workspace in workspaces:
                listing = observe(workspace, "files")
                assert listing["workspace_id"] == workspace["id"]
                assert len(listing["entries"]) == 11, listing
                kinds = {e["name"]: e["kind"] for e in listing["entries"]}
                assert (
                    kinds["outside"] == kinds["dangling"] == "symlink"
                    and kinds["pipe"] == "other"
                )
                assert (
                    observe(workspace, "files", "目录")["entries"][0]["name"]
                    == "line\nfile.txt"
                )
                assert observe(workspace, "read", "目录/line\nfile.txt")["content"] == {
                    "kind": "text",
                    "text": exact,
                }
                assert observe(workspace, "read", "empty")["content"] == {
                    "kind": "text",
                    "text": "",
                }
                assert (
                    observe(workspace, "read", "long")["content"]["text"]
                    == "中" * 20_000
                )
                assert (
                    observe(workspace, "read", "large")["content"]["kind"]
                    == "too_large"
                )
                assert (
                    observe(workspace, "read", "binary")["content"]["kind"] == "binary"
                )
                for path in [
                    "../hosts.json",
                    "/etc/passwd",
                    "outside/hosts.json",
                    "dangling",
                    "pipe",
                    "目录",
                    "a/../empty",
                ]:
                    observe(workspace, "read", path, ok=False)
                for path in ["many", "outside", "../"]:
                    observe(workspace, "files", path, ok=False)
                if invalid_name_created:
                    observe(workspace, "files", "non-utf8", ok=False)
                # A rejected read does not poison the next request.
                assert observe(workspace, "read", "empty")["content"]["kind"] == "text"
            folder.rename(root / "kept")
            folder.mkdir()
            (folder / "empty").write_text("replacement")
            for workspace in workspaces:
                observe(workspace, "files", ok=False)
                observe(workspace, "read", "empty", ok=False)
            (folder / "empty").unlink()
            folder.rmdir()
            (root / "kept").rename(folder)
            final = {
                str(p.relative_to(folder)): hashlib.sha256(p.read_bytes()).hexdigest()
                for p in folder.rglob("*")
                if not p.is_symlink() and p.is_file()
            }
            assert initial == final
            assert [snapshot(host) for host in hosts] == before
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
                    "real_git_workspace": True,
                    "nested_unicode_newline_exact": True,
                    "long_empty_binary_large": True,
                    "directory_and_nonUTF8_limits_explicit": True,
                    "traversal_symlink_fifo_refused": True,
                    "replacement_refused_recovery": True,
                    "file_hashes_unchanged": True,
                    "full_host_snapshots_unchanged": True,
                    "owned_hosts_stopped": True,
                }
            )
        )


if __name__ == "__main__":
    main()
