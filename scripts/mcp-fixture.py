#!/usr/bin/env python3
"""Minimal synthetic stdio MCP server for installed-provider discovery checks."""
import json
import sys

for line in sys.stdin:
    request = json.loads(line)
    if "id" not in request:
        continue
    method = request["method"]
    if method == "initialize":
        result = {"protocolVersion": request["params"]["protocolVersion"],
            "capabilities": {"tools": {}}, "serverInfo": {"name": "Fleetpane fixture", "version": "1"}}
    elif method == "tools/list":
        result = {"tools": [{"name": "echo", "description": "Synthetic MCP inventory 中文",
            "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}}}]}
    elif method in {"resources/list", "resources/templates/list"}:
        result = {"resources" if method == "resources/list" else "resourceTemplates": []}
    elif method == "ping":
        result = {}
    else:
        print(json.dumps({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32601,"message":"Fixture only supports discovery"}}), flush=True)
        continue
    print(json.dumps({"jsonrpc":"2.0","id":request["id"],"result":result}), flush=True)
