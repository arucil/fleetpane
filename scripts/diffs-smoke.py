#!/usr/bin/env python3
"""Real read-only Git comparisons through the shared CLI engine; no model calls."""

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"


def main():
    with tempfile.TemporaryDirectory(prefix="fp-diffs-") as temporary:
        root = Path(temporary)
        repo, store = root / "repo", root / "state"
        repo.mkdir()
        config = root / "hosts.json"
        config.write_text(
            json.dumps(
                {
                    "version": 1,
                    "hosts": [{"name": "Offline", "socket": str(root / "unused.sock")}],
                }
            )
        )

        def git(*args, cwd=repo, check=True):
            return subprocess.run(
                ["git", "-C", str(cwd), *args],
                capture_output=True,
                check=check,
                timeout=10,
            ).stdout

        def cli(*args, ok=True, env=None):
            result = subprocess.run(
                [str(BINARY), "workspace", str(store), *args],
                capture_output=True,
                check=False,
                timeout=45,
                env=env,
            )
            assert (result.returncode == 0) == ok, (args, result.stdout, result.stderr)
            return json.loads(result.stdout) if ok else result.stderr.decode()

        def register(path=repo):
            return cli("add", str(config), "Offline", "git", str(path))["id"]

        def diff(path, scope="unstaged", workspace=None, ok=True, env=None):
            return cli("diff", workspace or saved, scope, path, ok=ok, env=env)

        def text(path, scope="unstaged", workspace=None):
            content = diff(path, scope, workspace)["content"]
            assert content["kind"] == "patch", content
            return content["text"]

        git("init", "-q", "-b", "main")
        git("config", "user.email", "fixture@example.invalid")
        git("config", "user.name", "Fixture")
        sub = repo / "sub"
        sub.mkdir()
        (sub / "both.txt").write_text("original\n")
        (repo / "old\n中文.txt").write_text("rename me\n" * 5)
        (repo / "gone.txt").write_text("delete me\n")
        (repo / "binary").write_bytes(b"before\0binary")
        (repo / "[ab]*.txt").write_text("literal old\n")
        (repo / "a.txt").write_text("other old\n")
        (repo / "mode.sh").write_text("mode\n")
        git("add", ".")
        git("commit", "-qm", "baseline")
        saved = register(sub)
        (sub / "both.txt").write_text("index version\n")
        git("add", "sub/both.txt")
        (sub / "both.txt").write_text("working version\n")
        git("mv", "old\n中文.txt", "new\n中文.txt")
        (repo / "gone.txt").unlink()
        (repo / "binary").write_bytes(b"after\0binary")
        (repo / "[ab]*.txt").write_text("literal new\n")
        (repo / "a.txt").write_text("other new\n")
        (repo / "mode.sh").chmod(0o755)
        (repo / "new 中文.txt").write_text("untracked 中文\n")
        (repo / "empty").touch()
        (repo / "untracked-dir").mkdir()
        (repo / "untracked-dir/child").write_text("nested\n")
        secret = root / "outside"
        secret.write_text("DO NOT FOLLOW SYMLINK\n")
        (repo / "link").symlink_to(secret)
        (repo / "intent.txt").write_text("intent to add\n")
        git("add", "-N", "intent.txt")
        marker = root / "external-was-run"
        driver = root / "driver"
        driver.write_text(f"#!/bin/sh\ntouch '{marker}'\nprintf 'external output\\n'\n")
        driver.chmod(0o755)
        git("config", "diff.external", str(driver))
        git("config", "diff.fixture.textconv", str(driver))
        git("config", "diff.renames", "false")
        git("config", "diff.relative", "true")
        git("config", "diff.noprefix", "true")
        (repo / ".gitattributes").write_text("*.txt diff=fixture\n")
        before = hashlib.sha256((repo / ".git/index").read_bytes()).hexdigest()
        staged = text("sub/both.txt", "staged")
        unstaged = text("sub/both.txt")
        assert (
            "-original" in staged
            and "+index version" in staged
            and "working version" not in staged
        )
        assert (
            "-index version" in unstaged
            and "+working version" in unstaged
            and "original" not in unstaged
        )
        renamed = text("new\n中文.txt", "staged")
        assert "rename from" in renamed and "rename to" in renamed
        assert "deleted file mode" in text("gone.txt")
        assert diff("binary")["content"]["kind"] == "binary"
        assert "+untracked 中文" in text("new 中文.txt")
        assert diff("new 中文.txt", "staged")["content"]["kind"] == "empty"
        assert "new file mode" in text("empty")
        assert diff("untracked-dir/")["content"]["kind"] == "unavailable"
        link = text("link")
        assert "120000" in link and str(secret) in link and "DO NOT FOLLOW" not in link
        assert "+intent to add" in text("intent.txt")
        assert diff("intent.txt", "staged")["content"]["kind"] == "empty"
        assert "old mode" in text("mode.sh")
        literal = text("[ab]*.txt")
        assert "literal new" in literal and "other new" not in literal
        hostile = dict(
            os.environ,
            GIT_DIR=str(root / "missing"),
            GIT_INDEX_FILE=str(root / "index"),
            GIT_EXTERNAL_DIFF=str(driver),
        )
        assert (
            "+working version" in diff("sub/both.txt", env=hostile)["content"]["text"]
        )
        for path in ["../outside", str(secret), ":(glob)**", "missing.txt"]:
            diff(path, ok=False)
        assert not marker.exists()
        assert before == hashlib.sha256((repo / ".git/index").read_bytes()).hexdigest()
        # Oversized results refuse explicitly, and a later small read still succeeds.
        (repo / "long").write_text("x" * 17000)
        assert "16 KiB line" in diff("long", ok=False)
        (repo / "large").write_text("x\n" * 200000)
        assert "256 KiB" in diff("large", ok=False)
        (repo / "input-large").write_bytes(b"x" * (1024 * 1024 + 1))
        assert diff("input-large")["content"]["kind"] == "unavailable"
        (repo / "non-utf8").write_bytes(b"\xff\xfe\n")
        assert "non-UTF-8" in diff("non-utf8", ok=False)
        assert "+working version" in text("sub/both.txt")
        # External baseline moves are observed afresh; no stale file result survives.
        (repo / "new 中文.txt").unlink()
        assert "no longer" in diff("new 中文.txt", ok=False)
        unborn = root / "unborn"
        unborn.mkdir()
        git("init", "-q", cwd=unborn)
        (unborn / "first").write_text("first commit\n")
        git("add", "first", cwd=unborn)
        unborn_id = register(unborn)
        first = diff("first", "staged", unborn_id)
        assert first["head"] is None and "+first commit" in first["content"]["text"]
        # Real unmerged index, rather than inventing conflict semantics.
        conflict = root / "conflict"
        conflict.mkdir()
        git("init", "-q", "-b", "main", cwd=conflict)
        git("config", "user.email", "fixture@example.invalid", cwd=conflict)
        git("config", "user.name", "Fixture", cwd=conflict)
        target = conflict / "file"
        target.write_text("base\n")
        git("add", ".", cwd=conflict)
        git("commit", "-qm", "base", cwd=conflict)
        git("checkout", "-qb", "side", cwd=conflict)
        target.write_text("side\n")
        git("commit", "-qam", "side", cwd=conflict)
        git("checkout", "-q", "main", cwd=conflict)
        target.write_text("main\n")
        git("commit", "-qam", "main", cwd=conflict)
        git("merge", "side", cwd=conflict, check=False)
        conflict_id = register(conflict)
        for scope in ["staged", "unstaged"]:
            assert diff("file", scope, conflict_id)["content"]["kind"] == "unavailable"
        # Exercise the diff subprocess deadline itself, not just a status timeout.
        shim = root / "shim"
        shim.mkdir()
        pid_file = root / "diff-pid"
        real_git, sleep = shutil.which("git"), shutil.which("sleep")
        assert real_git and sleep
        (shim / "git").write_text(
            f"#!{sys.executable}\nimport os,sys\n"
            f"if 'diff' in sys.argv[1:]:\n open({str(pid_file)!r},'w').write(str(os.getpid()))\n os.execv({sleep!r},['sleep','300'])\n"
            f"os.execv({real_git!r},[{real_git!r},*sys.argv[1:]])\n"
        )
        (shim / "git").chmod(0o755)
        started = time.monotonic()
        error = diff(
            "sub/both.txt",
            ok=False,
            env={**os.environ, "PATH": str(shim) + os.pathsep + os.environ["PATH"]},
        )
        assert "10 second deadline" in error and 9 < time.monotonic() - started < 25
        # kill(0) is portable across the Unix CI platforms; the child was reaped.
        try:
            os.kill(int(pid_file.read_text()), 0)
        except ProcessLookupError:
            pass
        else:
            raise AssertionError("timed-out Git observation still exists")
        assert "+working version" in text("sub/both.txt")
        # Replacing the repository invalidates its saved identity.
        (repo / ".git").rename(repo / ".git-old")
        git("init", "-q")
        assert "replaced" in diff("sub/both.txt", ok=False)
        print(
            json.dumps(
                {
                    "ok": True,
                    "real_git": True,
                    "comparisons": [
                        "staged/unstaged",
                        "rename/newline/Unicode",
                        "delete",
                        "binary",
                        "untracked/empty/symlink/directory",
                        "intent-to-add",
                        "mode",
                        "literal pathspec",
                        "unborn HEAD",
                        "conflict",
                        "input/output/display limits",
                        "diff deadline and recovery",
                        "stale/replaced refusal",
                    ],
                    "external_tools_called": False,
                    "index_unchanged": True,
                    "host_contact": False,
                }
            )
        )


if __name__ == "__main__":
    main()
