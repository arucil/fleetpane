#!/usr/bin/env python3
"""Synthetic ACP v1 provider for Host/desktop verification; no model calls."""

import json
import sys
import threading
import time
import uuid
from pathlib import Path

lock = threading.RLock()
sessions = {}
resident = set()
running = {}
permissions = {}
next_permission = 10
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


def prompt(message):
    global next_permission
    params = message["params"]
    identity = params["sessionId"]
    text = params["prompt"][0]["text"]
    stop = running[identity]
    with lock:
        with (Path.cwd() / "prompt-count.txt").open("a") as output:
            output.write(text + "\n")
        sessions[identity]["prompts"].append(text)
        persist()
    if text == "[fail]":
        send(
            {
                "id": message["id"],
                "error": {"code": -32603, "message": "Synthetic ACP prompt failure"},
            }
        )
        running.pop(identity)
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
        while not ready.wait(0.05) and not stop.is_set():
            pass
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
                    "sessionCapabilities": {"list": {}, "resume": {}, "close": {}}
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
        persist()
        resident.add(identity)
        reply(
            message["id"],
            {
                "sessionId": identity,
                "configOptions": sessions[identity]["configOptions"],
            },
        )
    elif method == "session/list":
        rows = [
            {key: s[key] for key in ("sessionId", "cwd", "title")}
            for s in sessions.values()
            if s["sessionId"] not in resident and params.get("cwd") in (None, s["cwd"])
        ]
        reply(message["id"], {"sessions": rows})
    elif method == "session/resume":
        session = sessions[params["sessionId"]]
        assert params["cwd"] == session["cwd"] and params["mcpServers"] == []
        resident.add(params["sessionId"])
        reply(message["id"], {"configOptions": session["configOptions"]})
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
        if params["sessionId"] in running:
            running[params["sessionId"]].set()
    elif method is None and message.get("id") in permissions:
        pending = permissions[message["id"]]
        assert pending["result"] is None, "permission answered twice"
        pending["result"] = message["result"]
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
