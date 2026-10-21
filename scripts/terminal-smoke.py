#!/usr/bin/env python3
"""Real PTYs in isolated Hosts with Codex and ACP protocol fixtures; no model calls."""

import json
import os
import signal
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"


def main():
    receipts = []
    with tempfile.TemporaryDirectory(prefix="fp-terminal-", dir="/tmp") as temporary:
        root = Path(temporary)
        folder = root / "work 中文"
        folder.mkdir()
        for acp in (False, True):
            home = root / ("acp" if acp else "codex")
            home.mkdir(mode=0o700)
            endpoint, log = home / "host/control.sock", home / "host.log"
            store, config = home / "state", home / "hosts.json"
            config.write_text(
                json.dumps(
                    {"version": 1, "hosts": [{"name": "test", "socket": str(endpoint)}]}
                )
            )

            def cli(*args, ok=True):
                result = subprocess.run(
                    [str(BINARY), *map(str, args)],
                    capture_output=True,
                    text=True,
                    timeout=25,
                    check=False,
                )
                assert (result.returncode == 0) == ok, (
                    args,
                    result.stdout,
                    result.stderr,
                )
                return json.loads(result.stdout) if ok else result.stderr

            def rpc(request, endpoint=endpoint):
                with socket.socket(socket.AF_UNIX) as stream:
                    stream.settimeout(5)
                    stream.connect(str(endpoint))
                    stream.sendall(json.dumps(request).encode() + b"\n")
                    return json.loads(stream.makefile("rb").readline(1024 * 1024))

            with log.open("w") as output:
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
                    ],
                    stdout=subprocess.DEVNULL,
                    stderr=output,
                    start_new_session=True,
                )
            fault_terminal = blocked = journal_terminal = {"pid": -1}
            try:
                deadline = time.monotonic() + 15
                while not endpoint.exists() or "ready:" not in log.read_text():
                    assert process.poll() is None, log.read_text()
                    assert time.monotonic() < deadline, log.read_text()
                    time.sleep(0.02)
                workspace = cli(
                    "workspace", store, "add", config, "test", "folder", folder
                )
                wid = workspace["id"]

                def terminal(*args, ok=True, mutation=False, store=store, wid=wid):
                    return cli(
                        "terminal",
                        store,
                        wid,
                        *(["--take-control"] if mutation else []),
                        *args,
                        ok=ok,
                    )

                first = terminal("create", "shell 中文", mutation=True)
                tid = first["id"]
                if receipts:
                    assert "does not belong" in terminal(
                        "output", receipts[-1]["terminal_id"], 0, ok=False
                    )
                pid = first["pid"]
                assert first["cwd"] == str(folder)
                before = rpc({"command": "snapshot"})["data"]
                old_lease = before["owner"]
                assert before["activity"]["terminal_ids"] == [tid]
                assert len(terminal("list")) == 1
                assert rpc({"command": "snapshot"})["data"]["owner"] == old_lease

                def send(text, tid=tid):
                    current = terminal("output", tid, 0)["terminal"]
                    return terminal(
                        "input", tid, current["next_input"], text, mutation=True
                    )

                def wait_output(predicate, tid=tid):
                    deadline = time.monotonic() + 10
                    while True:
                        result = terminal("output", tid, 0)
                        if predicate(result):
                            return result
                        assert time.monotonic() < deadline, result
                        time.sleep(0.02)

                # Shell changes are private to the PTY, and no Enter is appended.
                send("printf '%s\\n' 'EXECUTED 中文 $() `literal`' > marker")
                assert not (folder / "marker").exists()
                current = terminal("output", tid, 0)["terminal"]
                terminal("key", tid, current["next_input"], "enter", mutation=True)
                wait_output(lambda _: (folder / "marker").exists())
                assert (
                    folder / "marker"
                ).read_text() == "EXECUTED 中文 $() `literal`\n"
                (folder / "marker").unlink()
                assert "input sequence changed" in terminal(
                    "input", tid, 0, "DUPLICATE", mutation=True, ok=False
                )
                terminal("resize", tid, 33, 101, mutation=True)
                send("stty size; printf 'MULTI1\\n'; printf 'MULTI2\\n'\n")
                output = wait_output(
                    lambda r: (
                        b"33 101" in bytes(r["bytes"])
                        and b"MULTI2\r\n" in bytes(r["bytes"])
                    )
                )
                assert b"MULTI1\r\n" in bytes(output["bytes"])
                terminal("rename", tid, "renamed 中文", mutation=True)
                assert terminal("list")[0]["label"] == "renamed 中文"
                assert terminal("list")[0]["pid"] == pid
                # Explicit Ctrl-C interrupts only the foreground job; the shell survives.
                send("sleep 60 & echo $! > background-pid; wait\n")
                wait_output(lambda _: (folder / "background-pid").exists())
                background = int((folder / "background-pid").read_text())
                current = terminal("output", tid, 0)["terminal"]
                terminal("key", tid, current["next_input"], "interrupt", mutation=True)
                send("printf 'INTERRUPTED_OK\\n'\n")
                wait_output(lambda r: b"INTERRUPTED_OK\r\n" in bytes(r["bytes"]))
                # This command intentionally started a background job; terminate it
                # explicitly, since closing a PTY is not a process-tree sandbox.
                send(f"kill {background}; wait {background}; rm background-pid\n")

                # A second workspace/Host cannot read or operate the first terminal.
                other_folder = home / "other"
                other_folder.mkdir()
                other = cli(
                    "workspace", store, "add", config, "test", "folder", other_folder
                )
                assert cli("terminal", store, other["id"], "list") == []
                assert "does not belong" in cli(
                    "terminal", store, other["id"], "output", tid, 0, ok=False
                )
                # An old controller is fenced after the explicit CLI takeover.
                assert (
                    rpc(
                        {
                            "command": "query",
                            "lease": old_lease,
                            "method": "fleetpane/terminals",
                            "params": {"action": "list", "workspace_id": wid},
                        }
                    )["data"]["code"]
                    == "stale_controller"
                )

                # Drain rejects creation while the existing terminal remains usable.
                cli("session", endpoint, "--take-control", "drain")
                assert "host_draining" in terminal(
                    "create", "forbidden", mutation=True, ok=False
                )
                send("printf 'STILL_ALIVE\\n'\n")
                wait_output(lambda r: b"STILL_ALIVE\r\n" in bytes(r["bytes"]))
                cli("session", endpoint, "--take-control", "ready")
                # Capacity includes exited but unclosed shells. Closing one frees a
                # slot; its close must kill its own foreground command only.
                second = terminal("create", "close test", mutation=True)
                terminal(
                    "input",
                    second["id"],
                    0,
                    "sh -c 'echo $$ > close-pid; exec sleep 60'\n",
                    mutation=True,
                )
                deadline = time.monotonic() + 10
                while not (folder / "close-pid").exists():
                    assert time.monotonic() < deadline
                    time.sleep(0.02)
                foreground_pid = int((folder / "close-pid").read_text())
                terminal("close", second["id"], mutation=True)
                assert terminal("output", second["id"], 0)["terminal"]["closed"]
                deadline = time.monotonic() + 5
                while Path(f"/proc/{foreground_pid}").exists():
                    # Linux can briefly retain an already dead orphan as a zombie.
                    status = Path(f"/proc/{foreground_pid}/stat").read_text().split()[2]
                    if status == "Z":
                        break
                    assert time.monotonic() < deadline, (
                        "foreground process survived close"
                    )
                    time.sleep(0.02)
                (folder / "close-pid").unlink()
                # A process that never reads stdin fills the PTY input queue.
                # Its write deadline fences further input, while Host reads stay live.
                blocked = terminal("create", "blocked input", mutation=True)
                bid = blocked["id"]
                terminal(
                    "input",
                    bid,
                    0,
                    "stty -icanon -echo; touch blocked-ready; sleep 60\n",
                    mutation=True,
                )
                deadline = time.monotonic() + 10
                while not (folder / "blocked-ready").exists():
                    assert time.monotonic() < deadline
                    time.sleep(0.02)
                import uuid

                write_fault = None
                for _ in range(32):
                    current = terminal("output", bid, 0)["terminal"]
                    owner = rpc({"command": "snapshot"})["data"]["owner"]
                    operation = str(uuid.uuid4())
                    admitted = rpc(
                        {
                            "command": "execute",
                            "lease": owner,
                            "operation_id": operation,
                            "method": "fleetpane/terminal/input",
                            "params": {
                                "action": "input",
                                "workspace_id": wid,
                                "id": bid,
                                "expected_input": current["next_input"],
                                "bytes": [120] * 4096,
                            },
                        }
                    )
                    assert admitted["response"] == "operation", admitted
                    started = time.monotonic()
                    assert (
                        rpc({"command": "snapshot"})["data"]["host_id"]
                        == before["host_id"]
                    )
                    assert time.monotonic() - started < 0.75, "blocked PTY stalled Host"
                    deadline = time.monotonic() + 5
                    while True:
                        receipt = rpc(
                            {"command": "operation", "operation_id": operation}
                        )["data"]
                        if receipt["outcome"]["state"] == "finished":
                            break
                        assert time.monotonic() < deadline
                        time.sleep(0.02)
                    write_fault = terminal("output", bid, 0)["terminal"]["input_fault"]
                    if write_fault:
                        break
                assert write_fault and "may be partially written" in write_fault
                current = terminal("output", bid, 0)["terminal"]
                assert "uncertain" in terminal(
                    "input",
                    bid,
                    current["next_input"],
                    "replay",
                    mutation=True,
                    ok=False,
                )
                terminal("close", bid, mutation=True)
                (folder / "blocked-ready").unlink()
                extras = [
                    terminal("create", f"capacity {n}", mutation=True)
                    for n in range(15)
                ]
                assert "limit" in terminal(
                    "create", "over capacity", mutation=True, ok=False
                )
                for extra in extras:
                    terminal("close", extra["id"], mutation=True)

                # A detached client never owns the shell. Independent calls above
                # already exit; delayed output still arrives under the same PID.
                send("sleep 0.1; printf 'AFTER_DISCONNECT\\n'\n")
                output = wait_output(
                    lambda r: b"AFTER_DISCONNECT\r\n" in bytes(r["bytes"])
                )
                assert output["terminal"]["pid"] == pid
                send(
                    "head -c 100000 /dev/zero | tr '\\000' x; printf '\\nFLOOD_DONE\\n'\n"
                )
                output = wait_output(lambda r: b"FLOOD_DONE\r\n" in bytes(r["bytes"]))
                assert len(output["bytes"]) == 65536 and output["dropped"] > 0
                assert output["end"] - output["start"] == len(output["bytes"])
                assert terminal("output", tid, output["end"])["bytes"] == []

                # Durable dedup survives a deliberately lost controller reply.
                op = str(uuid.uuid4())
                state = rpc({"command": "snapshot"})["data"]
                command = {
                    "command": "execute",
                    "lease": state["owner"],
                    "operation_id": op,
                    "method": "fleetpane/terminal/input",
                    "params": {
                        "action": "input",
                        "workspace_id": wid,
                        "id": tid,
                        "expected_input": output["terminal"]["next_input"],
                        "bytes": list(b"printf once >> once\n"),
                    },
                }
                with socket.socket(socket.AF_UNIX) as stream:
                    stream.connect(str(endpoint))
                    stream.sendall(json.dumps(command).encode() + b"\n")
                deadline = time.monotonic() + 10
                while True:
                    receipt = rpc({"command": "operation", "operation_id": op})
                    if (
                        receipt.get("response") == "operation"
                        and receipt["data"]["outcome"]["state"] == "finished"
                    ):
                        break
                    assert time.monotonic() < deadline, receipt
                    time.sleep(0.02)
                assert rpc(command)["data"] == receipt["data"]
                wait_output(lambda _: (folder / "once").exists())
                assert (folder / "once").read_text() == "once"
                (folder / "once").unlink()

                # Actual process exit, not output silence, supplies the code.
                send("exit 7\n")
                output = wait_output(
                    lambda r: (
                        r["terminal"]["exit"] is not None
                        and r["terminal"]["output_eof"]
                    )
                )
                assert output["terminal"]["exit"] == {"code": 7, "signal": None}
                terminal("close", tid, mutation=True)
                assert next(t for t in terminal("list") if t["id"] == tid)["closed"]
                assert (
                    rpc({"command": "snapshot"})["data"]["activity"]["terminal_ids"]
                    == []
                )
                try:
                    os.kill(pid, 0)
                except ProcessLookupError:
                    pass
                else:
                    raise AssertionError("exited shell not reaped")
                # Replacing the saved directory is caught before shell creation.
                folder.rename(root / "old")
                folder.mkdir()
                assert "replaced" in terminal(
                    "create", "replacement", mutation=True, ok=False
                )
                folder.rmdir()
                (root / "old").rename(folder)
                after = rpc({"command": "snapshot"})["data"]
                assert (
                    before["host_id"] == after["host_id"]
                    and before["provider_pid"] == after["provider_pid"]
                )
                # Keep a terminal write in flight while the independent provider
                # dies. Its durable receipt must finish, never become Unknown.
                fault_terminal = terminal(
                    "create", "provider fault boundary", mutation=True
                )
                blocked = terminal(
                    "create", "pending at provider failure", mutation=True
                )
                bid = blocked["id"]
                send("stty -icanon -echo; touch fault-blocked; sleep 60\n", tid=bid)
                wait_output(lambda _: (folder / "fault-blocked").exists(), tid=bid)
                pending = None
                for _ in range(32):
                    current = terminal("output", bid, 0)["terminal"]
                    pending = str(uuid.uuid4())
                    admitted = rpc(
                        {
                            "command": "execute",
                            "lease": rpc({"command": "snapshot"})["data"]["owner"],
                            "operation_id": pending,
                            "method": "fleetpane/terminal/input",
                            "params": {
                                "action": "input",
                                "workspace_id": wid,
                                "id": bid,
                                "expected_input": current["next_input"],
                                "bytes": [120] * 4096,
                            },
                        }
                    )
                    assert admitted["response"] == "operation", admitted
                    time.sleep(0.05)
                    outcome = rpc({"command": "operation", "operation_id": pending})[
                        "data"
                    ]["outcome"]
                    if outcome["state"] == "accepted":
                        break
                assert outcome["state"] == "accepted", (
                    "did not create a pending PTY write"
                )
                owner_at_failure = rpc({"command": "snapshot"})["data"]["owner"]
                assert (
                    str(ROOT / "scripts")
                    in Path(f"/proc/{after['provider_pid']}/cmdline").read_text()
                )
                os.kill(after["provider_pid"], signal.SIGTERM)
                deadline = time.monotonic() + 5
                while not rpc({"command": "snapshot"})["data"]["fault"]:
                    assert time.monotonic() < deadline
                    time.sleep(0.02)
                assert rpc({"command": "snapshot"})["data"]["fault_scope"] == "provider"
                while True:
                    outcome = rpc({"command": "operation", "operation_id": pending})[
                        "data"
                    ]["outcome"]
                    assert outcome["state"] != "unknown", outcome
                    if outcome["state"] == "finished":
                        break
                    assert time.monotonic() < deadline
                    time.sleep(0.02)
                assert (
                    "may be partially written"
                    in terminal("output", bid, 0)["terminal"]["input_fault"]
                )
                assert terminal("close", bid, mutation=True)["closed"]
                (folder / "fault-blocked").unlink()
                assert (
                    terminal("output", fault_terminal["id"], 0)["terminal"]["pid"]
                    == fault_terminal["pid"]
                )

                # Every local mutation and the exact operation UUID dedup still
                # work after provider loss. Unknown methods still target provider.
                terminal("resize", fault_terminal["id"], 31, 93, mutation=True)
                terminal(
                    "rename", fault_terminal["id"], "still owned 中文", mutation=True
                )
                current = terminal("output", fault_terminal["id"], 0)["terminal"]
                post_fault_op = str(uuid.uuid4())
                post_fault = {
                    "command": "execute",
                    "lease": rpc({"command": "snapshot"})["data"]["owner"],
                    "operation_id": post_fault_op,
                    "method": "fleetpane/terminal/input",
                    "params": {
                        "action": "input",
                        "workspace_id": wid,
                        "id": fault_terminal["id"],
                        "expected_input": current["next_input"],
                        "bytes": list(b"stty size; printf once >> fault-once\n"),
                    },
                }
                with socket.socket(socket.AF_UNIX) as stream:
                    stream.connect(str(endpoint))
                    stream.sendall(json.dumps(post_fault).encode() + b"\n")
                deadline = time.monotonic() + 5
                while True:
                    recovered = rpc(
                        {"command": "operation", "operation_id": post_fault_op}
                    )
                    if (
                        recovered.get("response") == "operation"
                        and recovered["data"]["outcome"]["state"] == "finished"
                    ):
                        break
                    assert time.monotonic() < deadline, recovered
                    time.sleep(0.02)
                assert rpc(post_fault) == recovered
                wait_output(
                    lambda r: (
                        b"31 93" in bytes(r["bytes"])
                        and (folder / "fault-once").exists()
                    ),
                    tid=fault_terminal["id"],
                )
                assert (folder / "fault-once").read_text() == "once"
                (folder / "fault-once").unlink()
                post_fault["lease"] = owner_at_failure
                assert rpc(post_fault)["data"]["code"] == "stale_controller"
                post_fault["lease"] = rpc({"command": "snapshot"})["data"]["owner"]
                post_fault["operation_id"] = str(uuid.uuid4())
                post_fault["method"] = "fleetpane/terminal/future"
                assert rpc(post_fault)["data"]["code"] == "provider_unavailable"
                cli("session", endpoint, "--take-control", "drain")
                assert "host_draining" in terminal(
                    "create", "no new work", mutation=True, ok=False
                )
                send("printf DRAINED_OK\n", tid=fault_terminal["id"])
                wait_output(
                    lambda r: b"DRAINED_OK" in bytes(r["bytes"]),
                    tid=fault_terminal["id"],
                )
                cli("session", endpoint, "--take-control", "ready")
                created_after_fault = terminal(
                    "create", "created after provider exit", mutation=True
                )
                assert terminal("close", created_after_fault["id"], mutation=True)[
                    "closed"
                ]
                assert terminal("close", fault_terminal["id"], mutation=True)["closed"]
                assert not Path(f"/proc/{fault_terminal['pid']}").exists()

                # Storage failure is a different fault domain: no local mutation
                # may bypass the durable journal, even after the lock is removed.
                journal_terminal = terminal("create", "journal fence", mutation=True)
                with sqlite3.connect(
                    str(endpoint) + ".journal/operations.sqlite3", timeout=0
                ) as database:
                    database.execute("BEGIN EXCLUSIVE")
                    assert "journal_unavailable" in terminal(
                        "rename",
                        journal_terminal["id"],
                        "not committed",
                        mutation=True,
                        ok=False,
                    )
                    database.rollback()
                assert rpc({"command": "snapshot"})["data"]["fault_scope"] == "host"
                assert "host_unavailable" in terminal(
                    "close", journal_terminal["id"], mutation=True, ok=False
                )
                assert (
                    terminal("output", journal_terminal["id"], 0)["terminal"]["label"]
                    == "journal fence"
                )
                assert "host_unavailable" in cli(
                    "session", endpoint, "--take-control", "drain", ok=False
                )
                receipts.append(
                    {
                        "provider": "ACP fixture" if acp else "Codex fixture",
                        "host_id": after["host_id"],
                        "host_pid": process.pid,
                        "provider_pid": after["provider_pid"],
                        "shell_pid": pid,
                        "fault_shell_pid": fault_terminal["pid"],
                        "terminal_id": tid,
                        "blocked_input_fenced": True,
                        "provider_fault_boundary": "local PTY create/input/resize/rename/close and dedup remain available",
                        "pending_at_provider_failure": pending,
                        "post_fault_lost_reply_operation": post_fault_op,
                        "journal_fault_fenced": True,
                        "journal_shell_pid": journal_terminal["pid"],
                        "dropped_bytes": output["dropped"],
                        "exit": output["terminal"]["exit"],
                        "lost_reply_operation": op,
                        "status": "passed",
                    }
                )
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGINT)
                    process.wait(timeout=10)
                assert process.returncode == 0, log.read_text()
                deadline = time.monotonic() + 5
                while any(
                    Path(f"/proc/{t['pid']}").exists()
                    for t in (fault_terminal, blocked, journal_terminal)
                ):
                    assert time.monotonic() < deadline, (
                        "owned shell survived Host teardown"
                    )
                    time.sleep(0.02)
    print(json.dumps(receipts, indent=2))


if __name__ == "__main__":
    main()
