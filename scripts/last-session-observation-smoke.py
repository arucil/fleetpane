#!/usr/bin/env python3
"""Revalidate last-session source/membership through actual synthetic Hosts."""
import importlib.util
import json
import subprocess
import tempfile
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("directory_fixture", ROOT / "scripts/fleet-directory-smoke.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def main():
    with tempfile.TemporaryDirectory(prefix="fp-last-observe-") as temporary:
        root = Path(temporary)
        fixture = module.Fixture(root)
        try:
            fixture.start()
            before = [fixture.snapshot(i) for i in range(4)]
            store = root / "controller"

            def run(index, *args, ok=True):
                result = subprocess.run([str(module.BINARY), "last-session", str(store), fixture.hosts[index]["socket"], *map(str, args)], capture_output=True, text=True, timeout=20)
                assert (result.returncode == 0) == ok, result.stderr
                return json.loads(result.stdout) if ok else result.stderr

            revision = 0
            def target(thread="same-201", archived=False, **overrides):
                return {"host_id": before[0]["host_id"], "thread_id": thread, "cwd": str(root / "0"), "archived": archived, **overrides}

            def save(value):
                nonlocal revision
                result = run(0, "set", revision, json.dumps(value))
                revision = result["revision"]
                return result

            saved = save(target())
            assert run(0, "check") == saved  # Third metadata page, exact source.
            saved = save(target("archived-120", True))
            assert run(0, "check") == saved  # Second page of the first workspace.
            for value, reason in [
                (target("archived-120", False), "absent"),
                (target("same-201", True), "absent"),
                (target(cwd=str(root / "1")), "absent"),
                (target(host_id=str(uuid.uuid4())), "replaced Host"),
                (target(thread="missing"), "absent"),
            ]:
                saved = save(value)
                assert reason in run(0, "check", ok=False)
                assert run(0, "get") == saved
            after = [fixture.snapshot(i) for i in range(4)]
            for old, new in zip(before, after):
                for key in ["host_id", "owner", "activity", "pending_requests", "fault"]:
                    assert old[key] == new[key], key
        finally:
            fixture.close()
    print("PASS: paged active/archived identity, changed directory/archive/Host refusal, no automatic takeover/resume/send; four Hosts retain owners/activity/pending requests")


if __name__ == "__main__":
    main()
