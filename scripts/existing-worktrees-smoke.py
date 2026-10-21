#!/usr/bin/env python3
"""Existing local branch plans through the production CLI, real private Git."""
import fcntl
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"


def main():
    with tempfile.TemporaryDirectory(prefix="fp-existing-worktrees-") as temporary:
        root = Path(temporary)
        repo, store = root / "repo", root / "state"
        repo.mkdir()

        def git(*args):
            result = subprocess.run(["git", "-C", str(repo), *map(str, args)], capture_output=True, text=True, timeout=15, check=False)
            assert result.returncode == 0, (args, result.stdout, result.stderr)
            return result.stdout.strip()

        def call(*args, succeeds=True, state=store, env=None):
            result = subprocess.run([str(BINARY), args[0], str(state), *map(str, args[1:])], capture_output=True, text=True, timeout=30, check=False, env=env)
            assert (result.returncode == 0) == succeeds, (args, result.stdout, result.stderr)
            return json.loads(result.stdout) if result.stdout else result.stderr

        config = root / "hosts.json"
        config.write_text(json.dumps({"version": 1, "hosts": [{"name": "Offline", "socket": str(root / "unused/control.sock") }]}))
        git("init", "-b", "main")
        git("config", "user.email", "fixture@example.invalid")
        git("config", "user.name", "Fixture")
        (repo / "sub").mkdir()
        (repo / "sub/中文.txt").write_text("base\n")
        git("add", ".")
        git("commit", "-qm", "base")
        base = git("rev-parse", "HEAD")
        source = call("workspace", "add", config, "Offline", "git", repo / "sub")["id"]

        def plan(branch, succeeds=True, id=None):
            return call("worktree", "plan-existing", source, branch, id or uuid.uuid4(), succeeds=succeeds)

        def unchanged_failure(job, error):
            assert error in call("worktree", "apply", job["plan"]["id"], succeeds=False)
            assert call("worktree", "check", job["plan"]["id"]) == job
            assert not Path(job["plan"]["path"]).exists()

        # Same-named tag at a DIFFERENT commit must never detach or redirect us.
        git("branch", "feature/中文")
        (repo / "sub/中文.txt").write_text("new main\n")
        git("commit", "-qam", "move main")
        git("tag", "feature/中文")
        git("update-ref", "refs/remotes/origin/remote-only", "HEAD")
        (repo / "untracked.txt").write_text("preserve source")
        index = hashlib.sha256((repo / ".git/index").read_bytes()).hexdigest()
        first = plan("feature/中文")
        assert first["plan"]["branch_mode"] == "existing" and first["plan"]["commit"] == base
        assert first["plan"]["base_ref"] == "refs/heads/feature/中文"
        assert first["plan"]["cwd"] == str(Path(first["plan"]["path"]) / "sub")
        assert plan("feature/中文", id=first["plan"]["id"]) == first
        call("worktree", "plan", source, "feature/中文", "refs/heads/feature/中文", first["plan"]["id"], succeeds=False)
        for branch in ["main", "missing", "remote-only", "origin/remote-only", "-b", "HEAD", "bad..branch", "a\nb", "a" * 256]:
            plan(branch, succeeds=False)
        result = call("worktree", "apply", first["plan"]["id"])
        assert result["state"]["status"] == "completed", result
        inspected = call("workspace", "inspect", result["state"]["workspace_id"])
        assert inspected["git"]["head"] == base and inspected["git"]["branch"] == "feature/中文"
        assert inspected["workspace"]["project_id"] == call("workspace", "inspect", source)["workspace"]["project_id"]
        assert Path(first["plan"]["cwd"], "中文.txt").read_text() == "base\n"
        assert git("rev-parse", "refs/heads/feature/中文") == base
        assert git("symbolic-ref", "HEAD") == "refs/heads/main"
        assert hashlib.sha256((repo / ".git/index").read_bytes()).hexdigest() == index
        plan("feature/中文", succeeds=False)
        call("worktree", "apply", first["plan"]["id"], succeeds=False)
        for branch, mutation in [
            ("moved", lambda: git("update-ref", "refs/heads/moved", base)),
            ("deleted", lambda: git("branch", "-D", "deleted")),
            ("occupied", lambda: git("worktree", "add", root / "occupied", "occupied")),
        ]:
            git("branch", branch)
            job = plan(branch)
            mutation()
            unchanged_failure(job, {"moved": "moved since preview", "deleted": "local branch does not exist", "occupied": "already checked out"}[branch])

        # Independent stores and different linked source roots share the lock.
        other = root / "other-state"
        linked_source = call("workspace", "add", config, "Offline", "git", root / "occupied/sub", state=other)["id"]
        git("branch", "locked")
        job = plan("locked")
        second = call("worktree", "plan-existing", linked_source, "locked", uuid.uuid4(), state=other)
        gate = repo / ".git/fleetpane-worktree-create.lock"
        with gate.open("r+b") as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            unchanged_failure(job, "holds this repository")
            error = call("worktree", "apply", second["plan"]["id"], state=other, succeeds=False)
            assert "holds this repository" in error
            assert call("worktree", "check", second["plan"]["id"], state=other) == second
        assert call("worktree", "apply", job["plan"]["id"])["state"]["status"] == "completed"
        assert "already checked out" in call("worktree", "apply", second["plan"]["id"], state=other, succeeds=False)

        # An external Git writer does not honor our guard. A late tip change is
        # a failed one-shot outcome with retained evidence, never false success.
        git("branch", "late")
        late = plan("late")
        fakebin = root / "bin"
        fakebin.mkdir()
        real_git = shutil.which("git")
        assert real_git
        wrapper = fakebin / "git"
        wrapper.write_text(
            "#!/usr/bin/env python3\nimport os,subprocess,sys\n"
            f"real={real_git!r}\n"
            "if 'worktree' in sys.argv and 'add' in sys.argv:\n"
            f"    subprocess.run([real,'-C',{str(repo)!r},'update-ref','refs/heads/late',{base!r}],check=True)\n"
            "os.execv(real,[real,*sys.argv[1:]])\n"
        )
        wrapper.chmod(0o700)
        failed = call("worktree", "apply", late["plan"]["id"], env={**os.environ, "PATH": f"{fakebin}:{os.environ['PATH']}"}, succeeds=False)
        assert failed["state"]["status"] == "failed" and "does not match its plan" in failed["state"]["error"]
        assert Path(late["plan"]["path"], ".git").is_file()
        assert git("rev-parse", "refs/heads/late") == base
        call("worktree", "apply", late["plan"]["id"], succeeds=False)
        assert not (root / "unused").exists(), "No Host was contacted or started"
    print("PASS: existing local branch, exact tip/subdirectory, same-name tag, source index/ref preservation, mode UUID binding, absent/remote/occupied/invalid refusal, moved/deleted/newly occupied preflight, cross-store linked-root guard, external late-tip failed residual and no Host contact")


if __name__ == "__main__":
    main()
