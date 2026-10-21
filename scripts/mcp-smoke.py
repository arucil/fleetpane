#!/usr/bin/env python3
"""Explicit journaled MCP discovery, paging, fencing, drain and malformed replies."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import uuid

REPO = Path(__file__).resolve().parents[1]
BINARY = REPO / "target/debug/fleetpane-host"


def main():
    with tempfile.TemporaryDirectory(prefix="fleetpane-mcp-") as directory:
        root = Path(directory)
        inventory = root / "mcp.json"
        servers = [{"name": f"server-{index:03}", "authStatus": "future-auth",
                    "runtimeStatus": None, "toolsError": "tool lookup failed 中文" if index == 0 else None,
                    "tools": {"qualified_tool": {"name": "bare", "inputSchema": {"type": "object"}}},
                    "pluginId": "synthetic", "resources": [], "resourceTemplates": []} for index in range(37)]
        inventory.write_text(json.dumps(servers))
        socket = root / "private/control.sock"
        with (root / "host.log").open("w+") as log:
            process = subprocess.Popen([str(BINARY), "serve", str(socket), str(root),
                sys.executable, str(REPO / "scripts/provider-fixture.py"), "--mcp", str(inventory)],
                stdout=subprocess.DEVNULL, stderr=log, start_new_session=True)
            try:
                deadline = time.monotonic() + 10
                while not socket.exists() or "ready:" not in (root / "host.log").read_text():
                    assert process.poll() is None and time.monotonic() < deadline
                    time.sleep(0.02)

                def raw(payload):
                    result = subprocess.run([str(BINARY), "request", str(socket)], input=json.dumps(payload),
                        capture_output=True, text=True, timeout=10, check=False)
                    assert result.stdout, result.stderr
                    return json.loads(result.stdout)

                def run(*args):
                    return subprocess.run([str(BINARY), "session", str(socket), *args],
                        capture_output=True, text=True, timeout=25, check=False)

                def snapshot():
                    return raw({"command": "snapshot"})["data"]

                initial = snapshot()
                first = run("--take-control", "mcp")
                assert first.returncode == 0, first.stderr
                first = json.loads(first.stdout)
                assert len(first["result"]["data"]) == 32 and first["result"]["nextCursor"] == "32"
                owner = snapshot()["owner"]
                assert snapshot()["journal_total"] == initial["journal_total"] + 1
                refused = run("mcp")
                assert refused.returncode != 0 and "--take-control" in refused.stderr
                second = run("--take-control", "mcp", "--cursor", "32")
                assert second.returncode == 0, second.stderr
                assert len(json.loads(second.stdout)["result"]["data"]) == 5
                state = snapshot()
                assert state["journal_total"] == initial["journal_total"] + 2
                stale = raw({"command":"execute", "lease":owner, "operation_id":str(uuid.uuid4()),
                    "method":"mcpServerStatus/list", "params":{"limit":32,"detail":"toolsAndAuthOnly"}})
                assert stale["response"] == "error", stale
                assert stale["data"]["code"] == "stale_controller", stale
                assert snapshot()["journal_total"] == state["journal_total"]

                drained = run("--take-control", "drain")
                assert drained.returncode == 0, drained.stderr
                total = snapshot()["journal_total"]
                denied = run("--take-control", "mcp")
                assert denied.returncode != 0 and "drain" in denied.stderr.lower(), denied.stderr
                assert snapshot()["journal_total"] == total
                assert run("--take-control", "ready").returncode == 0
                inventory.write_text(json.dumps({"error":"discovery unavailable"}))
                failed = run("--take-control", "mcp")
                assert failed.returncode != 0 and "discovery unavailable" in failed.stderr
                inventory.write_text(json.dumps([{"name":"malformed","tools":{}}]))
                malformed = run("--take-control", "mcp")
                assert malformed.returncode != 0 and "MCP catalog" in malformed.stderr, malformed.stderr
                inventory.write_text("[]")
                empty = run("--take-control", "mcp")
                assert empty.returncode == 0 and json.loads(empty.stdout)["result"]["data"] == []
                final = snapshot()
                assert final["host_id"] == initial["host_id"] and final["provider_pid"] == initial["provider_pid"]
                assert final["fault"] is None and final["pending_requests"] == []
                print("MCP smoke passed: paging, explicit journal, fencing, drain, provider error, malformed and empty inventory")
            finally:
                os.killpg(process.pid, signal.SIGTERM)
                process.wait(timeout=8)


if __name__ == "__main__":
    main()
