#!/usr/bin/env python3
"""Synthetic ACP v1 provider for Host/desktop verification; no model calls."""

import base64
import hashlib
import json
import os
import sys
import threading
import time
import uuid
from pathlib import Path
from resource_fixture import blocks as resource_blocks

lock = threading.RLock()
sessions = {}
resident = set()
running = {}
permissions = {}
next_permission = 10
cancel_gates = set()
store = Path.cwd() / "fixture-acp-sessions.json"
if store.exists():
    sessions = json.loads(store.read_text())


def send(value):
    with lock:
        print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def reply(identity, value):
    send({"id": identity, "result": value})


def update(identity, value):
    send(
        {"method": "session/update", "params": {"sessionId": identity, "update": value}}
    )


def metadata_inbox(path):
    # Private QA gate only. Provider stdout still contains ordinary ACP updates.
    with Path(path).open() as source:
        while True:
            line = source.readline()
            if not line:
                time.sleep(0.025)
                continue
            message = json.loads(line)
            with lock:
                identity = message["sessionId"]
                assert identity in resident
                patch = message["update"]
                if patch["sessionUpdate"] == "current_mode_update":
                    assert os.environ.get("FLEETPANE_FIXTURE_MODES") == "1"
                    sessions[identity]["modes"]["currentModeId"] = patch[
                        "currentModeId"
                    ]
                else:
                    assert patch["sessionUpdate"] == "session_info_update"
                    for key in ("title", "updatedAt", "_meta"):
                        if key in patch:
                            sessions[identity][key] = patch[key]
                persist()
                update(identity, patch)


def persist():
    store.write_text(json.dumps(sessions))


def options():
    return [
        {
            "id": "model",
            "name": "Model",
            "type": "select",
            "category": "model",
            "currentValue": "model-a",
            "options": [
                {
                    "group": "synthetic",
                    "name": "Synthetic",
                    "options": [
                        {"value": "model-a", "name": "Synthetic A"},
                        {"value": "model-b", "name": "Synthetic B"},
                    ],
                }
            ],
        },
        {
            "id": "reasoning_effort",
            "name": "Reasoning effort",
            "type": "select",
            "currentValue": "low",
            "options": [
                {"value": "low", "name": "Low"},
                {"value": "high", "name": "High"},
            ],
        },
    ]


def mode_change(message):
    identity = message["params"]["sessionId"]
    mode = message["params"]["modeId"]
    with lock:
        assert identity in resident
        assert mode in [m["id"] for m in sessions[identity]["modes"]["availableModes"]]
        with Path("mode-wire.jsonl").open("a") as output:
            output.write(json.dumps(message) + "\n")
        if mode == "restricted":
            send(
                {
                    "id": message["id"],
                    "error": {
                        "code": -32000,
                        "message": "Synthetic mode change denied",
                    },
                }
            )
            return
        sessions[identity]["modes"]["currentModeId"] = mode
        persist()
        update(
            identity, {"sessionUpdate": "current_mode_update", "currentModeId": mode}
        )
        Path(f"mode-pending-{identity}.json").write_text(json.dumps(message))
    while Path(f"mode-hold-{identity}").exists():
        time.sleep(0.025)
    reply(message["id"], {})


def prompt(message):
    global next_permission
    params = message["params"]
    identity = params["sessionId"]
    text = "".join(
        block["text"] for block in params["prompt"] if block["type"] == "text"
    )
    images = []
    for block in params["prompt"]:
        if block["type"] == "image":
            assert os.environ.get("FLEETPANE_FIXTURE_IMAGES") == "1"
            data = base64.b64decode(block["data"], validate=True)
            images.append(
                {
                    "sha256": hashlib.sha256(data).hexdigest(),
                    "bytes": len(data),
                    "mimeType": block["mimeType"],
                }
            )
    stop = running[identity]
    with lock:
        with (Path.cwd() / "prompt-count.txt").open("a") as output:
            output.write(text + "\n")
        sessions[identity]["prompts"].append(text)
        if images:
            sessions[identity].setdefault("imagePrompts", []).append(images)
        persist()
    if text == "[permission-cancel-gate]":
        # A provider that cannot settle until every permission receives a reply.
        # The final settlement gate lets QA distinguish requested from completed.
        cancel_gates.add(identity)
        requests = []

        def permission():
            global next_permission
            with lock:
                key = next_permission
                next_permission += 1
                pending = {
                    "ready": threading.Event(),
                    "result": None,
                    "session": identity,
                }
                permissions[key] = pending
                requests.append((key, pending))
            send(
                {
                    "id": key,
                    "method": "session/request_permission",
                    "params": {
                        "sessionId": identity,
                        "toolCall": {
                            "toolCallId": "gated",
                            "title": "Cancellation gate",
                        },
                        "options": [
                            {
                                "optionId": "allow-one",
                                "name": "Run this tool",
                                "kind": "allow_once",
                            },
                            {
                                "optionId": "reject-one",
                                "name": "Skip this tool",
                                "kind": "reject_once",
                            },
                        ],
                    },
                }
            )

        permission()
        permission()
        stop.wait()
        permission()  # Already queued/in flight when cancellation arrives.
        for _, pending in requests:
            pending["ready"].wait()
            assert pending["result"] == {"outcome": {"outcome": "cancelled"}}
        Path(f"cancel-{identity}-answered.json").write_text(
            json.dumps([key for key, _ in requests])
        )
        while not Path(f"cancel-{identity}-settle").exists():
            time.sleep(0.025)
        update(
            identity,
            {
                "sessionUpdate": "agent_message_chunk",
                "content": {
                    "type": "text",
                    "text": "Cancellation cleanup finished 中文",
                },
            },
        )
        with lock:
            for key, _ in requests:
                permissions.pop(key)
            running.pop(identity)
        reply(message["id"], {"stopReason": "cancelled"})
        return
    reasons = ("end_turn", "max_tokens", "max_turn_requests", "refusal", "cancelled")
    if text == "[permission-review]":
        path = str(Path.cwd() / "review-settings.md")
        update(
            identity,
            {
                "sessionUpdate": "tool_call",
                "toolCallId": "reviewed",
                "title": "Old operation",
                "rawInput": {"command": "old-command", "old": True},
            },
        )
        reviews = [
            {
                "toolCallId": "reviewed",
                "title": "Review settings 中文",
                "name": "write_file",
                "kind": "edit",
                "rawInput": {
                    "file_path": path,
                    "content": "# New settings 中文\nKeep exact content.",
                    "enabled": False,
                    "attempt": 0,
                },
                "locations": [{"path": path, "line": 42}],
                "content": [
                    {
                        "type": "content",
                        "content": {
                            "type": "text",
                            "text": "Proposed settings before approval 中文",
                        },
                    }
                ],
            },
            {
                "toolCallId": "standalone-review",
                "title": "Review plan 中文",
                "kind": "think",
                "rawInput": {
                    "plan": "# Proposed plan 中文\n1. Review changes.\n2. Wait for approval."
                },
                "content": [
                    {
                        "type": "content",
                        "content": {
                            "type": "text",
                            "text": "\n".join(
                                f"Review detail {n:03d} 中文" for n in range(180)
                            ),
                        },
                    }
                ],
            },
            {
                "toolCallId": "boolean-review",
                "title": "Review boolean input",
                "rawInput": False,
            },
        ]
        for index, tool in enumerate(reviews):
            with lock:
                key = next_permission
                next_permission += 1
                pending = {"ready": threading.Event(), "result": None}
                permissions[key] = pending
            request = {
                "id": key,
                "method": "session/request_permission",
                "params": {
                    "sessionId": identity,
                    "toolCall": tool,
                    "options": [
                        {
                            "optionId": "allow-one",
                            "name": "Run this tool",
                            "kind": "allow_once",
                        },
                        {
                            "optionId": "reject-one",
                            "name": "Skip this tool",
                            "kind": "reject_once",
                        },
                    ],
                },
            }
            Path(f"review-{identity}-{index}.json").write_text(json.dumps(request))
            send(request)
            pending["ready"].wait()
            with lock:
                permissions.pop(key)
                Path(f"review-{identity}-{index}-answer.json").write_text(
                    json.dumps(pending["result"])
                )
            if stop.is_set():
                break
        running.pop(identity)
        reply(
            message["id"], {"stopReason": "cancelled" if stop.is_set() else "end_turn"}
        )
        return
    if text.startswith("[resource-") and text.endswith("]"):
        case = text[len("[resource-"):-1]
        update(identity, {"sessionUpdate": "tool_call", "toolCallId": "resource",
                          "title": f"Resource {case}", "status": "pending", "rawInput": False})
        update(identity, {"sessionUpdate": "tool_call_update", "toolCallId": "resource",
                          "status": "completed", "content": [
                              {"type": "content", "content": block}
                              for block in resource_blocks(case)]})
        running.pop(identity)
        reply(message["id"], {"stopReason": "end_turn"})
        return
    if text == "[tool-progress]":
        path = str(Path.cwd() / "synthetic-settings.json")
        phases = [
            {
                "sessionUpdate": "tool_call",
                "toolCallId": "minimal",
                "title": "Read settings",
                "name": "read_file",
                "rawInput": False,
                "locations": [{"path": path, "line": 0}],
            },
            {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "minimal",
                "status": "in_progress",
                "title": "Reading settings 中文",
                "kind": "read",
                "rawInput": {"old": True},
                "rawOutput": {"old": True},
                "content": [
                    {
                        "type": "content",
                        "content": {
                            "type": "text",
                            "text": "Progress before replacement 中文",
                        },
                    }
                ],
            },
            {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "minimal",
                "status": "completed",
                "title": "",
                "name": None,
                "kind": None,
                "rawInput": {"final": False},
                "rawOutput": 0,
                "locations": [{"path": path, "line": 42}],
                "content": [
                    {
                        "type": "content",
                        "content": {"type": "text", "text": "**Final result 中文**"},
                    }
                ],
            },
            {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "minimal",
                "title": None,
                "rawInput": None,
                "rawOutput": None,
                "locations": [],
                "content": [],
            },
            {
                "sessionUpdate": "tool_call",
                "toolCallId": "failed",
                "title": "Second tool failed",
                "status": "failed",
                "rawOutput": False,
            },
        ]
        gate = Path.cwd() / "tool-progress-next.txt"
        for index, phase in enumerate(phases):
            if index:
                while not (
                    gate.exists() and int(gate.read_text()) >= index
                ) and not stop.wait(0.025):
                    pass
            if stop.is_set():
                break
            update(identity, phase)
            (Path.cwd() / "tool-progress-stage.txt").write_text(str(index))
        running.pop(identity)
        reply(
            message["id"], {"stopReason": "cancelled" if stop.is_set() else "end_turn"}
        )
        return
    if (
        text in tuple(f"[stop:{reason}]" for reason in reasons)
        or text == "[settlement-gate]"
    ):
        update(
            identity,
            {
                "sessionUpdate": "agent_message_chunk",
                "content": {
                    "type": "text",
                    "text": "Observed output 中文; waiting for protocol settlement.",
                },
            },
        )
        if text == "[settlement-gate]":
            gate = Path.cwd() / "fixture-settlement.txt"
            while not gate.exists() and not stop.wait(0.02):
                pass
            reason = "cancelled" if stop.is_set() else gate.read_text().strip()
            assert reason in reasons, reason
        else:
            reason = text[6:-1]
        running.pop(identity)
        reply(message["id"], {"stopReason": reason})
        return
    if text == "[fail]":
        send(
            {
                "id": message["id"],
                "error": {"code": -32603, "message": "Synthetic ACP prompt failure"},
            }
        )
        running.pop(identity)
        return
    if text == "[chronology]":
        events = [
            {
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "Before 中文\n"},
            },
            {
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "  keep spaces "},
            },
            {
                "sessionUpdate": "tool_call",
                "toolCallId": "chronology-tool",
                "title": "First chronological tool",
                "status": "in_progress",
            },
            {
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "After first tool"},
            },
            {
                "sessionUpdate": "agent_thought_chunk",
                "content": {"type": "text", "text": "Interleaved thought 中文"},
            },
            {
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "Tail before tool update"},
            },
            {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "chronology-tool",
                "status": "completed",
                "rawOutput": "original result 中文",
            },
            {
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "New after tool update"},
            },
            {
                "sessionUpdate": "agent_message_chunk",
                "messageId": "stream",
                "content": {"type": "text", "text": "Explicit message one"},
            },
            {
                "sessionUpdate": "tool_call",
                "toolCallId": "chronology-second",
                "title": "Second chronological tool",
                "status": "completed",
                "rawOutput": "second result",
            },
            {
                "sessionUpdate": "agent_message_chunk",
                "messageId": "stream",
                "content": {"type": "text", "text": " two"},
            },
            {
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "Anonymous final 中文"},
            },
        ]
        for event in events:
            update(identity, event)
        running.pop(identity)
        reply(message["id"], {"stopReason": "end_turn"})
        return
    if text in ("[todos]", "[todos-clear]"):
        todos = [
            {"content": "检查真实格式", "status": "in_progress"},
            {"content": "保留未来状态", "status": "pending"},
        ]
        update(
            identity,
            {
                "sessionUpdate": "tool_call",
                "toolCallId": "todo-1",
                "title": "todo_write",
                "status": "in_progress",
                "rawInput": {"todos": todos},
            },
        )
        update(
            identity,
            {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "todo-1",
                "status": "completed",
            },
        )
        todos[0]["status"] = "completed"
        todos[1]["status"] = "awaiting_review"
        update(
            identity,
            {
                "sessionUpdate": "tool_call",
                "toolCallId": "todo-2",
                "title": "todo_write",
                "status": "completed",
                "rawInput": {"todos": [] if text == "[todos-clear]" else todos},
            },
        )
        update(
            identity,
            {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "todo-1",
                "content": [],
            },
        )
        update(
            identity,
            {
                "sessionUpdate": "tool_call",
                "toolCallId": "todo-failed",
                "title": "todo_write",
                "status": "failed",
                "rawInput": {"todos": False},
            },
        )
        running.pop(identity)
        reply(message["id"], {"stopReason": "end_turn"})
        return
    if text in ("[tasks]", "[tasks-wait]", "[tasks-clear]", "[tasks-future]"):
        entries = [
            {"content": "检查现有实现", "status": "completed", "priority": "high"},
            {
                "content": "实现任务面板\n保留中文换行",
                "status": "in_progress",
                "priority": "high",
            },
            {"content": "验证恢复与复制", "status": "pending", "priority": "medium"},
        ]
        if text == "[tasks-future]":
            entries[1]["status"] = "awaiting_review"

        def plan(rows):
            update(identity, {"sessionUpdate": "plan", "entries": rows})

        plan(entries)
        if text == "[tasks-wait]":
            stop.wait()
            running.pop(identity)
            reply(message["id"], {"stopReason": "cancelled"})
            return
        if text == "[tasks]":
            plan(
                [
                    {
                        "content": "重规划：执行验证",
                        "status": "in_progress",
                        "priority": "low",
                    }
                ]
            )
            for index in range(140):
                update(
                    identity,
                    {
                        "sessionUpdate": "config_option_update",
                        "configOptions": options(),
                    },
                )
        elif text == "[tasks-clear]":
            plan([])
    if text == "[burst]":
        for index in range(50):
            if stop.wait(0.1):
                break
            update(
                identity,
                {
                    "sessionUpdate": "agent_message_chunk",
                    "messageId": "burst",
                    "content": {"type": "text", "text": f"chunk-{index:02} 你好\n"},
                },
            )
        running.pop(identity)
        reply(
            message["id"], {"stopReason": "cancelled" if stop.is_set() else "end_turn"}
        )
        return
    if text == "[large-history]":
        for index in range(24):
            update(
                identity,
                {
                    "sessionUpdate": "tool_call",
                    "toolCallId": f"history-{index:02}",
                    "title": f"History tool {index:02}",
                    "status": "completed",
                    "rawInput": {"index": index},
                    "content": [
                        {
                            "type": "content",
                            "content": {
                                "type": "text",
                                "text": f"BEGIN {index:02}\n"
                                + "x" * 60000
                                + f"\nEND {index:02}",
                            },
                        }
                    ],
                },
            )
        update(
            identity,
            {
                "sessionUpdate": "agent_message_chunk",
                "messageId": "history-answer",
                "content": {"type": "text", "text": "**History** ready: 24 tools"},
            },
        )
        running.pop(identity)
        reply(message["id"], {"stopReason": "end_turn"})
        return
    update(
        identity,
        {
            "sessionUpdate": "agent_thought_chunk",
            "messageId": "thought",
            "content": {"type": "text", "text": "Synthetic ACP reasoning"},
        },
    )
    call = f"call-{uuid.uuid4()}"
    update(
        identity,
        {
            "sessionUpdate": "tool_call",
            "toolCallId": call,
            "title": "Synthetic tool",
            "kind": "other",
            "status": "in_progress",
            "rawInput": {"text": text},
        },
    )
    if text == "[permission]":
        with lock:
            key = next_permission
            next_permission += 1
            ready = threading.Event()
            pending = {"ready": ready, "result": None}
            permissions[key] = pending
        send(
            {
                "id": key,
                "method": "session/request_permission",
                "params": {
                    "sessionId": identity,
                    "toolCall": {"toolCallId": call, "title": "Synthetic tool"},
                    "options": [
                        {
                            "optionId": "allow-one",
                            "name": "Run this tool",
                            "kind": "allow_once",
                        },
                        {
                            "optionId": "reject-one",
                            "name": "Skip this tool",
                            "kind": "reject_once",
                        },
                    ],
                },
            }
        )
        ready.wait()
        with lock:
            permissions.pop(key)
        if not stop.is_set():
            sessions[identity]["permission"] = pending["result"]
            persist()
    if text == "[slow]":
        for _ in range(200):
            if stop.wait(0.05):
                break
    else:
        stop.wait(0.2)
    update(
        identity,
        {
            "sessionUpdate": "tool_call_update",
            "toolCallId": call,
            "status": "failed" if stop.is_set() else "completed",
            "content": [
                {
                    "type": "content",
                    "content": {"type": "text", "text": "Synthetic ACP tool output"},
                }
            ],
        },
    )
    if not stop.is_set():
        for chunk in ["**ACP** Echo: ", text]:
            update(
                identity,
                {
                    "sessionUpdate": "agent_message_chunk",
                    "messageId": "answer",
                    "content": {"type": "text", "text": chunk},
                },
            )
            time.sleep(0.05)
    running.pop(identity)
    reply(message["id"], {"stopReason": "cancelled" if stop.is_set() else "end_turn"})


if inbox := os.environ.get("FLEETPANE_FIXTURE_METADATA_INBOX"):
    threading.Thread(target=metadata_inbox, args=(inbox,), daemon=True).start()

for line in sys.stdin:
    message = json.loads(line)
    assert message["jsonrpc"] == "2.0"
    method = message.get("method")
    params = message.get("params", {})
    if method == "initialize":
        assert params["protocolVersion"] == 1 and params["clientCapabilities"] == {}
        reply(
            message["id"],
            {
                "protocolVersion": 1,
                "agentInfo": {"name": "synthetic-acp", "version": "1"},
                "agentCapabilities": {
                    "sessionCapabilities": {"list": {}, "resume": {}, "close": {}},
                    **(
                        {"promptCapabilities": {"image": True}}
                        if os.environ.get("FLEETPANE_FIXTURE_IMAGES") == "1"
                        else {}
                    ),
                },
                "authMethods": [],
            },
        )
    elif method == "session/new":
        identity = str(uuid.uuid4())
        assert Path(params["cwd"]).is_absolute() and params["mcpServers"] == []
        sessions[identity] = {
            "sessionId": identity,
            "cwd": params["cwd"],
            "title": "Synthetic ACP",
            "configOptions": options(),
            "prompts": [],
        }
        if os.environ.get("FLEETPANE_FIXTURE_MODES") == "1":
            sessions[identity]["modes"] = {
                "currentModeId": "ask",
                "availableModes": [
                    {"id": "ask", "name": "Ask"},
                    {"id": "code", "name": "Code", "description": "Change files 中文"},
                    {
                        "id": "restricted",
                        "name": "Restricted",
                        "description": "Synthetic rejection",
                    },
                ],
            }
        persist()
        resident.add(identity)
        reply(
            message["id"],
            {
                "sessionId": identity,
                "configOptions": sessions[identity]["configOptions"],
                **(
                    {"modes": sessions[identity]["modes"]}
                    if "modes" in sessions[identity]
                    else {}
                ),
            },
        )
    elif method == "session/list":
        rows = [
            {
                key: s[key]
                for key in ("sessionId", "cwd", "title", "updatedAt", "_meta")
                if key in s
            }
            for s in sessions.values()
            if s["sessionId"] not in resident and params.get("cwd") in (None, s["cwd"])
        ]
        reply(message["id"], {"sessions": rows})
    elif method == "session/resume":
        session = sessions[params["sessionId"]]
        assert params["cwd"] == session["cwd"] and params["mcpServers"] == []
        resident.add(params["sessionId"])
        if fresh := os.environ.get("FLEETPANE_FIXTURE_RESUME_MODE"):
            session["modes"] = {
                "currentModeId": fresh,
                "availableModes": [{"id": fresh, "name": "Fresh mode"}],
            }
            persist()
        reply(
            message["id"],
            {
                "configOptions": session["configOptions"],
                **({"modes": session["modes"]} if "modes" in session else {}),
            },
        )
    elif method == "session/set_mode":
        assert os.environ.get("FLEETPANE_FIXTURE_MODES") == "1"
        threading.Thread(target=mode_change, args=(message,), daemon=True).start()
    elif method == "session/set_config_option":
        session = sessions[params["sessionId"]]
        option = next(
            o for o in session["configOptions"] if o["id"] == params["configId"]
        )
        option["currentValue"] = params["value"]
        persist()
        update(
            params["sessionId"],
            {
                "sessionUpdate": "config_option_update",
                "configOptions": session["configOptions"],
            },
        )
        reply(message["id"], {"configOptions": session["configOptions"]})
    elif method == "session/prompt":
        assert params["sessionId"] not in running
        running[params["sessionId"]] = threading.Event()
        threading.Thread(target=prompt, args=(message,), daemon=True).start()
    elif method == "session/cancel":
        if params["sessionId"] in cancel_gates:
            with Path("cancel-wire.jsonl").open("a") as output:
                output.write(json.dumps(message) + "\n")
        if params["sessionId"] in running:
            running[params["sessionId"]].set()
    elif method is None and message.get("id") in permissions:
        pending = permissions[message["id"]]
        assert pending["result"] is None, "permission answered twice"
        pending["result"] = message["result"]
        if pending.get("session") in cancel_gates:
            with Path("cancel-wire.jsonl").open("a") as output:
                output.write(json.dumps(message) + "\n")
        pending["ready"].set()
    else:
        send(
            {
                "id": message["id"],
                "error": {
                    "code": -32601,
                    "message": "Unsupported synthetic ACP method",
                },
            }
        )
