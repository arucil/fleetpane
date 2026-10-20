#!/usr/bin/env python3
"""Private real Host processes: durable drafts, CAS, one-attempt send and recovery."""

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


def wait(probe):
    end = time.monotonic() + 15
    while time.monotonic() < end:
        value = probe()
        if value:
            return value
        time.sleep(0.025)
    raise AssertionError("fixture deadline")


def main():
    with tempfile.TemporaryDirectory(prefix="fp-drafts-") as temporary:
        root = Path(temporary)
        store = root / "state"
        processes = []

        def start(index, hold=False, acp=False):
            home = root / str(index)
            home.mkdir(exist_ok=True)
            endpoint = home / "host/control.sock"
            log = home / "host.log"
            arguments = (
                [sys.executable, str(ROOT / "scripts/acp-fixture.py")]
                if acp
                else [
                    sys.executable,
                    str(ROOT / "scripts/provider-fixture.py"),
                    "--send-effects",
                    str(home / "sends.jsonl"),
                ]
            )
            if hold:
                arguments.append("--hold-send")
            with log.open("w") as output:
                process = subprocess.Popen(
                    [
                        str(BINARY),
                        "serve-acp" if acp else "serve",
                        str(endpoint),
                        str(home),
                        *arguments,
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
            wait(
                lambda: (
                    process.poll() is not None
                    or (endpoint.exists() and "ready:" in log.read_text())
                )
            )
            assert process.poll() is None, log.read_text()
            return process, endpoint

        def raw(endpoint, request):
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(5)
                stream.connect(str(endpoint))
                stream.sendall((json.dumps(request) + "\n").encode())
                return json.loads(stream.makefile("rb").readline(1024 * 1024))

        def snapshot(endpoint):
            return raw(endpoint, {"command": "snapshot"})["data"]

        def session(endpoint, *args):
            result = subprocess.run(
                [str(BINARY), "session", str(endpoint), "--take-control", *args],
                capture_output=True,
                check=True,
                timeout=25,
            )
            return json.loads(result.stdout)["result"]

        def draft(*args, ok=True):
            result = subprocess.run(
                [str(BINARY), "draft", str(store), *args],
                capture_output=True,
                check=False,
                timeout=30,
            )
            assert (result.returncode == 0) == ok, (
                args[:3],
                result.stdout[-1000:],
                result.stderr[-1000:],
            )
            return json.loads(result.stdout) if ok else result.stderr.decode()

        def key(endpoint):
            thread = session(endpoint, "create", str(endpoint.parent.parent))["thread"][
                "id"
            ]
            return json.dumps(
                {
                    "socket": str(endpoint),
                    "host_id": snapshot(endpoint)["host_id"],
                    "thread_id": thread,
                    "cwd": str(endpoint.parent.parent),
                }
            )

        try:
            _, normal = start(0)
            held_process, held = start(1, hold=True)
            _, acp = start(2, acp=True)
            identity = key(normal)
            before = snapshot(normal)
            text = "第一行\n  keep whitespace 中文\n"
            saved = draft("edit", identity, "0", text)
            assert draft("get", identity)["text"] == text
            assert snapshot(normal) == before
            assert (store / "creations.sqlite3").stat().st_mode & 0o077 == 0
            attempts = [
                subprocess.Popen(
                    [
                        str(BINARY),
                        "draft",
                        str(store),
                        "edit",
                        identity,
                        str(saved["revision"]),
                        value,
                    ],
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                )
                for value in ["first", "second"]
            ]
            for process in attempts:
                process.communicate(timeout=10)
            assert sorted(process.returncode for process in attempts) == [0, 1]
            current = draft("get", identity)
            draft(
                "edit",
                identity,
                str(current["revision"]),
                "x" * (64 * 1024 + 1),
                ok=False,
            )
            current = draft("edit", identity, str(current["revision"]), text)
            draft("send", identity, str(current["revision"]), "--take-control")
            settled = draft("get", identity)
            assert settled["text"] == "" and settled["attempt"] is None
            draft(
                "send", identity, str(current["revision"]), "--take-control", ok=False
            )
            assert len((root / "0/sends.jsonl").read_text().splitlines()) == 1
            wait(lambda: not snapshot(normal)["activity"]["turns"])
            rejected = draft(
                "edit", identity, str(settled["revision"]), "[options-fail]"
            )
            draft(
                "send", identity, str(rejected["revision"]), "--take-control", ok=False
            )
            rejected = draft("get", identity)
            assert rejected["text"] == "[options-fail]" and rejected["attempt"] is None
            acp_key = key(acp)
            acp_draft = draft("edit", acp_key, "0", "ACP draft send 中文")
            draft("send", acp_key, str(acp_draft["revision"]), "--take-control")
            assert draft("get", acp_key)["text"] == ""
            held_key = key(held)
            pending = draft("edit", held_key, "0", "hold this exact text")
            controller = subprocess.Popen(
                [
                    str(BINARY),
                    "draft",
                    str(store),
                    "send",
                    held_key,
                    str(pending["revision"]),
                    "--take-control",
                ],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            try:
                effects = root / "1/sends.jsonl"
                wait(lambda: effects.exists() and effects.read_text())
                controller.kill()
                controller.communicate(timeout=5)
                pending = draft("get", held_key)
                assert pending["attempt"] is not None
                before = snapshot(held)
                observed = raw(
                    held,
                    {
                        "command": "query",
                        "lease": before["owner"],
                        "method": "thread/read",
                        "params": {
                            "threadId": json.loads(held_key)["thread_id"],
                            "includeTurns": False,
                        },
                    },
                )
                assert (
                    observed["response"] == "query"
                    and "error" not in observed["data"]["message"]
                )
                # Provider reads have response events; receipt-only checks do not.
                before = snapshot(held)
                assert draft("check", held_key)["resolution"]["status"] == "pending"
                after = snapshot(held)
                assert after == before, {
                    k: [before.get(k), after.get(k)]
                    for k in after
                    if before.get(k) != after.get(k)
                }
                draft(
                    "edit", held_key, str(pending["revision"]), "replacement", ok=False
                )
                draft(
                    "send",
                    held_key,
                    str(pending["revision"]),
                    "--take-control",
                    ok=False,
                )
                assert len(effects.read_text().splitlines()) == 1
                os.killpg(held_process.pid, signal.SIGINT)
                held_process.wait(timeout=10)
                _, restarted = start(1, hold=True)
                assert snapshot(restarted)["host_id"] != json.loads(held_key)["host_id"]
                assert draft("check", held_key)["resolution"]["status"] == "unknown"
                draft(
                    "send",
                    held_key,
                    str(pending["revision"]),
                    "--take-control",
                    ok=False,
                )
                assert len(effects.read_text().splitlines()) == 1
            finally:
                if controller.poll() is None:
                    controller.kill()
                    controller.communicate(timeout=5)
            # Stable row cursors include cleared tombstones; no large unbounded load.
            for index in range(35):
                item = json.loads(identity)
                item["thread_id"] = f"other-{index}"
                draft("edit", json.dumps(item), "0", str(index))
            page = draft("list")
            assert (
                len(page) == 32 and len(draft("list", str(page[-1]["position"]))) >= 6
            )
            print(
                json.dumps(
                    {
                        "ok": True,
                        "saved_text_exact": True,
                        "concurrent_edit": "one winner",
                        "provider_success": ["Codex", "ACP"],
                        "provider_rejection_restores_text": True,
                        "killed_controller": "pending attempt retained; no second dispatch",
                        "host_restart": "unknown; no resend",
                        "host_reads_do_not_mutate": True,
                        "pages": 32,
                    }
                )
            )
        finally:
            for process in processes:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGINT)
                    process.wait(timeout=10)


if __name__ == "__main__":
    main()
