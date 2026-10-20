#!/usr/bin/env python3
"""Synthetic mutation receiver with fsynced side effects and held responses."""

import json
import os
import sys
from pathlib import Path


def send(message):
    print(json.dumps(message), flush=True)


for line in sys.stdin:
    message = json.loads(line)
    method = message.get("method")
    if method == "initialize":
        send({"id": message["id"], "result": {"userAgent": "journal-fixture/1"}})
    elif method == "initialized":
        pass
    elif method == "fixture/mutate":
        with Path("effects.jsonl").open("a") as effects:
            effects.write(json.dumps(message) + "\n")
            effects.flush()
            os.fsync(effects.fileno())
        if not message["params"].get("hold"):
            send({"id": message["id"], "result": message["params"]})
    elif method == "thread/list":
        send({"id": message["id"], "result": {"data": [], "nextCursor": None}})
    else:
        raise AssertionError(f"unexpected fixture method: {method}")
