#!/usr/bin/env python3
"""Private Hosts: exact-turn follow-ups, durable drafts and no automatic replay."""

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


def wait(probe):
    end = time.monotonic() + 15
    while time.monotonic() < end:
        if value := probe():
            return value
        time.sleep(0.025)
    raise AssertionError("steering fixture deadline")


def raw(endpoint, request):
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(5)
        stream.connect(str(endpoint))
        stream.sendall((json.dumps(request) + "\n").encode())
        return json.loads(stream.makefile("rb").readline(1024 * 1024))


def run(arguments, ok=True):
    result = subprocess.run(
        [str(BINARY), *map(str, arguments)],
        capture_output=True,
        text=True,
        timeout=25,
        check=False,
    )
    assert (result.returncode == 0) == ok, (arguments, result.stderr[-1500:])
    return json.loads(result.stdout) if ok else result.stderr


def main():
    processes = []
    with tempfile.TemporaryDirectory(prefix="fp-steer-", dir="/tmp") as directory:
        root = Path(directory)
        store = root / "store"

        def start(index, hold=False, acp=False):
            home = root / str(index)
            home.mkdir(exist_ok=True)
            endpoint = home / "host/control.sock"
            log = home / "host.log"
            arguments = [
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
            ]
            if not acp:
                arguments += ["--steer-effects", str(home / "steers.jsonl")]
            if hold:
                arguments.append("--hold-steer")
            with log.open("w") as output:
                process = subprocess.Popen(
                    arguments,
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
            wait(
                lambda: (
                    process.poll() is not None
                    or (endpoint.exists() and "ready:" in log.read_text())
                )
            )
            assert process.poll() is None, log.read_text()
            return process, endpoint

        def snap(endpoint):
            return raw(endpoint, {"command": "snapshot"})["data"]

        def session(endpoint, *args, ok=True):
            value = run(["session", endpoint, "--take-control", *args], ok=ok)
            return (
                value["result"] if ok and args[0] not in ("drain", "ready") else value
            )

        def draft(*args, ok=True):
            return run(["draft", store, *args], ok=ok)

        def create(endpoint, prompt="[wait]"):
            thread = session(endpoint, "create", str(root))["thread"]["id"]
            turn = session(endpoint, "send", thread, prompt)["turn"]["id"]
            wait(
                lambda: any(
                    t["turn_id"] == turn for t in snap(endpoint)["activity"]["turns"]
                )
            )
            key = json.dumps(
                {
                    "socket": str(endpoint),
                    "host_id": snap(endpoint)["host_id"],
                    "thread_id": thread,
                    "cwd": str(root),
                }
            )
            return thread, turn, key

        def save(key, text):
            current = draft("get", key)
            return draft("edit", key, str(current["revision"]), text)

        try:
            _, endpoint = start(0)
            thread, turn, key = create(endpoint)
            provider = snap(endpoint)["provider_pid"]
            text = "追加中文\n  keep indentation \n"
            saved = save(key, text)
            assert draft(
                "steer", key, str(saved["revision"]), turn, "--take-control"
            ) == {"turnId": turn}
            assert draft("get", key)["text"] == ""
            history = session(endpoint, "open", thread)["thread"]
            assert len(history["turns"]) == 1 and history["turns"][0]["id"] == turn
            assert (
                sum(
                    i.get("content") == [{"type": "text", "text": text}]
                    for i in history["turns"][0]["items"]
                )
                == 1
            )
            stale = snap(endpoint)["owner"]
            session(endpoint, "open", thread)  # Explicit controller replacement.
            request = {
                "command": "execute",
                "lease": snap(endpoint)["owner"],
                "operation_id": str(uuid.uuid4()),
                "method": "turn/steer",
                "params": {
                    "threadId": thread,
                    "expectedTurnId": turn,
                    "input": [{"type": "text", "text": "same UUID once"}],
                },
            }
            raw(endpoint, request)
            wait(
                lambda: (
                    raw(
                        endpoint,
                        {
                            "command": "operation",
                            "operation_id": request["operation_id"],
                        },
                    )["data"]["outcome"]["state"]
                    == "finished"
                )
            )
            raw(endpoint, request)
            effects = (endpoint.parent.parent / "steers.jsonl").read_text().splitlines()
            assert len(effects) == 2
            old = {**request, "lease": stale, "operation_id": str(uuid.uuid4())}
            assert raw(endpoint, old)["response"] == "error"
            session(endpoint, "drain")
            denied = {
                **request,
                "lease": snap(endpoint)["owner"],
                "operation_id": str(uuid.uuid4()),
            }
            assert raw(endpoint, denied)["response"] == "error"
            session(endpoint, "ready")
            saved = save(key, "must not reach a different turn")
            draft(
                "steer",
                key,
                str(saved["revision"]),
                "wrong-turn",
                "--take-control",
                ok=False,
            )
            assert (
                draft("get", key)["text"] == saved["text"]
                and draft("get", key)["attempt"] is None
            )
            assert (
                len((endpoint.parent.parent / "steers.jsonl").read_text().splitlines())
                == 2
            )
            saved = save(key, "[bad-steer-ack]")
            draft(
                "steer", key, str(saved["revision"]), turn, "--take-control", ok=False
            )
            assert draft("get", key)["attempt"] is not None
            draft("check", key, ok=False)
            assert draft("get", key)["text"] == "[bad-steer-ack]"
            assert snap(endpoint)["provider_pid"] == provider
            session(endpoint, "stop", thread, turn)

            # Pending approvals remain separate from a follow-up acknowledgement.
            _, control = start(1)
            thread, turn, key = create(control, "[approval]")
            wait(lambda: snap(control)["pending_requests"])
            pending = snap(control)["pending_requests"]
            saved = save(key, "Please reconsider; do not approve this tool")
            draft("steer", key, str(saved["revision"]), turn, "--take-control")
            assert snap(control)["pending_requests"] == pending
            session(control, "stop", thread, turn)
            session(control, "steer", thread, turn, "after stop", ok=False)
            assert len(session(control, "open", thread)["thread"]["turns"]) == 1

            _, acp = start(2, acp=True)
            thread, turn, key = create(acp, "[tasks-wait]")
            saved = save(key, "Unsupported must not become a new prompt")
            draft(
                "steer", key, str(saved["revision"]), turn, "--take-control", ok=False
            )
            assert (
                draft("get", key)["text"] == saved["text"]
                and draft("get", key)["attempt"] is None
            )
            assert len(session(acp, "open", thread)["thread"]["turns"]) == 1
            session(acp, "stop", thread, turn)

            held, control = start(3, hold=True)
            thread, turn, key = create(control)
            saved = save(key, text)
            controller = subprocess.Popen(
                [
                    str(BINARY),
                    "draft",
                    str(store),
                    "steer",
                    key,
                    str(saved["revision"]),
                    turn,
                    "--take-control",
                ],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            try:
                wait(lambda: (control.parent.parent / "steers.jsonl").exists())
                controller.kill()
                controller.wait(timeout=5)
                retained = draft("get", key)
                assert retained["attempt"] is not None and retained["text"] == text
                before = snap(control)
                assert draft("check", key)["resolution"]["status"] == "pending"
                assert snap(control) == before
                draft(
                    "steer",
                    key,
                    str(retained["revision"]),
                    turn,
                    "--take-control",
                    ok=False,
                )
                assert (
                    len(
                        (control.parent.parent / "steers.jsonl")
                        .read_text()
                        .splitlines()
                    )
                    == 1
                )
                os.killpg(held.pid, signal.SIGINT)
                held.wait(timeout=8)
                _, control = start(3, hold=True)
                checked = draft("check", key)
                assert checked["resolution"]["status"] == "unknown"
                assert (
                    len(
                        (control.parent.parent / "steers.jsonl")
                        .read_text()
                        .splitlines()
                    )
                    == 1
                )
            finally:
                if controller.poll() is None:
                    controller.kill()
                    controller.wait(timeout=5)
            print(
                json.dumps(
                    {
                        "ok": True,
                        "exact_turn_no_new_turn": True,
                        "text_exact": True,
                        "same_UUID_once": True,
                        "stale_drain_refused": True,
                        "pending_approval_unchanged": True,
                        "wrong_ack_preserves_attempt": True,
                        "ACP_unsupported_no_start": True,
                        "controller_kill_host_restart_no_replay": True,
                    }
                )
            )
        finally:
            for process in processes:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGINT)
            for process in processes:
                try:
                    process.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait(timeout=5)


if __name__ == "__main__":
    main()
