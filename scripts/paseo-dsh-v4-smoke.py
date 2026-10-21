#!/usr/bin/env python3
"""Offline DSH generation selection and v4 archive readback; synthetic data only."""

import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / "target/debug/fleetpane-host"
ANSWER = "## DeepSeek v4 历史\n\n最新历史 **完整保留**。\n\n| Version | State |\n| --- | --- |\n| v4 | Saved |"
TOOL_OUTPUT = "V4_TOOL_RESULT\n中文结果 retained"


def fixture(root, codec=None):
    home, cwd, dsh = (root / name for name in ("paseo", "工作区 with spaces", "dsh"))
    for path in (home / "projects", home / "agents/project", cwd, dsh):
        path.mkdir(parents=True, exist_ok=True)

    def write(path, value):
        path.write_text(json.dumps(value, ensure_ascii=False))

    write(
        home / "projects/workspaces.json",
        [
            {
                "workspaceId": "v4-workspace",
                "projectId": "project",
                "cwd": str(cwd),
                "kind": "directory",
                "displayName": "DeepSeek v4 迁移",
            }
        ],
    )
    write(
        home / "agents/project/dsh-v4.json",
        {
            "id": "dsh-v4",
            "provider": "deepseek-harness",
            "cwd": str(cwd),
            "workspaceId": "v4-workspace",
            "title": "最新 DeepSeek v4 会话",
            "persistence": {"provider": "deepseek-harness", "sessionId": "dsh-v4"},
        },
    )
    key = (
        "--"
        + str(cwd)
        .replace("/", "-")
        .lstrip("-")
        .replace("工作区", "~5DE5~4F5C~533A")
        .replace(" ", "~0020")
        + "--"
    )
    directory = dsh / "sessions" / key / "dsh-v4"
    directory.mkdir(parents=True, exist_ok=True)
    header = {
        "type": "session",
        "version": 4,
        "id": "dsh-v4",
        "cwd": str(cwd),
        "createdAt": 1,
        "isSeeded": False,
        "delegationDepth": 0,
    }
    events = [
        {
            "type": "user/message",
            "data": {
                "id": "user",
                "role": "user",
                "source": {"kind": "user"},
                "content": [{"type": "text", "text": "迁移后的历史问题"}],
            },
        },
        {
            "type": "assistant/message",
            "data": {
                "message": {
                    "id": "assistant",
                    "role": "assistant",
                    "source": {"kind": "model"},
                    "content": [{"type": "text", "text": ANSWER}],
                }
            },
        },
        {
            "type": "tool/call",
            "data": {
                "name": "read",
                "callId": "v4-call",
                "arguments": '{"path":"sample.txt"}',
            },
        },
        {
            "type": "tool/result",
            "data": {
                "message": {
                    "id": "result",
                    "role": "tool",
                    "toolCallId": "v4-call",
                    "source": {"kind": "tool", "callId": "v4-call"},
                    "content": [{"type": "text", "text": TOOL_OUTPUT}],
                }
            },
        },
        {
            "type": "synthetic/extension",
            "ignorable": True,
            "data": {"preserved": {"opaque": [False, 0, "未知记录"]}},
        },
    ]
    for seq, event in enumerate(events):
        event.update(seq=seq, time=seq + 1)
    records = [header, *events]
    suffix = ""
    if codec:
        # Exercise the installed provider's own released v4 encoder/decoder,
        # including native tool-role content. This is framing validation only;
        # it does not assert a runnable model-session lifecycle.
        script = """
import fs from 'node:fs';
import {pathToFileURL} from 'node:url';
import {zstdCompressSync} from 'node:zlib';
const {releasedV4SessionFormatCodec:codec} = await import(pathToFileURL(process.argv[1]));
const [header,...events]=JSON.parse(fs.readFileSync(0,'utf8'));
const {type,...logical}=header;
const encoded=[codec.encodeHeader(logical,0),...events.map(e=>codec.encodeEvent(e))];
const decoder=codec.createDecoder(encoded[0],'strict');
let count=0;
for(const row of encoded.slice(1)) decoder.decodeRow(row,{emitEvent:()=>count++,emitRun:()=>{throw Error('Unexpected run')}});
decoder.finish({});
if(count!==events.length) throw Error('Row loss');
for(const row of encoded) process.stdout.write(zstdCompressSync(Buffer.from(JSON.stringify(row)+'\\n')));
"""
        encoded = subprocess.run(
            ["node", "--input-type=module", "-e", script, str(codec)],
            input=json.dumps(records).encode(),
            capture_output=True,
            check=True,
        ).stdout
        suffix = ".zstd"
    else:
        encoded = "".join(
            json.dumps(r, ensure_ascii=False) + "\n" for r in records
        ).encode()
    path = directory / f"session.v4.jsonl{suffix}"
    path.write_bytes(encoded)
    # Valid older snapshot coexists with the authoritative current generation.
    old = [
        {**header, "version": 3},
        {
            "type": "assistant/message",
            "data": {
                "message": {"content": [{"type": "text", "text": "OLD_V3_SNAPSHOT"}]}
            },
        },
    ]
    old_text = "".join(json.dumps(r) + "\n" for r in old).encode()
    if codec:
        old_text = subprocess.run(
            [
                "node",
                "--input-type=module",
                "-e",
                "import fs from 'node:fs';import {zstdCompressSync} from 'node:zlib';process.stdout.write(zstdCompressSync(fs.readFileSync(0)));",
            ],
            input=old_text,
            capture_output=True,
            check=True,
        ).stdout
    (directory / f"session.v3.jsonl{suffix}").write_bytes(old_text)
    config = root / "hosts.json"
    write(
        config,
        {
            "version": 1,
            "hosts": [
                {"name": "DSH v4 offline", "socket": str(root / "unstarted.sock")}
            ],
        },
    )
    return home, dsh, config, path


def check(root, binary, codec, before):
    home, dsh, config, path = fixture(root, codec)
    store = root / "hosts.creations"

    def command(*args):
        process = subprocess.run(
            [str(binary), "paseo-import", *map(str, args)],
            check=False,
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert process.returncode == 0, process.stderr
        return json.loads(process.stdout)

    def hashes():
        return {
            str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
            for base in (home, dsh)
            for p in base.rglob("*")
            if p.is_file()
        }

    original = hashes()
    preview = command("preview", home)
    imported = command(
        "workspace",
        store,
        home,
        preview["candidates"][0]["revision"],
        config,
        "DSH v4 offline",
        "v4-workspace",
    )
    session = command("sessions", store, imported["id"])[0]
    status = command("copy-history", store, session["id"], root / "unused-codex", dsh)
    page = command("history", store, session["id"], 0)
    if before:
        assert "OLD_V3_SNAPSHOT" in json.dumps(page) and Path(
            status["source"]
        ).name.startswith("session.v3"), page
        print(json.dumps({"before_latest_v4_silently_ignored": "REPRODUCED"}))
        return
    assert (
        Path(status["source"]) == path
        and status["rows"] == 4
        and status["source_records"] == 6
    ), status
    assert page["items"][1]["text"] == ANSWER and page["next"] is None
    assert page["items"][3]["call"]["rawOutput"] == [
        {"type": "text", "text": TOOL_OUTPUT}
    ]
    assert command("source-record", store, session["id"], 6) == {
        "type": "synthetic/extension",
        "ignorable": True,
        "data": {"preserved": {"opaque": [False, 0, "未知记录"]}},
        "seq": 4,
        "time": 5,
    }
    assert hashes() == original and not (root / "unstarted.sock").exists()
    path.unlink()
    assert command("history", store, session["id"], 0) == page
    assert (
        command("copy-history", store, session["id"], root / "unused-codex", dsh)
        == status
    )
    print(
        json.dumps(
            {
                "latest_v4_selected": "PASS",
                "exact_markdown_tool_and_unknown_record": "PASS",
                "offline_reopen_without_source": "PASS",
                "source_unchanged_before_removal": "PASS",
                "installed_v4_codec": bool(codec),
            }
        )
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture", type=Path)
    parser.add_argument("--binary", type=Path, default=BIN)
    parser.add_argument("--dsh-codec", type=Path)
    parser.add_argument("--before", action="store_true")
    args = parser.parse_args()
    if args.fixture:
        fixture(args.fixture.resolve(), args.dsh_codec)
        print(args.fixture.resolve())
    else:
        with tempfile.TemporaryDirectory(prefix="fp-dsh-v4-") as directory:
            check(Path(directory), args.binary, args.dsh_codec, args.before)
