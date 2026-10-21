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

from resource_fixture import blocks as resource_blocks

lock = threading.Lock()
approval_operation = None
state_lock = threading.RLock()
threads = {}
running = {}
approvals = {}
questions = {}

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument(
    "--reasoning-control", type=Path, help="Gate synthetic reasoning stream stages"
)
parser.add_argument(
    "--history", type=Path, help="Load synthetic thread data for GUI checks"
)
parser.add_argument(
    "--models", type=Path, help="Reload synthetic model catalog on each model/list"
)
parser.add_argument(
    "--tree-control",
    type=Path,
    help="Reload synthetic session inventory faults on reads",
)
parser.add_argument(
    "--stop-control",
    type=Path,
    help="Control synthetic interrupt acknowledgement/completion ordering",
)
parser.add_argument(
    "--stop-effects", type=Path, help="Record synthetic interrupt submissions"
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
parser.add_argument(
    "--steer-effects", type=Path, help="Fsync synthetic turn/steer receipts"
)
parser.add_argument(
    "--hold-steer", action="store_true", help="Hold a steer response after receipt"
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
    if text == "[reasoning]":
        reasoning = {
            "id": f"reasoning-{turn_id}", "type": "reasoning",
            "summary": [], "content": [],
        }

        def publish(method, **fields):
            notify(method, {"threadId": thread_id, "turnId": turn_id,
                            "itemId": reasoning["id"], **fields})

        def gate(stage):
            if args.reasoning_control:
                while json.loads(args.reasoning_control.read_text())["stage"] < stage:
                    if stop.wait(0.025):
                        return False
            return not stop.is_set()

        with state_lock:
            turn = next(t for t in threads[thread_id]["turns"] if t["id"] == turn_id)
            turn["items"].append(reasoning)
            notify("item/started", {"threadId": thread_id, "turnId": turn_id,
                                    "item": copy.deepcopy(reasoning)})
            reasoning["summary"].append("  实时思考\n")
            publish("item/reasoning/summaryPartAdded", summaryIndex=0)
            publish("item/reasoning/summaryTextDelta", summaryIndex=0,
                    delta=reasoning["summary"][0])
        if not gate(1):
            return
        with state_lock:
            reasoning["summary"][0] += "**continued**  "
            publish("item/reasoning/summaryTextDelta", summaryIndex=0, delta="**continued**  ")
            reasoning["summary"].append("Second part 中文")
            publish("item/reasoning/summaryPartAdded", summaryIndex=1)
            publish("item/reasoning/summaryTextDelta", summaryIndex=1, delta=reasoning["summary"][1])
            reasoning["content"].append("Raw detail\n  exact spaces  ")
            publish("item/reasoning/textDelta", contentIndex=0, delta=reasoning["content"][0])
        if not gate(2):
            return
        with state_lock:
            notify("item/completed", {"threadId": thread_id, "turnId": turn_id,
                                      "item": copy.deepcopy(reasoning)})
        finish_turn(thread_id, turn_id, "completed")
        return
    if text.startswith("[resource-") and text.endswith("]"):
        case = text[len("[resource-") : -1]
        item = {
            "id": f"resource-{turn_id}",
            "type": "mcpToolCall",
            "server": "synthetic",
            "tool": f"resource_{case}",
            "status": "completed",
            "arguments": False,
            "result": {"content": resource_blocks(case)},
            "error": None,
        }
        with state_lock:
            turn = next(
                turn for turn in threads[thread_id]["turns"] if turn["id"] == turn_id
            )
            turn["items"].append(item)
            notify(
                "item/completed",
                {"threadId": thread_id, "turnId": turn_id, "item": item},
            )
        finish_turn(thread_id, turn_id, "completed")
        return
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


def controlled_interrupt(message):
    if args.stop_effects:
        with args.stop_effects.open("a") as output:
            output.write(json.dumps(message) + "\n")
            output.flush()
            os.fsync(output.fileno())
    control = json.loads(args.stop_control.read_text())
    mode = control["mode"]
    if mode == "reject":
        send(
            {
                "id": message["id"],
                "error": {"code": -32600, "message": "Synthetic stop rejection"},
            }
        )
        return
    if mode == "ack-first":
        send({"id": message["id"], "result": {}})
        notify(
            "thread/status/changed",
            {"threadId": message["params"]["threadId"], "status": {"type": "idle"}},
        )
    elif mode == "completion-first":
        finish_turn(
            message["params"]["threadId"], message["params"]["turnId"], "interrupted"
        )
    elif mode != "unanswered":
        raise ValueError(f"Unknown synthetic stop mode {mode}")
    while not json.loads(args.stop_control.read_text()).get("release", False):
        time.sleep(0.01)
    if mode != "completion-first":
        finish_turn(
            message["params"]["threadId"], message["params"]["turnId"], "interrupted"
        )
    if mode != "ack-first":
        send({"id": message["id"], "result": {}})


def session(message):
    method, params = message["method"], message["params"]
    if method == "turn/interrupt" and args.stop_control:
        threading.Thread(
            target=controlled_interrupt, args=(message,), daemon=True
        ).start()
        return
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
        elif method == "thread/loaded/list":
            control = (
                json.loads(args.tree_control.read_text()) if args.tree_control else {}
            )
            if control.get("fail"):
                send(
                    {
                        "id": message["id"],
                        "error": {
                            "code": -32600,
                            "message": "Synthetic tree inventory failure",
                        },
                    }
                )
                return
            ids = [
                thread["id"]
                for thread in threads.values()
                if thread.get("fixtureLoaded", False)
            ]
            if "loadedIds" in control:
                ids = control["loadedIds"]
            cursor = params.get("cursor")
            offset = int(cursor) if cursor else 0
            end = offset + params["limit"]
            result = {
                "data": ids[offset:end],
                "nextCursor": str(end) if end < len(ids) else None,
            }
        elif method == "thread/list":
            filtered = [
                thread
                for thread in threads.values()
                if bool(thread.get("fixtureArchived", False))
                == params.get("archived", False)
                and thread.get("fixtureListed", True)
            ]
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
            query = {key: params.get(key) for key in ("searchTerm", "cwd", "archived")}
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
        elif method == "thread/fork":
            assert params["excludeTurns"] is True and params["deferGoalContinuation"] is True
            records = [json.loads(line) for line in Path(params["path"]).read_text().splitlines()]
            assert records[0]["payload"]["id"] == params["threadId"]
            if args.creation_effects:
                with args.creation_effects.open("a") as effects:
                    effects.write(json.dumps(message) + "\n")
                    effects.flush()
                    os.fsync(effects.fileno())
            if args.hold_creation:
                return
            thread_id = f"forked-fixture-{len(threads) + 1}"
            thread = {"id":thread_id,"cwd":params["cwd"],"preview":"Imported context","name":"Forked 中文 history","historyMode":"paginated","status":{"type":"idle"},"turns":[{"id":"imported-turn","status":"completed","items":[{"id":f"saved-{i}","type":"agentMessage","text":record["payload"]["content"][0]["text"]} for i,record in enumerate(records) if record.get("type")=="response_item" and record["payload"].get("type")=="message"]}]}
            threads[thread_id] = thread
            result = {"thread":{**copy.deepcopy(thread),"turns":[]}}
        elif method == "thread/unarchive":
            thread = threads[params["threadId"]]
            if thread.get("name") == "[restore-fail]":
                send(
                    {
                        "id": message["id"],
                        "error": {
                            "code": -32600,
                            "message": "Synthetic restore refusal",
                        },
                    }
                )
                return
            thread["fixtureArchived"] = False
            result = {"thread": {**copy.deepcopy(thread), "turns": []}}
        elif method in ("thread/read", "thread/resume"):
            if method == "thread/resume":
                if threads[params["threadId"]].get("fixtureArchived", False):
                    send(
                        {
                            "id": message["id"],
                            "error": {
                                "code": -32600,
                                "message": "Restore the archived fixture before resuming",
                            },
                        }
                    )
                    return
                if threads[params["threadId"]]["status"]["type"] == "notLoaded":
                    threads[params["threadId"]]["status"] = {"type": "idle"}
            thread = copy.deepcopy(threads[params["threadId"]])
            if method == "thread/read" and args.tree_control:
                control = json.loads(args.tree_control.read_text())
                if control.get("wrongReadIdentity") == thread["id"]:
                    thread["id"] = "another-original-session"
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
            text = "".join(
                part["text"] for part in params["input"] if part["type"] == "text"
            )
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
                "content": copy.deepcopy(params["input"]),
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
        elif method == "turn/steer":
            thread_id = params["threadId"]
            turn_id = params["expectedTurnId"]
            thread = threads[thread_id]
            active = next(
                (t for t in thread["turns"] if t["status"] == "inProgress"), None
            )
            if active is None or active["id"] != turn_id:
                send(
                    {
                        "id": message["id"],
                        "error": {
                            "code": -32600,
                            "message": "expected active turn does not match",
                        },
                    }
                )
                return
            text = params["input"][0]["text"]
            if args.steer_effects:
                with args.steer_effects.open("a") as output:
                    output.write(json.dumps(params, ensure_ascii=False) + "\n")
                    output.flush()
                    os.fsync(output.fileno())
            if args.hold_steer:
                return
            if text == "[bad-steer-ack]":
                send({"id": message["id"], "result": {"turnId": "wrong-turn"}})
                return
            item = {
                "id": f"steer-{turn_id}-{len(active['items'])}",
                "type": "userMessage",
                "content": [{"type": "text", "text": text}],
            }
            active["items"].append(item)
            notify(
                "item/completed",
                {"threadId": thread_id, "turnId": turn_id, "item": item},
            )
            result = {"turnId": turn_id}
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
        "thread/loaded/list",
        "thread/name/set",
        "thread/start",
        "thread/fork",
        "thread/read",
        "thread/unarchive",
        "thread/resume",
        "thread/turns/list",
        "thread/items/list",
        "turn/start",
        "turn/steer",
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
