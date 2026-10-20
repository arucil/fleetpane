#!/usr/bin/env python3
"""Real Git directories/worktrees with private Codex/ACP fixtures; no model calls."""

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


def main():
    with tempfile.TemporaryDirectory(prefix="fp-workspaces-") as temporary:
        root = Path(temporary)
        repo, linked = root / "repo", root / "linked"
        config, store = root / "hosts.json", root / "state"
        hosts, processes = [], []

        def git(cwd, *arguments):
            result = subprocess.run(
                ["git", "-C", str(cwd), *arguments],
                capture_output=True,
                check=True,
                timeout=10,
            )
            return result.stdout

        def snapshot(index):
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(5)
                stream.connect(hosts[index]["socket"])
                stream.sendall(b'{"command":"snapshot"}\n')
                response = json.loads(stream.makefile("rb").readline(1024 * 1024))
                assert response["response"] == "snapshot", response
                return response["data"]

        def call(*arguments, succeeds=True, env=None):
            result = subprocess.run(
                [str(BINARY), "workspace", str(store), *map(str, arguments)],
                capture_output=True,
                text=True,
                timeout=60,
                check=False,
                env=env,
            )
            assert (result.returncode == 0) == succeeds, (
                arguments,
                result.stdout,
                result.stderr,
            )
            return json.loads(result.stdout) if succeeds else result.stderr

        def register(path, host="Codex", kind="git"):
            return call("add", config, host, kind, path)

        try:
            for index, name in enumerate(["Codex", "DeepSeek"]):
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
                        env={
                            **os.environ,
                            "HOME": str(home),
                            "CODEX_HOME": str(home / "codex"),
                            "DSH_HOME": str(home / "dsh"),
                        },
                    )
                processes.append(process)
                deadline = time.monotonic() + 15
                while "ready:" not in log.read_text():
                    assert process.poll() is None, log.read_text()
                    assert time.monotonic() < deadline
                    time.sleep(0.025)
                hosts.append({"name": name, "socket": str(endpoint)})
            config.write_text(json.dumps({"version": 1, "hosts": hosts}))
            repo.mkdir()
            git(repo, "init", "-b", "main")
            (repo / "notes space.txt").write_text("main content\n")
            (repo / "old name.txt").write_text("rename content\n")
            git(repo, "add", ".")
            git(
                repo,
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.test",
                "commit",
                "-m",
                "fixture",
            )
            git(repo, "worktree", "add", "-b", "feature", str(linked))
            (linked / "notes space.txt").write_text("independent linked content\n")
            git(linked, "mv", "old name.txt", "renamed\nfile.txt")
            (linked / "中文.txt").write_text("private workspace\n")
            old_states = [snapshot(index) for index in range(2)]
            main_workspace = register(repo)
            linked_workspace = register(linked)
            assert main_workspace["id"] != linked_workspace["id"]
            assert main_workspace["project_id"] == linked_workspace["project_id"]
            alias = root / "alias"
            alias.symlink_to(linked, target_is_directory=True)
            assert register(alias)["id"] == linked_workspace["id"]
            other_host = register(linked, "DeepSeek")
            assert (
                other_host["id"] != linked_workspace["id"]
                and other_host["project_id"] != linked_workspace["project_id"]
            )
            index_path = Path(
                git(
                    linked, "rev-parse", "--path-format=absolute", "--git-path", "index"
                )
                .decode()
                .rstrip("\n")
            )
            digest = hashlib.sha256(index_path.read_bytes()).hexdigest()
            for _ in range(3):
                overview = call(
                    "inspect",
                    linked_workspace["id"],
                    env={
                        **os.environ,
                        "GIT_DIR": str(repo / ".git"),
                        "GIT_WORK_TREE": str(repo),
                    },
                )
                assert overview["git"]["linked_worktree"] and overview["git"][
                    "root"
                ] == str(linked.resolve()), overview
                assert len(overview["git"]["changes"]) == 3, overview
                renamed = next(
                    change
                    for change in overview["git"]["changes"]
                    if change["original_path"]
                )
                assert (
                    renamed["original_path"] == "old name.txt"
                    and renamed["path"] == "renamed\nfile.txt"
                )
            assert hashlib.sha256(index_path.read_bytes()).hexdigest() == digest, (
                "inspection refreshed Git index"
            )
            assert call("inspect", main_workspace["id"])["git"]["changes"] == []
            assert (repo / "notes space.txt").read_text() == "main content\n"
            git(linked, "branch", "-m", "renamed-branch")
            git(repo, "remote", "add", "origin", "https://example.test/synthetic.git")
            git(repo, "remote", "set-url", "origin", "https://example.test/changed.git")
            same = register(linked)
            assert (
                same["id"] == linked_workspace["id"]
                and same["project_id"] == main_workspace["project_id"]
            )
            assert call("inspect", same["id"])["git"]["branch"] == "renamed-branch"
            git(linked, "checkout", "--detach", "HEAD")
            assert call("inspect", same["id"])["git"]["branch"] is None
            git(linked, "checkout", "renamed-branch")
            for index, old in enumerate(old_states):
                current = snapshot(index)
                for field in (
                    "owner",
                    "journal_total",
                    "host_pid",
                    "provider_pid",
                    "admission",
                ):
                    assert current[field] == old[field], (index, field)

            # Shared command actually sends the registered canonical cwd to its Host.
            assert call("create", linked_workspace["id"])["thread"]["cwd"] == str(
                linked.resolve()
            )
            assert call("create", other_host["id"])["thread"]["cwd"] == str(
                linked.resolve()
            )
            assert (
                snapshot(0)["journal_total"] == 1 and snapshot(1)["journal_total"] == 1
            )
            unborn = root / "unborn"
            unborn.mkdir()
            git(unborn, "init", "-b", "main")
            empty = register(unborn)
            assert call("inspect", empty["id"])["git"]["head"] is None
            folder = root / "folder"
            folder.mkdir()
            plain = register(folder, kind="folder")
            assert call("inspect", plain["id"])["git"] is None
            call("add", config, "Codex", "git", folder, succeeds=False)
            folder.rename(root / "original-folder")
            folder.mkdir()
            before = snapshot(0)
            call("inspect", plain["id"], succeeds=False)
            call("create", plain["id"], "--take-control", succeeds=False)
            assert (
                snapshot(0)["owner"] == before["owner"]
                and snapshot(0)["journal_total"] == before["journal_total"]
            )
            replacement = register(folder, kind="folder")
            assert replacement["id"] != plain["id"]
            (unborn / ".git").rename(unborn / ".git-previous")
            git(unborn, "init", "-b", "main")
            call("inspect", empty["id"], succeeds=False)
            replaced_repo = register(unborn)
            assert (
                replaced_repo["id"] != empty["id"]
                and replaced_repo["project_id"] != empty["project_id"]
            )
            for index in range(35):
                path = root / f"folder-{index}"
                path.mkdir()
                register(path, kind="folder")
            first = call("list")
            assert len(first) == 32
            second = call("list", first[-1]["position"])
            assert len(second) >= 3
            assert {item["id"] for item in first}.isdisjoint(
                item["id"] for item in second
            )
            # Force bounded I/O failures through the same Git process boundary.
            fakebin = root / "fakebin"
            fakebin.mkdir()
            fakegit = fakebin / "git"
            fakegit.write_text(
                f"#!{sys.executable}\nimport sys\nsys.stdout.buffer.write(b'x' * 262145)\n"
            )
            fakegit.chmod(0o700)
            environment = {**os.environ, "PATH": f"{fakebin}:{os.environ['PATH']}"}
            error = call(
                "inspect", linked_workspace["id"], succeeds=False, env=environment
            )
            assert "256 KiB" in error, error
            marker = root / "slow-git-pid"
            fakegit.write_text(
                f"#!{sys.executable}\nimport os,time\nfrom pathlib import Path\nPath({str(marker)!r}).write_text(str(os.getpid()))\ntime.sleep(30)\n"
            )
            started = time.monotonic()
            error = call(
                "inspect", linked_workspace["id"], succeeds=False, env=environment
            )
            assert (
                "10 second deadline" in error and 9 <= time.monotonic() - started < 25
            ), error
            try:
                os.kill(int(marker.read_text()), 0)
            except ProcessLookupError:
                pass
            else:
                raise AssertionError("timed-out Git child was not reaped")
            print(
                "PASS: stable per-Host project/workspace IDs, linked worktree isolation, rename/newline/Unicode/status, index unchanged, detached/unborn, replacement refusal, Codex/ACP cwd, pagination"
            )
        finally:
            for process in processes:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGTERM)
                    process.wait(timeout=5)


if __name__ == "__main__":
    main()
