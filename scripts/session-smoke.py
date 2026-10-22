#!/usr/bin/env python3
"""Exercise conversation commands through the same adapter as the desktop.

Synthetic provider only; no accounts, model requests or existing sessions.
"""

import json
import os
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path


def eventually(probe, description):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        value = probe()
        if value:
            return value
        time.sleep(0.05)
    raise AssertionError(f"deadline exceeded: {description}")


def main():
    binary = Path(__file__).resolve().parents[1] / "target/debug/fleetpane-host"
    fixture = Path(__file__).with_name("provider-fixture.py").resolve()
    with tempfile.TemporaryDirectory(prefix="fleetpane-session-") as directory:
        root = Path(directory)
        socket = root / "private/control.sock"
        with (root / "host.log").open("w+") as log:
            process = subprocess.Popen(
                [
                    str(binary),
                    "serve",
                    str(socket),
                    str(root),
                    sys.executable,
                    str(fixture),
                ],
                stdout=subprocess.DEVNULL,
                stderr=log,
                start_new_session=True,
            )
            try:
                eventually(
                    lambda: (
                        socket.exists() and "ready:" in (root / "host.log").read_text()
                    ),
                    "Host readiness",
                )

                def raw(value):
                    result = subprocess.run(
                        [str(binary), "request", str(socket)],
                        input=json.dumps(value),
                        capture_output=True,
                        text=True,
                        timeout=8,
                        check=False,
                    )
                    assert result.stdout, result.stderr
                    return json.loads(result.stdout)

                def state():
                    response = raw({"command": "snapshot"})
                    assert response["response"] == "snapshot", response
                    return response["data"]

                def command(*args, takeover=True):
                    result = subprocess.run(
                        [
                            str(binary),
                            "session",
                            str(socket),
                            *(["--take-control"] if takeover else []),
                            *args,
                        ],
                        capture_output=True,
                        text=True,
                        timeout=25,
                        check=False,
                    )
                    assert result.returncode == 0, result.stderr
                    reply = json.loads(result.stdout)
                    assert reply["sequence"] > 0
                    return reply

                def opened():
                    return command("open", thread_id)["result"]["thread"]

                def completed():
                    thread = opened()
                    return (
                        thread
                        if thread["turns"][-1]["status"] != "inProgress"
                        else None
                    )

                initial = state()
                thread_id = command("create", str(root), takeover=False)["result"][
                    "thread"
                ]["id"]
                first_owner = state()["owner"]
                listed = command("list")["result"]["data"]
                assert listed[0]["id"] == thread_id
                refused = subprocess.run(
                    [str(binary), "session", str(socket), "list"],
                    capture_output=True,
                    text=True,
                    check=False,
                )
                assert refused.returncode == 1 and "--take-control" in refused.stderr
                cursor = state()["sequence"]
                sent = command("send", thread_id, "你好\nsecond line")
                thread = eventually(completed, "streamed reply")
                assert (
                    thread["turns"][0]["items"][1]["text"] == "Echo: 你好\nsecond line"
                )
                events = raw(
                    {
                        "command": "events",
                        "host_id": initial["host_id"],
                        "after": cursor,
                    }
                )["data"]
                assert any(
                    event["message"].get("method") == "item/agentMessage/delta"
                    for event in events
                )
                assert any(
                    event["sequence"] < sent["sequence"]
                    and event["message"].get("method") == "item/completed"
                    for event in events
                ), "pre-response item must be replayed"
                turn_id = command("send", thread_id, "[wait]")["result"]["turn"]["id"]
                assert opened()["turns"][-1]["status"] == "inProgress"
                command("stop", thread_id, turn_id)
                assert (
                    eventually(completed, "interrupt")["turns"][-1]["status"]
                    == "interrupted"
                )
                for decision in ("decline", "accept"):
                    command("send", thread_id, "[approval]")
                    request = eventually(
                        lambda: state()["pending_requests"], "approval retained"
                    )[0]
                    command(
                        "list"
                    )  # Controller replacement must preserve the approval.
                    current = state()
                    stale = raw(
                        {
                            "command": "respond",
                            "lease": first_owner,
                            "request_id": request["id"],
                            "result": {"decision": decision},
                        }
                    )
                    assert stale["data"]["code"] == "stale_controller"
                    answered = raw(
                        {
                            "command": "respond",
                            "lease": current["owner"],
                            "request_id": request["id"],
                            "result": {"decision": decision},
                        }
                    )
                    assert answered["response"] == "answered", answered
                    thread = eventually(completed, "approval result")
                    assert (
                        thread["turns"][-1]["items"][-1]["text"]
                        == f"Echo: Approval: {decision}"
                    )
                    assert not state()["pending_requests"]
                for decision in ("cancel", "accept"):
                    command("send", thread_id, "[offered-approval]")
                    pending = eventually(
                        lambda: state()["pending_requests"], "offered approval retained"
                    )[0]
                    before = state()
                    for invalid in ("decline", {"accept": None}):
                        unoffered = raw({
                            "command": "respond", "lease": before["owner"],
                            "request_id": pending["id"], "result": {"decision": invalid},
                        })
                        assert unoffered["response"] == "error", unoffered
                        assert unoffered["data"]["code"] == "invalid_approval", unoffered
                    after = state()
                    for key in ("host_id", "provider_pid", "owner", "sequence", "pending_requests", "journal_total", "operation_ids", "activity"):
                        assert after[key] == before[key], key
                    response = {
                        "command": "respond", "lease": before["owner"],
                        "request_id": pending["id"], "result": {"decision": decision},
                    }
                    assert raw(response)["response"] == "answered"
                    assert raw(response)["data"]["code"] == "unknown_request"
                    thread = eventually(completed, "offered decision reaches provider")
                    assert thread["turns"][-1]["items"][-1]["text"] == f"Echo: Approval: {decision}"
                    assert not state()["pending_requests"]
                for custom in (False, True):
                    command("send", thread_id, "[questions]")
                    pending = eventually(
                        lambda: state()["pending_requests"], "questions retained"
                    )[0]
                    old_owner = state()["owner"]
                    # Another controller can read/reopen without resolving the question.
                    command("open", thread_id)
                    assert state()["pending_requests"][0] == pending
                    response = {
                        "answers": {
                            "language": {
                                "answers": ["None of the above", "user_note: 自定义"]
                                if custom
                                else ["Rust (Recommended)"]
                            },
                            "notes": {"answers": ["user_note: 中文\n第二行"]},
                        }
                    }
                    stale = raw(
                        {
                            "command": "respond",
                            "lease": old_owner,
                            "request_id": pending["id"],
                            "result": response,
                        }
                    )
                    assert stale["data"]["code"] == "stale_controller"

                    def answer(request_id, payload):
                        return subprocess.run(
                            [
                                str(binary),
                                "session",
                                str(socket),
                                "--take-control",
                                "answer",
                                json.dumps(request_id),
                            ],
                            input=json.dumps(payload),
                            capture_output=True,
                            text=True,
                            timeout=8,
                            check=False,
                        )

                    invalid = answer(
                        pending["id"], {"answers": {"wrong": {"answers": ["bad"]}}}
                    )
                    assert (
                        invalid.returncode != 0
                        and "answer every question" in invalid.stderr
                    )
                    assert state()["pending_requests"][0] == pending
                    accepted = answer(pending["id"], response)
                    assert accepted.returncode == 0, accepted.stderr
                    assert json.loads(accepted.stdout) == {"answered": True}
                    duplicate = answer(pending["id"], response)
                    assert (
                        duplicate.returncode != 0
                        and "no longer pending" in duplicate.stderr
                    )
                    thread = eventually(completed, "agent continues after answers")
                    echoed = thread["turns"][-1]["items"][-1]["text"]
                    assert (
                        json.loads(echoed.removeprefix("Echo: Answers received: "))
                        == response
                    )
                    assert not state()["pending_requests"]
                turn_id = command("send", thread_id, "[questions]")["result"]["turn"][
                    "id"
                ]
                eventually(
                    lambda: state()["pending_requests"], "question before interrupt"
                )
                command("stop", thread_id, turn_id)
                assert (
                    eventually(completed, "interrupted question")["turns"][-1]["status"]
                    == "interrupted"
                )
                eventually(
                    lambda: not state()["pending_requests"],
                    "provider resolves interrupted question",
                )
                command("send", thread_id, "[fail]")
                assert (
                    eventually(completed, "failed turn")["turns"][-1]["error"][
                        "message"
                    ]
                    == "Synthetic turn failure"
                )
                turn_id = command("send", thread_id, "[approval]")["result"]["turn"][
                    "id"
                ]
                eventually(
                    lambda: state()["pending_requests"], "approval before interrupt"
                )
                command("stop", thread_id, turn_id)
                assert (
                    eventually(completed, "interrupted approval")["turns"][-1]["status"]
                    == "interrupted"
                )
                eventually(
                    lambda: not state()["pending_requests"],
                    "provider resolved interrupted approval",
                )
                final = state()
                assert initial["host_id"] == final["host_id"]
                assert initial["provider_pid"] == final["provider_pid"]
                print(
                    json.dumps(
                        {
                            "provider": "synthetic",
                            "create_list_read_send_stream_interrupt": "PASS",
                            "approval_accept_decline_handoff": "PASS",
                            "failed_turn_visible": "PASS",
                            "questions_choice_other_unicode_handoff_validation_duplicate_stop": "PASS",
                            "offered_approval_invalid_preserves_pending_cancel_accept_duplicate": "PASS",
                            "same_host_and_provider": True,
                        }
                    )
                )
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGINT)
                    process.wait(timeout=10)


if __name__ == "__main__":
    main()
