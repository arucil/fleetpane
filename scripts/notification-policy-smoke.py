#!/usr/bin/env python3
"""Exercise the real alert-policy CLI with private synthetic local storage."""
import json
from pathlib import Path
import subprocess
import tempfile
import uuid

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/fleetpane-host"


def cli(store, *args, payload=None, ok=True):
    result = subprocess.run(
        [str(BINARY), "notification-policy", str(store), *map(str, args)],
        input=payload,
        capture_output=True,
        timeout=15,
    )
    assert (result.returncode == 0) == ok, result.stderr.decode()
    return json.loads(result.stdout) if ok else result.stderr.decode()


with tempfile.TemporaryDirectory(prefix="fp-policy-smoke-") as temporary:
    root = Path(temporary)
    store = root / "preferences"
    policy = {"muted": False, "muted_kinds": [], "muted_hosts": [], "muted_sessions": []}
    initial = cli(store, "get")
    assert initial == {"revision": 0, "value": policy}
    policy["muted"] = True
    policy["muted_kinds"] = ["failed"]
    policy["muted_hosts"] = [str(root / "missing-host.sock")]
    policy["muted_sessions"] = [{"socket": str(root / "other.sock"), "host_id": str(uuid.uuid4()), "thread_id": "same"}]
    saved = cli(store, "set", 0, payload=json.dumps(policy).encode())
    assert saved == {"revision": 1, "value": policy}
    assert cli(store, "get") == saved
    assert "another window" in cli(store, "set", 0, payload=json.dumps(policy).encode(), ok=False)
    assert cli(store, "get") == saved
    for data in [b"invalid", b"{}", b"x" * 524289, json.dumps({**policy, "future": True}).encode(), json.dumps({**policy, "muted_hosts": ["relative"]}).encode()]:
        absent = root / str(uuid.uuid4())
        cli(absent, "set", 0, payload=data, ok=False)
        assert not absent.exists(), "invalid policy must be rejected before opening a store"
    bad = {**policy, "muted_sessions": policy["muted_sessions"] * 2}
    cli(store, "set", 1, payload=json.dumps(bad).encode(), ok=False)
    assert cli(store, "get") == saved
    policy["muted"] = False
    unmuted = cli(store, "set", 1, payload=json.dumps(policy).encode())
    assert unmuted["revision"] == 2
    assert not unmuted["value"]["muted"]
    assert unmuted["value"]["muted_kinds"] == ["failed"], "global unmute preserves scoped rules"
    assert not (root / "missing-host.sock").exists()
    assert not (root / "other.sock").exists()
    print("notification policy smoke: persistence, CAS, exact scopes, invalid-input bounds and no Host dependency passed")
