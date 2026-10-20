#!/usr/bin/env python3
"""Real Git worktree creation via the production shared engine; private fixtures."""

import hashlib
import json
import os
import shutil
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


def wait(probe):
    end = time.monotonic() + 15
    while time.monotonic() < end:
        if value := probe():
            return value
        time.sleep(0.02)
    raise AssertionError("deadline waiting for fixture observation")


def main():
    with tempfile.TemporaryDirectory(prefix="fp-worktrees-") as temporary:
        root = Path(temporary)
        repo, store = root / "repo", root / "state"
        repo.mkdir()

        def git(*args):
            return subprocess.check_output(["git", "-C", str(repo), *args], timeout=15)

        def call(namespace, *args, succeeds=True, env=None, timeout=30):
            result = subprocess.run(
                [str(BINARY), namespace, str(store), *map(str, args)],
                capture_output=True,
                text=True,
                timeout=timeout,
                env=env,
                check=False,
            )
            assert (result.returncode == 0) == succeeds, (
                args,
                result.stdout,
                result.stderr,
            )
            if result.stdout:
                return json.loads(result.stdout)
            return result.stderr

        def plan(branch, base="main", source=None):
            return call(
                "worktree", "plan", source or source_id, branch, base, uuid.uuid4()
            )

        git("init", "-b", "main")
        git("config", "user.email", "fixture@example.invalid")
        git("config", "user.name", "Fixture")
        (repo / "sub").mkdir()
        (repo / "sub/中文.txt").write_text("base\n")
        git("add", ".")
        git("commit", "-qm", "base")
        original = git("rev-parse", "HEAD").decode().strip()
        control = root / "host/control.sock"
        config = root / "hosts.json"
        config.write_text(
            json.dumps(
                {"version": 1, "hosts": [{"name": "Codex", "socket": str(control)}]}
            )
        )
        process = subprocess.Popen(
            [
                str(BINARY),
                "serve",
                str(control),
                str(root),
                sys.executable,
                str(ROOT / "scripts/provider-fixture.py"),
            ],
            stdout=subprocess.DEVNULL,
            stderr=(root / "host.log").open("w"),
            start_new_session=True,
        )

        def snapshot():
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(5)
                stream.connect(str(control))
                stream.sendall(b'{"command":"snapshot"}\n')
                value = json.loads(stream.makefile("rb").readline(1024 * 1024))
                assert value["response"] == "snapshot", value
                return value["data"]

        wrappers = []
        try:
            wait(
                lambda: (
                    control.exists()
                    and "fleetpane-host ready:" in (root / "host.log").read_text()
                )
            )
            initial = snapshot()
            source = call("workspace", "add", config, "Codex", "git", repo / "sub")
            source_id = source["id"]
            prepared = plan("feature/中文")
            first = prepared["plan"]
            assert prepared["state"]["status"] == "prepared"
            assert first["commit"] == original and not Path(first["path"]).exists()
            assert first["cwd"] == str(Path(first["path"]) / "sub")
            assert snapshot() == initial
            (repo / "sub/中文.txt").write_text("moving base\n")
            git("commit", "-qam", "move baseline")
            (repo / "local-untracked.txt").write_text("leave this alone")
            index_hash = hashlib.sha256((repo / ".git/index").read_bytes()).digest()
            hook_marker = root / "hook-ran"
            hook = repo / ".git/hooks/post-checkout"
            hook.write_text(f"#!/bin/sh\ntouch '{hook_marker}'\n")
            hook.chmod(0o700)
            assert (
                call(
                    "worktree", "plan", source_id, "feature/中文", "main", first["id"]
                )["plan"]
                == first
            )
            call(
                "worktree",
                "plan",
                source_id,
                "other",
                "main",
                first["id"],
                succeeds=False,
            )
            arguments = [str(BINARY), "worktree", str(store), "apply", first["id"]]
            clients = [
                subprocess.Popen(
                    arguments, stdout=subprocess.PIPE, stderr=subprocess.PIPE
                )
                for _ in range(2)
            ]
            responses = [client.communicate(timeout=30) for client in clients]
            assert sorted(client.returncode for client in clients) == [0, 1], responses
            complete = call("worktree", "check", first["id"])
            assert complete["state"]["status"] == "completed"
            workspace = call("workspace", "inspect", complete["state"]["workspace_id"])
            assert (
                workspace["git"]["head"] == original
                and workspace["git"]["branch"] == "feature/中文"
            )
            assert workspace["workspace"]["project_id"] == source["project_id"]
            assert Path(first["cwd"], "中文.txt").read_text() == "base\n"
            assert (
                not Path(first["path"], "local-untracked.txt").exists()
                and not hook_marker.exists()
            )
            assert (
                hashlib.sha256((repo / ".git/index").read_bytes()).digest()
                == index_hash
            )
            call("worktree", "apply", first["id"], succeeds=False)
            assert snapshot() == initial
            call("workspace", "create", complete["state"]["workspace_id"])
            state = snapshot()
            assert state["journal_total"] == 1
            listing = subprocess.check_output(
                [str(BINARY), "session", str(control), "--take-control", "list"],
                timeout=15,
            )
            assert first["cwd"] in listing.decode()
            before_failures = snapshot()
            for branch, base in [
                ("-b", "main"),
                ("bad..branch", "main"),
                ("main", "main"),
                ("ok", "--help"),
                ("ok", "not-a-ref"),
            ]:
                call(
                    "worktree",
                    "plan",
                    source_id,
                    branch,
                    base,
                    uuid.uuid4(),
                    succeeds=False,
                )
            collision = plan("path-collision")
            destination = Path(collision["plan"]["path"])
            destination.mkdir()
            (destination / "owned.txt").write_text("preserve")
            call("worktree", "apply", collision["plan"]["id"], succeeds=False)
            assert (destination / "owned.txt").read_text() == "preserve"
            assert (
                call("worktree", "check", collision["plan"]["id"])["state"]["status"]
                == "prepared"
            )
            branch_collision = plan("branch-collision")
            git("branch", "branch-collision")
            failed = call(
                "worktree", "apply", branch_collision["plan"]["id"], succeeds=False
            )
            assert (
                failed["state"]["status"] == "failed"
                and "Residual directory" in failed["state"]["error"]
            )
            call("worktree", "apply", branch_collision["plan"]["id"], succeeds=False)
            missing = repo / "untracked-dir"
            missing.mkdir()
            missing_source = call("workspace", "add", config, "Codex", "git", missing)
            call(
                "worktree",
                "plan",
                missing_source["id"],
                "absent-subdirectory",
                "main",
                uuid.uuid4(),
                succeeds=False,
            )
            replacement = plan("source-replaced")
            (repo / ".git").rename(repo / ".git-original")
            git("init", "-b", "main")
            call("worktree", "apply", replacement["plan"]["id"], succeeds=False)
            shutil.rmtree(repo / ".git")
            (repo / ".git-original").rename(repo / ".git")
            parent_replaced = plan("parent-replaced")
            parent = Path(parent_replaced["plan"]["parent"]["path"])
            parent.rename(store / "worktrees-old")
            parent.mkdir(mode=0o700)
            call("worktree", "apply", parent_replaced["plan"]["id"], succeeds=False)
            parent.rmdir()
            (store / "worktrees-old").rename(parent)
            # Create the real worktree, then lose the controller before its reply.
            fakebin = root / "bin"
            fakebin.mkdir()
            marker = root / "effect"
            real_git = shutil.which("git")
            assert real_git
            wrapper = fakebin / "git"
            wrapper.write_text(
                f"#!{sys.executable}\nimport os,subprocess,sys,time\nfrom pathlib import Path\n"
                f"real={real_git!r}\n"
                "if 'worktree' in sys.argv and 'add' in sys.argv:\n"
                "    subprocess.run([real,*sys.argv[1:]],check=True)\n"
                f"    Path({str(marker)!r}).write_text(str(os.getpid()))\n"
                "    time.sleep(30)\n"
                "else: os.execv(real,[real,*sys.argv[1:]])\n"
            )
            wrapper.chmod(0o700)
            lost = plan("lost-controller")
            controller = subprocess.Popen(
                [str(BINARY), "worktree", str(store), "apply", lost["plan"]["id"]],
                env={**os.environ, "PATH": f"{fakebin}:{os.environ['PATH']}"},
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            try:
                pid = wait(
                    lambda: (
                        int(marker.read_text())
                        if marker.exists() and marker.read_text()
                        else None
                    )
                )
                wrappers.append(pid)
                controller.kill()
                controller.wait(timeout=5)
                unresolved = call("worktree", "check", lost["plan"]["id"])
                assert unresolved["state"]["status"] == "attempted"
                assert Path(lost["plan"]["path"], ".git").is_file()
                call("worktree", "apply", lost["plan"]["id"], succeeds=False)
                assert call("worktree", "check", lost["plan"]["id"]) == unresolved
            finally:
                if controller.poll() is None:
                    controller.kill()
                    controller.wait(timeout=5)
            partial = plan("partial-failure")
            residual = Path(partial["plan"]["path"])
            wrapper.write_text(
                f"#!{sys.executable}\nimport os,sys\nfrom pathlib import Path\n"
                "if 'worktree' in sys.argv and 'add' in sys.argv:\n"
                f"    p=Path({str(residual)!r});p.mkdir();(p/'partial.txt').write_text('preserve evidence')\n"
                "    sys.stderr.write('fixture checkout failed after creating a directory\\n');sys.exit(9)\n"
                f"else: os.execv({real_git!r},[{real_git!r},*sys.argv[1:]])\n"
            )
            environment = {**os.environ, "PATH": f"{fakebin}:{os.environ['PATH']}"}
            failed = call(
                "worktree",
                "apply",
                partial["plan"]["id"],
                succeeds=False,
                env=environment,
            )
            assert failed["state"]["status"] == "failed"
            assert "fixture checkout failed" in failed["state"]["error"]
            assert (residual / "partial.txt").read_text() == "preserve evidence"
            call("worktree", "apply", partial["plan"]["id"], succeeds=False)
            # Exercise the production deadline, including a descendant holding pipes.
            slow = plan("timeout")
            child_marker = root / "sleep-child"
            wrapper.write_text(
                f"#!{sys.executable}\nimport os,subprocess,sys,time\nfrom pathlib import Path\n"
                "if 'worktree' in sys.argv and 'add' in sys.argv:\n"
                "    child=subprocess.Popen([sys.executable,'-c','import time;time.sleep(300)'])\n"
                f"    Path({str(child_marker)!r}).write_text(str(child.pid))\n"
                "    time.sleep(300)\n"
                f"else: os.execv({real_git!r},[{real_git!r},*sys.argv[1:]])\n"
            )
            started = time.monotonic()
            failed = call(
                "worktree",
                "apply",
                slow["plan"]["id"],
                succeeds=False,
                env=environment,
                timeout=145,
            )
            assert 119 <= time.monotonic() - started < 140
            assert (
                failed["state"]["status"] == "failed"
                and "120 seconds" in failed["state"]["error"]
            )
            pid = child_marker.read_text()
            state = subprocess.run(
                ["ps", "-o", "stat=", "-p", pid],
                capture_output=True,
                text=True,
                check=False,
            )
            assert not state.stdout.strip() or state.stdout.strip().startswith("Z"), (
                state.stdout
            )
            for index in range(35):
                plan(f"page-{index}")
            page = call("worktree", "list")
            assert len(page) == 32
            older = call("worktree", "list", page[-1]["position"])
            assert older and {j["plan"]["id"] for j in page}.isdisjoint(
                j["plan"]["id"] for j in older
            )
            assert snapshot() == before_failures
        finally:
            for pid in wrappers:
                try:
                    os.killpg(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            os.killpg(process.pid, signal.SIGINT)
            process.wait(timeout=10)
            assert not control.exists()
        print(
            "PASS: pinned baseline/subdirectory, real independent worktree, no hook/main-index mutation, one concurrent dispatch, preserved partial failure, controller-kill attempted, 120s deadline kills descendants, replacement refusal, paging and Codex cwd"
        )


if __name__ == "__main__":
    main()
