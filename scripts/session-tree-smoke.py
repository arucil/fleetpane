#!/usr/bin/env python3
"""Synthetic provider ancestry/loaded inventory; never target an existing Host."""

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


def fixture(root):
    def row(id, parent=None, **extra):
        return {
            "id": id,
            "sessionId": "tree",
            "parentThreadId": parent,
            "forkedFromId": None,
            "cwd": str(root),
            "name": f"Session 中文 {id}",
            "preview": id,
            "ephemeral": False,
            "status": {"type": "idle"},
            "turns": [],
            **extra,
        }

    return [
        row("root", fixtureLoaded=True),
        row("child", "root", fixtureLoaded=True),
        row("grandchild", "child", fixtureArchived=True),
        row(
            "ephemeral", "root", ephemeral=True, fixtureListed=False, fixtureLoaded=True
        ),
        row("fork", forkedFromId="root"),
        row("orphan", "missing"),
        *[
            row(f"other-{n}", sessionId=f"other-tree-{n}", fixtureLoaded=True)
            for n in range(107)
        ],
    ]


def main():
    receipts = []
    with tempfile.TemporaryDirectory(prefix="fp-tree-") as temp:
        root = Path(temp)
        history = root / "history.json"
        control = root / "control.json"
        history.write_text(json.dumps(fixture(root)))
        control.write_text("{}")
        for acp in (False, True):
            home = root / ("acp" if acp else "codex")
            home.mkdir()
            endpoint = home / "host/control.sock"
            with (home / "host.log").open("w") as log:
                process = subprocess.Popen(
                    [
                        str(BINARY),
                        "serve-acp" if acp else "serve",
                        str(endpoint),
                        str(home),
                        sys.executable,
                        str(
                            ROOT
                            / "scripts"
                            / ("acp-fixture.py" if acp else "provider-fixture.py")
                        ),
                        *(
                            []
                            if acp
                            else [
                                "--history",
                                str(history),
                                "--tree-control",
                                str(control),
                            ]
                        ),
                    ],
                    stdout=subprocess.DEVNULL,
                    stderr=log,
                    start_new_session=True,
                )
            try:
                deadline = time.monotonic() + 10
                while (
                    not endpoint.exists()
                    or "ready:" not in (home / "host.log").read_text()
                ):
                    assert process.poll() is None and time.monotonic() < deadline, (
                        home / "host.log"
                    ).read_text()
                    time.sleep(0.02)

                def rpc(request):
                    with socket.socket(socket.AF_UNIX) as stream:
                        stream.settimeout(10)
                        stream.connect(str(endpoint))
                        stream.sendall(json.dumps(request).encode() + b"\n")
                        return json.loads(stream.makefile("rb").readline())

                def state():
                    return rpc({"command": "snapshot"})["data"]

                def run(*args):
                    return subprocess.run(
                        [str(BINARY), "session", str(endpoint), *args],
                        capture_output=True,
                        text=True,
                        timeout=35,
                        check=False,
                    )

                def mutate(*args):
                    reply = run("--take-control", *args)
                    assert reply.returncode == 0, reply.stderr

                def inspect():
                    reply = run("tree", "root")
                    assert reply.returncode == 0, reply.stderr
                    return json.loads(reply.stdout)

                if acp:
                    before = state()
                    reply = run("tree", "root")
                    assert (
                        reply.returncode != 0 and "does not advertise" in reply.stderr
                    ), reply.stderr
                    after = state()
                    assert (
                        before["owner"] == after["owner"]
                        and after["journal_total"] == 0
                    )
                    receipts.append(
                        {
                            "provider": "ACP fixture",
                            "unsupported_without_claim_or_mutation": True,
                        }
                    )
                    continue
                before = state()
                reply = run("tree", "root")
                assert reply.returncode != 0 and "No Host lease" in reply.stderr, (
                    reply.stderr
                )
                assert state()["owner"] is None and state()["journal_total"] == 0
                mutate("list")
                mutate("send", "child", "[approval]")
                mutate("send", "other-0", "[approval]")
                deadline = time.monotonic() + 5
                while len(state()["pending_requests"]) != 2:
                    assert time.monotonic() < deadline
                    time.sleep(0.02)
                before = state()
                view = inspect()
                after = state()
                for key in (
                    "host_id",
                    "host_pid",
                    "provider_pid",
                    "owner",
                    "journal_total",
                    "activity",
                    "pending_requests",
                ):
                    assert before[key] == after[key], key
                assert [r["metadata"]["id"] for r in view["rows"]] == [
                    "root",
                    "child",
                    "grandchild",
                    "ephemeral",
                ], view
                assert (
                    view["rows"][1]["pending_requests"] == 1
                    and view["rows"][1]["active_turn"]["thread_id"] == "child"
                )
                assert (
                    view["rows"][2]["archived"] is True
                    and not view["rows"][2]["loaded"]
                )
                assert (
                    view["rows"][3]["archived"] is None
                    and view["rows"][3]["metadata"]["ephemeral"]
                )
                assert (
                    view["scanned_nodes"] == 113
                    and view["other_loaded_contexts"] == 107
                )
                assert view["unresolved_same_session"] == ["orphan"]
                assert run("--take-control", "tree", "root").returncode != 0
                assert state()["owner"] == before["owner"]
                for fault, text in [
                    ({"fail": True}, "Synthetic tree inventory failure"),
                    ({"loadedIds": ["root", "root"]}, "duplicate"),
                    ({"wrongReadIdentity": "ephemeral"}, "another identity"),
                ]:
                    control.write_text(json.dumps(fault))
                    reply = run("tree", "root")
                    assert reply.returncode != 0 and text in reply.stderr, reply.stderr
                    assert state()["journal_total"] == before["journal_total"]
                control.write_text("{}")
                mutate("drain")
                before = state()
                assert len(inspect()["rows"]) == 4
                after = state()
                assert all(
                    before[k] == after[k]
                    for k in (
                        "owner",
                        "admission",
                        "journal_total",
                        "activity",
                        "pending_requests",
                    )
                )
                receipts.append(
                    {
                        "provider": "Codex fixture",
                        "catalog_nodes": 113,
                        "loaded_contexts": 110,
                        "subtree_ids": [r["metadata"]["id"] for r in view["rows"]],
                        "transitive_archived_and_ephemeral_children": True,
                        "fork_excluded": True,
                        "orphan_visible": True,
                        "loaded_and_catalog_pagination": True,
                        "in_tree_and_other_approval_preserved": True,
                        "read_only_during_drain": True,
                        "inventory_faults_refused": 3,
                        "no_claim_or_journal_mutation": True,
                    }
                )
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGINT)
                process.wait(timeout=10)
                assert process.returncode == 0 and not endpoint.exists()
    print(json.dumps(receipts, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
