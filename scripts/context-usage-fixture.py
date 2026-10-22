#!/usr/bin/env python3
"""Synthetic provider context telemetry; no SDK, authentication or model calls.

--inbox is append-only JSONL: {"emit": <complete provider frame>} or
{"settle": "thread-id"}. Context updates can arrive while idle or between turns.
"""
import argparse
import json
import threading
import time
from pathlib import Path
import sys

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--provider", choices=("codex", "acp"), required=True)
parser.add_argument("--inbox", type=Path, required=True)
parser.add_argument("--wire-log", type=Path, required=True)
args = parser.parse_args()
lock = threading.RLock()
store = args.inbox.with_suffix(".sessions.json")
sessions = json.loads(store.read_text()) if store.exists() else {}
pending = {}


def send(value):
    if args.provider == "acp":
        value = {"jsonrpc": "2.0", **value}
    with lock:
        print(json.dumps(value, ensure_ascii=False), flush=True)


def persist():
    store.write_text(json.dumps(sessions, ensure_ascii=False))


def reply(identity, result):
    send({"id": identity, "result": result})


def notify(method, params):
    send({"method": method, "params": params})


def settle(identity):
    request, turn = pending.pop(identity)
    if args.provider == "acp":
        # This is deliberately a huge *session total*, never current context.
        reply(request, {"stopReason": "end_turn", "usage": {
            "totalTokens": 999999, "inputTokens": 999990, "outputTokens": 9}})
    else:
        turn["status"] = "completed"
        sessions[identity]["status"] = {"type": "idle"}
        notify("turn/completed", {"threadId": identity, "turn": turn})
    persist()


def inbox():
    with args.inbox.open() as source:
        while True:
            line = source.readline()
            if not line:
                time.sleep(0.02)
                continue
            command = json.loads(line)
            with lock:
                if "settle" in command:
                    settle(command["settle"])
                else:
                    send(command["emit"])


threading.Thread(target=inbox, daemon=True).start()
for line in sys.stdin:
    frame = json.loads(line)
    with lock:
        with args.wire_log.open("a") as output:
            output.write(json.dumps(frame, ensure_ascii=False) + "\n")
        method = frame.get("method")
        p = frame.get("params", {})
        request = frame.get("id")
        if method is None:
            # Explicit synthetic pending-choice answers are recorded, not executed.
            continue
        if method == "initialize":
            reply(request, {"protocolVersion": 1, "agentInfo": {"name": "context-fixture", "version": "1"},
                            "agentCapabilities": {"sessionCapabilities": {"list": {}, "resume": {}}}})
        elif method == "initialized":
            pass
        elif method in ("thread/start", "session/new"):
            identity = f"usage-{len(sessions) + 1}"
            thread = {"id": identity, "cwd": p["cwd"], "name": "Context fixture " + identity,
                      "preview": "Synthetic context observation", "status": {"type": "idle"}, "turns": []}
            sessions[identity] = thread
            persist()
            reply(request, {"sessionId": identity, "configOptions": []} if args.provider == "acp" else {"thread": thread})
        elif method in ("thread/read", "thread/resume"):
            thread = dict(sessions[p["threadId"]])
            if not p.get("includeTurns", False):
                thread["turns"] = []
            reply(request, {"thread": thread})
        elif method == "session/resume":
            assert p["sessionId"] in sessions
            reply(request, {"configOptions": []})
        elif method == "thread/list":
            reply(request, {"data": [{**thread, "turns": []} for thread in sessions.values()],
                            "nextCursor": None})
        elif method == "thread/loaded/list":
            reply(request, {"data": list(sessions), "nextCursor": None})
        elif method == "session/list":
            reply(request, {"sessions": [{"sessionId": key, "cwd": t["cwd"], "title": t["name"]}
                                          for key, t in sessions.items()]})
        elif method in ("model/list", "mcpServerStatus/list"):
            reply(request, {"data": [], "nextCursor": None})
        elif method in ("turn/start", "session/prompt"):
            identity = p["sessionId"] if args.provider == "acp" else p["threadId"]
            turn = {"id": f"turn-{identity}-{len(sessions[identity]['turns']) + 1}", "status": "inProgress", "items": []}
            sessions[identity]["turns"].append(turn)
            sessions[identity]["status"] = {"type": "active"}
            pending[identity] = (request, turn)
            persist()
            if args.provider == "codex":
                notify("turn/started", {"threadId": identity, "turn": turn})
                reply(request, {"turn": turn})
        else:
            send({"id": request, "error": {"code": -32601, "message": f"context fixture does not implement {method}"}})
