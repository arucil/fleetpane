#!/usr/bin/env python3
"""Synthetic JSONL provider. Test data only; not a provider implementation."""

import argparse
import copy
import json
import os
import subprocess
import sys
import threading
import time
from pathlib import Path

lock = threading.Lock()
approval_operation = None
state_lock = threading.RLock()
threads = {}
running = {}
approvals = {}
questions = {}

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument(
    "--history", type=Path, help="Load synthetic thread data for GUI checks"
)
parser.add_argument(
    "--models", type=Path, help="Reload synthetic model catalog on each model/list"
)
parser.add_argument(
    "--rename-admission-marker",
    type=Path,
    help="For [hold-rename], record receipt then hold the response until process exit",
)
parser.add_argument(
    "--history-page-delay",
    type=float,
    default=0,
    help="Synthetic history I/O latency in seconds; streams continue during it",
)
parser.add_argument(
    "--creation-effects", type=Path, help="Fsync synthetic creation side effects"
)
parser.add_argument(
    "--hold-creation",
    action="store_true",
    help="Hold creation after recording its synthetic side effect",
)
parser.add_argument(
    "--send-effects", type=Path, help="Fsync synthetic turn/start side effects"
)
parser.add_argument(
    "--hold-send",
    action="store_true",
    help="Hold turn/start after recording its synthetic side effect",
)
args = parser.parse_args()
if args.history:
    threads = {thread["id"]: thread for thread in json.loads(args.history.read_text())}


def model_catalog():
    if args.models:
        return json.loads(args.models.read_text())
    return [
        {
            "id": name,
            "model": name,
            "displayName": label,
            "description": "Synthetic provider capability; no external model call",
            "hidden": hidden,
            "isDefault": index == 0,
            "defaultReasoningEffort": efforts[0] if efforts else "none",
            "supportedReasoningEfforts": [
                {"reasoningEffort": effort, "description": f"Synthetic {effort}"}
                for effort in efforts
            ],
        }
        for index, (name, label, efforts, hidden) in enumerate(
            [
                ("fixture-balanced", "Balanced 测试模型", ["medium", "high"], False),
                ("fixture-quick", "Quick 测试模型", ["low"], False),
                ("fixture-basic", "Basic · no reasoning control", [], False),
                ("fixture-hidden", "Hidden test model", ["high"], True),
            ]
        )
    ]


def notify(method, params):
    send({"method": method, "params": params})


def finish_turn(thread_id, turn_id, status):
    with state_lock:
        thread = threads[thread_id]
        turn = next(turn for turn in thread["turns"] if turn["id"] == turn_id)
        if turn["status"] != "inProgress":
            return
        turn["status"] = status
        thread["status"] = {"type": "idle"}
        # Codex completion notifications need not repeat previously emitted items.
        notify("turn/completed", {"threadId": thread_id, "turn": {**turn, "items": []}})
        notify(
            "thread/status/changed", {"threadId": thread_id, "status": thread["status"]}
        )
        running[turn_id].set()
        request_id = f"approval-{turn_id}"
        if approvals.pop(request_id, None) is not None:
            notify(
                "serverRequest/resolved",
                {"threadId": thread_id, "requestId": request_id},
            )
        request_id = f"questions-{turn_id}"
        if questions.pop(request_id, None) is not None:
            notify(
                "serverRequest/resolved",
                {"threadId": thread_id, "requestId": request_id},
            )


def stream(thread_id, turn_id, text):
    stop = running[turn_id]
    if text in ("[tasks]", "[tasks-wait]", "[tasks-clear]", "[tasks-future]"):
        entries = [
            {"step": "检查现有实现", "status": "completed", "priority": "high"},
            {
                "step": "实现任务面板\n保留中文换行",
                "status": "inProgress",
                "priority": "high",
            },
            {"step": "验证恢复与复制", "status": "pending", "priority": "medium"},
        ]
        if text == "[tasks-future]":
            entries[1]["status"] = "awaiting_review"

        def plan(rows):
            notify(
                "turn/plan/updated",
                {
                    "threadId": thread_id,
                    "turnId": turn_id,
                    "explanation": "按 agent 报告的状态展示",
                    "plan": rows,
                },
            )

        plan(entries)
        if text == "[tasks-wait]":
            stop.wait()
            return
        if text == "[tasks]":
            plan([{"step": "重规划：执行验证", "status": "inProgress"}])
            # Force the original update outside the bounded event ring.
            for index in range(140):
                notify("fixture/unrelated", {"index": index})
        elif text == "[tasks-clear]":
            plan([])
    if text == "[wait]":
        stop.wait()
        return
    if text in ("[questions]", "[secret-question]"):
        request_id = f"questions-{turn_id}"
        ready = threading.Event()
        with state_lock:
            questions[request_id] = {"ready": ready, "response": None}
        prompts = [
            {
                "id": "language",
                "header": "Language",
                "question": "选择实现语言？",
                "isOther": True,
                "isSecret": False,
                "options": [
                    {
                        "label": "Rust (Recommended)",
                        "description": "Native performance and explicit ownership.",
                    },
                    {
                        "label": "Go",
                        "description": "Simple concurrency and quick builds.",
                    },
                ],
            },
            {
                "id": "notes",
                "header": "Details",
                "question": "补充要求（支持中文和换行）",
                "isOther": False,
                "isSecret": False,
                "options": None,
            },
        ]
        if text == "[secret-question]":
            prompts = [
                {
                    "id": "secret",
                    "header": "Secret",
                    "question": "Enter a synthetic secret",
                    "isOther": False,
                    "isSecret": True,
                    "options": None,
                }
            ]
        send(
            {
                "id": request_id,
                "method": "item/tool/requestUserInput",
                "params": {
                    "threadId": thread_id,
                    "turnId": turn_id,
                    "itemId": f"input-{turn_id}",
                    "questions": prompts,
                    "isBlocking": True,
                },
            }
        )
        while not ready.wait(0.02):
            if stop.is_set():
                return
        with state_lock:
            if stop.is_set():
                return
            response = questions.pop(request_id)["response"]
            text = (
                "Secret received (redacted)"
                if text == "[secret-question]"
                else "Answers received: " + json.dumps(response, ensure_ascii=False)
            )
    if text == "[approval]":
        request_id = f"approval-{turn_id}"
        ready = threading.Event()
        with state_lock:
            approvals[request_id] = {"ready": ready, "decision": None}
        send(
            {
                "id": request_id,
                "method": "item/commandExecution/requestApproval",
                "params": {
                    "threadId": thread_id,
                    "turnId": turn_id,
                    "itemId": f"command-{turn_id}",
                    "command": "synthetic approval; no command runs",
                },
            }
        )
        while not ready.wait(0.02):
            if stop.is_set():
                return
        with state_lock:
            if stop.is_set():
                return
            text = "Approval: " + approvals.pop(request_id)["decision"]
    with state_lock:
        if stop.is_set():
            return
        turn = next(
            turn for turn in threads[thread_id]["turns"] if turn["id"] == turn_id
        )
        if text == "[options]":
            text = "Turn options: " + json.dumps(
                turn["fixtureOptions"], ensure_ascii=False
            )
        item = {"id": f"assistant-{turn_id}", "type": "agentMessage", "text": ""}
        turn["items"].append(item)
        notify(
            "item/started",
            {"threadId": thread_id, "turnId": turn_id, "item": copy.deepcopy(item)},
        )
    for chunk in ["Echo: ", text[: len(text) // 2], text[len(text) // 2 :]]:
        if stop.wait(0.2):
            return
        with state_lock:
            item["text"] += chunk
            notify(
                "item/agentMessage/delta",
                {
                    "threadId": thread_id,
                    "turnId": turn_id,
                    "itemId": item["id"],
                    "delta": chunk,
                },
            )
    with state_lock:
        notify(
            "item/completed", {"threadId": thread_id, "turnId": turn_id, "item": item}
        )
        if text == "[fail]":
            turn["error"] = {"message": "Synthetic turn failure"}
    finish_turn(thread_id, turn_id, "failed" if text == "[fail]" else "completed")


def session(message):
    method, params = message["method"], message["params"]
    if method in ("thread/turns/list", "thread/items/list") and args.history_page_delay:
        time.sleep(args.history_page_delay)
    with state_lock:
        if method == "model/list":
            models = model_catalog()
            if isinstance(models, dict):
                send(
                    {
                        "id": message["id"],
                        "error": {"code": -32600, "message": models["error"]},
                    }
                )
                return
            models = [
                model
                for model in models
                if params["includeHidden"] or not model["hidden"]
            ]
            offset = int(params["cursor"]) if params.get("cursor") else 0
            end = offset + params["limit"]
            result = {
                "data": models[offset:end],
                "nextCursor": str(end) if end < len(models) else None,
            }
        elif method == "thread/list":
            filtered = list(threads.values())
            if params.get("searchTerm"):
                term = params["searchTerm"].casefold()
                filtered = [
                    thread
                    for thread in filtered
                    if term in (thread["name"] or thread["preview"]).casefold()
                ]
            if params.get("cwd"):
                filtered = [
                    thread for thread in filtered if thread["cwd"] == params["cwd"]
                ]
            query = {key: params.get(key) for key in ("searchTerm", "cwd")}
            cursor = params.get("cursor")
            offset = 0
            if cursor:
                decoded = json.loads(cursor)
                assert decoded["query"] == query, "cursor query changed"
                offset = decoded["offset"]
            end = offset + params["limit"]
            # Fault cases are explicit synthetic fixtures, used to verify retained results.
            if params.get("searchTerm") == "[list-fail]":
                send(
                    {
                        "id": message["id"],
                        "error": {
                            "code": -32600,
                            "message": "Synthetic directory failure",
                        },
                    }
                )
                return
            result = {
                "data": [{**thread, "turns": []} for thread in filtered[offset:end]],
                "nextCursor": json.dumps({"offset": end, "query": query})
                if end < len(filtered)
                else None,
            }
        elif method == "thread/name/set":
            if args.rename_admission_marker and params["name"] == "[hold-rename]":
                with args.rename_admission_marker.open("a") as marker:
                    marker.write(json.dumps(message) + "\n")
                # Intentional crash fixture: accepted by the provider, with no
                # response or fake settlement. The test terminates this process.
                threading.Event().wait()
            if params["name"] == "[rename-fail]":
                send(
                    {
                        "id": message["id"],
                        "error": {
                            "code": -32600,
                            "message": "Synthetic rename failure",
                        },
                    }
                )
                return
            thread = threads[params["threadId"]]
            thread["name"] = params["name"].strip()
            result = {}
            send({"id": message["id"], "result": result})
            notify(
                "thread/name/updated",
                {"threadId": thread["id"], "threadName": thread["name"]},
            )
            return
        elif method == "thread/start":
            if args.creation_effects:
                with args.creation_effects.open("a") as effects:
                    effects.write(json.dumps(message) + "\n")
                    effects.flush()
                    os.fsync(effects.fileno())
            if args.hold_creation:
                return
            assert params["historyMode"] == "paginated", (
                "shared adapter must select its supported history contract"
            )
            thread_id = f"fixture-thread-{len(threads) + 1}"
            thread = {
                "id": thread_id,
                "cwd": params["cwd"],
                "preview": "New conversation",
                "name": None,
                "historyMode": "paginated",
                "status": {"type": "idle"},
                "turns": [],
            }
            threads[thread_id] = thread
            result = {"thread": thread}
        elif method in ("thread/read", "thread/resume"):
            thread = copy.deepcopy(threads[params["threadId"]])
            if params.get("excludeTurns") or params.get("includeTurns") is False:
                thread["turns"] = []
            result = {"thread": thread}
        elif method in ("thread/turns/list", "thread/items/list"):
            thread = threads[params["threadId"]]
            cursor = params.get("cursor")
            offset = 0 if cursor is None else json.loads(cursor)["offset"]
            if method == "thread/turns/list":
                assert params["limit"] == 1 and params["itemsView"] == "notLoaded"
                rows = [{**turn, "items": []} for turn in reversed(thread["turns"])]
                scope = {"method": method, "thread": thread["id"]}
            else:
                assert params["limit"] == 16
                turn = next(
                    turn for turn in thread["turns"] if turn["id"] == params["turnId"]
                )
                rows = [
                    {"turnId": turn["id"], "item": item}
                    for item in reversed(turn["items"])
                ]
                scope = {"method": method, "thread": thread["id"], "turn": turn["id"]}
            assert params["sortDirection"] == "desc"
            if cursor is not None:
                assert json.loads(cursor)["scope"] == scope
            end = offset + params["limit"]
            result = {
                "data": rows[offset:end],
                "nextCursor": json.dumps({"scope": scope, "offset": end})
                if end < len(rows)
                else None,
                "backwardsCursor": None,
            }
        elif method == "turn/start":
            thread_id = params["threadId"]
            thread = threads[thread_id]
            if thread["status"]["type"] != "idle":
                send(
                    {
                        "id": message["id"],
                        "error": {"code": -32600, "message": "turn already active"},
                    }
                )
                return
            text = params["input"][0]["text"]
            if args.send_effects:
                with args.send_effects.open("a") as effects:
                    effects.write(json.dumps(message) + "\n")
                    effects.flush()
                    os.fsync(effects.fileno())
            if args.hold_send:
                # Leave this RPC unanswered while independent reads keep working.
                return
            if text == "[options-fail]":
                send(
                    {
                        "id": message["id"],
                        "error": {
                            "code": -32600,
                            "message": "Synthetic turn options rejection",
                        },
                    }
                )
                return
            turn_id = f"{thread_id}-turn-{len(thread['turns']) + 1}"
            turn = {
                "id": turn_id,
                "status": "inProgress",
                "items": [],
                "error": None,
                "fixtureOptions": {
                    key: params[key] for key in ("model", "effort") if key in params
                },
            }
            thread["turns"].append(turn)
            thread["preview"] = text
            thread["status"] = {"type": "active", "activeFlags": []}
            running[turn_id] = threading.Event()
            notify(
                "thread/status/changed",
                {"threadId": thread_id, "status": thread["status"]},
            )
            notify("turn/started", {"threadId": thread_id, "turn": copy.deepcopy(turn)})
            item = {
                "id": f"user-{turn_id}",
                "type": "userMessage",
                "content": [{"type": "text", "text": text}],
            }
            turn["items"].append(item)
            notify(
                "item/completed",
                {"threadId": thread_id, "turnId": turn_id, "item": item},
            )
            send({"id": message["id"], "result": {"turn": {**turn, "items": []}}})
            threading.Thread(
                target=stream, args=(thread_id, turn_id, text), daemon=True
            ).start()
            return
        elif method == "turn/interrupt":
            finish_turn(params["threadId"], params["turnId"], "interrupted")
            result = {}
        else:
            raise ValueError(f"unsupported fixture session method {method}")
        send({"id": message["id"], "result": result})


def send(value):
    with lock:
        print(json.dumps(value), flush=True)


def execute(message):
    completed = subprocess.run(
        message["params"]["command"], capture_output=True, text=True, check=False
    )
    send(
        {
            "id": message["id"],
            "result": {
                "exitCode": completed.returncode,
                "stdout": completed.stdout,
                "stderr": completed.stderr,
            },
        }
    )


for line in sys.stdin:
    message = json.loads(line)
    method = message.get("method")
    if method == "initialize":
        send({"id": message["id"], "result": {"userAgent": "synthetic-provider/1"}})
    elif method == "initialized":
        pass
    elif method == "command/exec":
        threading.Thread(target=execute, args=(message,), daemon=True).start()
    elif method in (
        "model/list",
        "thread/list",
        "thread/name/set",
        "thread/start",
        "thread/read",
        "thread/resume",
        "thread/turns/list",
        "thread/items/list",
        "turn/start",
        "turn/interrupt",
    ):
        session(message)
    elif method is None and message.get("id") in approvals:
        approval = approvals[message["id"]]
        approval["decision"] = message["result"]["decision"]
        approval["ready"].set()
    elif method is None and message.get("id") in questions:
        question = questions[message["id"]]
        question["response"] = message["result"]
        question["ready"].set()
    elif method == "fixture/approval":
        approval_operation = message["id"]
        send(
            {
                "id": 999,
                "method": "item/commandExecution/requestApproval",
                "params": {"command": "synthetic"},
            }
        )
    elif method is None and message.get("id") == 999:
        send({"id": approval_operation, "result": message["result"]})
        approval_operation = None
    else:
        send(
            {
                "id": message["id"],
                "error": {"code": -32601, "message": "fixture method not implemented"},
            }
        )
