#!/usr/bin/env python3
"""Installed Codex MCP discovery against a local stdio fixture; no model calls."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time


def main():
    repo = Path(__file__).resolve().parents[1]
    binary = repo / "target/debug/fleetpane-host"
    with tempfile.TemporaryDirectory(prefix="fp-codex-mcp-") as directory:
        root = Path(directory)
        home = root / "home"
        home.mkdir(mode=0o700)
        (home / "config.toml").write_text("[mcp_servers.fleetpane_fixture]\ncommand = " + json.dumps(sys.executable)
            + "\nargs = [" + json.dumps(str(repo / "scripts/mcp-fixture.py")) + "]\n")
        env = {key:value for key,value in os.environ.items() if not key.startswith(("OPENAI_", "CODEX_"))}
        env["CODEX_HOME"] = str(home)
        socket = root / "private/control.sock"
        with (root / "host.log").open("w+") as log:
            process = subprocess.Popen([str(binary), "serve", str(socket), str(root)], env=env,
                stdout=subprocess.DEVNULL, stderr=log, start_new_session=True)
            try:
                deadline = time.monotonic() + 20
                while not socket.exists() or "ready:" not in (root / "host.log").read_text():
                    assert process.poll() is None and time.monotonic() < deadline, (root / "host.log").read_text()
                    time.sleep(0.02)
                result = subprocess.run([str(binary), "session", str(socket), "--take-control", "mcp"],
                    env=env, capture_output=True, text=True, timeout=30, check=True)
                page = json.loads(result.stdout)["result"]
                servers = [s for s in page["data"] if s["name"] == "fleetpane_fixture"]
                assert len(servers) == 1 and page["nextCursor"] is None, page
                server = servers[0]
                assert server["authStatus"] == "unsupported", server
                assert server.get("runtimeStatus") is None, server
                assert any(tool["name"] == "echo" and tool["description"] == "Synthetic MCP inventory 中文"
                    for tool in server["tools"].values()), server
                print(json.dumps({"installed_codex":subprocess.check_output(["codex","--version"],text=True).strip(),
                    "server":server["name"],"tools":len(server["tools"]),"auth":server["authStatus"],
                    "runtime":server.get("runtimeStatus"),"model_calls":0}))
            finally:
                os.killpg(process.pid, signal.SIGTERM)
                process.wait(timeout=8)


if __name__ == "__main__":
    main()
