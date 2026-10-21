#!/usr/bin/env python3
"""Private synthetic Paseo archives -> independent Codex fork; optional real app-server.

--real uses an empty CODEX_HOME and a local rejecting model endpoint. No account,
external model call, existing session resume or source-provider process is used.
"""
import argparse
import hashlib
import json
import os
import signal
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / "target/debug/fleetpane-host"
MARKER = "FLEETPANE_IMPORTED_CONTEXT_中文_42"


class Model(BaseHTTPRequestHandler):
    requests = []

    def do_POST(self):
        type(self).requests.append(json.loads(self.rfile.read(int(self.headers["Content-Length"]))))
        body = json.dumps({"error":{"message":"Intentional local rejection","type":"invalid_request_error"}}).encode()
        self.send_response(400)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_args):
        pass


def cli(*args, ok=True):
    result = subprocess.run([str(BIN), *map(str,args)],capture_output=True,text=True,timeout=30)
    assert (result.returncode == 0) == ok, result.stderr
    return json.loads(result.stdout) if ok else result.stderr


def fixture(root):
    root.mkdir(parents=True, exist_ok=True)
    home, source, cwd = root/"paseo", root/"source-codex", root/"工作区 with space"
    for path in [home/"projects", home/"agents/project", source, cwd]:
        path.mkdir(parents=True)
    ident = str(uuid.uuid4())
    (home/"projects/workspaces.json").write_text(json.dumps([{"workspaceId":"one","projectId":"project","cwd":str(cwd),"kind":"directory","displayName":"Fork workspace","title":"历史迁移","archivedAt":None,"pinnedAt":None}]))
    (home/"agents/project/session.json").write_text(json.dumps({"id":"paseo-one","provider":"codex","cwd":str(cwd),"workspaceId":"one","title":"Saved 中文 context","lastStatus":"idle","persistence":{"provider":"codex","sessionId":ident}}))
    records=[
        {"timestamp":"2026-10-20T00:00:00Z","type":"session_meta","payload":{"id":ident,"timestamp":"2026-10-20T00:00:00Z","cwd":str(cwd),"originator":"codex_cli_rs","cli_version":"0.163.0","source":"cli","model_provider":"fixture","history_mode":"legacy"}},
        {"timestamp":"2026-10-20T00:00:01Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Remember this imported value: "+MARKER}]}},
        {"timestamp":"2026-10-20T00:00:02Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"## Saved context\n\n"+MARKER}]}}
    ]
    # Real Codex renders legacy transcript rows from event_msg; response_item
    # records independently carry model context. A real rollout contains both.
    records.extend([
        {"timestamp":"2026-10-20T00:00:01Z","type":"event_msg","payload":{"type":"user_message","message":"Remember this imported value: "+MARKER,"images":None,"local_images":[]}},
        {"timestamp":"2026-10-20T00:00:02Z","type":"event_msg","payload":{"type":"agent_message","message":"## Saved context\n\n"+MARKER}}
    ])
    log = source/"history.jsonl"
    log.write_text("".join(json.dumps(r,ensure_ascii=False)+"\n" for r in records))
    with sqlite3.connect(source/"state_5.sqlite") as db:
        db.execute("CREATE TABLE threads(id TEXT PRIMARY KEY,rollout_path TEXT)")
        db.execute("INSERT INTO threads VALUES(?,?)",(ident,str(log)))
    config=root/"hosts.json"
    endpoint=root/"host/control.sock"
    config.write_text(json.dumps({"version":1,"hosts":[{"name":"Fork QA","socket":str(endpoint)}]}))
    store=config.with_suffix(".creations")
    preview=cli("paseo-import","preview",home)
    imported=cli("paseo-import","workspace",store,home,preview["candidates"][0]["revision"],config,"Fork QA","one")
    session=cli("paseo-import","sessions",store,imported["id"])[0]
    cli("paseo-import","copy-history",store,session["id"],source,root/"unused-dsh")
    return {"root":root,"store":store,"socket":endpoint,"cwd":cwd,"source":source,"log":log,"session":session,"config":config}


def start(data, real=False, hold=False, port=0, opencodex=False):
    root, endpoint=data["root"], data["socket"]
    env=os.environ.copy()
    if real:
        home=root/"target-codex"
        home.mkdir(mode=0o700, exist_ok=True)
        model = "gpt-6-astra" if opencodex else "fixture"
        base = "http://127.0.0.1:10100/v1" if opencodex else f"http://127.0.0.1:{port}/v1"
        (home/"config.toml").write_text(f'''model = "{model}"
model_provider = "fixture"
[model_providers.fixture]
name = "Fleetpane local test"
base_url = "{base}"
wire_api = "responses"
requires_openai_auth = false
request_max_retries = 0
stream_max_retries = 0
''')
        env={key:value for key,value in env.items() if key not in {"OPENAI_API_KEY","CODEX_API_KEY","OPENAI_BASE_URL"}}
        env["CODEX_HOME"]=str(home)
        provider=["codex","app-server","--stdio"]
    else:
        provider=[sys.executable,str(ROOT/"scripts/provider-fixture.py"),"--creation-effects",str(root/"effects.jsonl")]
        if hold:
            provider.append("--hold-creation")
    with (root/"host.log").open("w") as output:
        process=subprocess.Popen([str(BIN),"serve",str(endpoint),str(data["cwd"]),*provider],env=env,stdout=subprocess.DEVNULL,stderr=output,start_new_session=True)
    deadline=time.monotonic()+20
    while "ready:" not in (root/"host.log").read_text():
        assert process.poll() is None,(root/"host.log").read_text()
        assert time.monotonic()<deadline,"Host readiness timeout"
        time.sleep(.05)
    return process


def stop(process):
    if process.poll() is None:
        os.killpg(process.pid,signal.SIGTERM)
        process.wait(timeout=10)
    # The Host owns a Node launcher which owns the native Codex process. Waiting
    # for the Host alone does not wait for that launcher to reap its child.
    deadline=time.monotonic()+10
    while True:
        rows=subprocess.run(["ps","-eo","pgid=,stat="],capture_output=True,text=True,check=True).stdout.splitlines()
        live=[row for row in rows if row.split()[0]==str(process.pid) and not row.split()[1].startswith("Z")]
        if not live:
            break
        assert time.monotonic()<deadline,"test provider process group did not stop"
        time.sleep(.05)


def exercise(data, real=False, port=0, opencodex=False):
    process=start(data,real,port=port,opencodex=opencodex)
    try:
        store,session=data["store"],data["session"]["id"]
        source=data["log"].read_bytes()
        data["log"].unlink() # Continue only from the published archive.
        before=hashlib.sha256((store/"paseo-history"/f"{session}.sqlite").read_bytes()).hexdigest()
        intents=[]
        workers=[subprocess.Popen([str(BIN),"paseo-import","prepare-fork",str(store),session],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True) for _ in range(4)]
        for worker in workers:
            out,error=worker.communicate(timeout=15)
            assert worker.returncode==0,error
            intents.append(json.loads(out))
        assert all(i==intents[0] for i in intents)
        intent=intents[0]
        assert Path(intent["path"]).read_bytes()==source
        assert not (data["root"]/"effects.jsonl").exists()
        assert not Model.requests
        assert cli("paseo-import","fork-status",store,session)=={"status":"prepared"}
        if real:
            cli("session",data["socket"],"--take-control","drain")
            cli("session",data["socket"],"--take-control","ready")
            replacement=cli("paseo-import","reprepare-fork",store,session,intent["id"])
            assert replacement["id"]!=intent["id"] and replacement["path"]==intent["path"]
            assert replacement["target"]["admission_generation"]>intent["target"]["admission_generation"]
            assert not replacement["attempted"] and not Model.requests
            intent=replacement
        fork=cli("paseo-import","fork",store,session,"--take-control")["thread"]
        assert fork["id"]!=intent["thread_id"] and fork["cwd"]==str(data["cwd"].resolve())
        assert fork["status"]["type"]=="idle" and fork["turns"]==[]
        assert not Model.requests, "fork unexpectedly contacted the model"
        assert cli("paseo-import","fork-status",store,session)=={"status":"created","thread_id":fork["id"]}
        assert "already attempted" in cli("paseo-import","fork",store,session,"--take-control",ok=False)
        opened=cli("session",data["socket"],"--take-control","open",fork["id"])["result"]["thread"]
        assert MARKER in json.dumps(opened,ensure_ascii=False),opened
        if real:
            cli("session",data["socket"],"--take-control","send",fork["id"],"What value was in the imported conversation? Reply with only that exact value. Do not use tools.")
            if not opencodex:
                deadline=time.monotonic()+10
                while not Model.requests:
                    assert time.monotonic()<deadline,"model did not receive continuation"
                    time.sleep(.05)
                assert MARKER in json.dumps(Model.requests,ensure_ascii=False)
            deadline=time.monotonic()+90
            while True:
                current=cli("session",data["socket"],"--take-control","open",fork["id"])["result"]["thread"]
                if current["turns"] and current["turns"][-1]["status"] in {"failed","completed","interrupted"}:
                    break
                assert time.monotonic()<deadline,"continuation turn did not settle"
                time.sleep(.05)
            if opencodex:
                turn=current["turns"][-1]
                assert turn["status"]=="completed",turn
                assert any(item["type"]=="agentMessage" and MARKER in item["text"] for item in turn["items"]),turn
            else:
                assert current["turns"][-1]["status"]=="failed"
        else:
            assert len((data["root"]/"effects.jsonl").read_text().splitlines())==1
        assert hashlib.sha256((store/"paseo-history"/f"{session}.sqlite").read_bytes()).hexdigest()==before
        assert cli("paseo-import","prepare-fork",store,session)["id"]==intent["id"]
        assert "already attempted" in cli("paseo-import","reprepare-fork",store,session,intent["id"],ok=False)
        return fork["id"]
    finally:
        stop(process)


def interrupted(data):
    process=start(data,hold=True)
    sender=None
    try:
        store,session=data["store"],data["session"]["id"]
        intent=cli("paseo-import","prepare-fork",store,session)
        sender=subprocess.Popen([str(BIN),"paseo-import","fork",str(store),session,"--take-control"],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        deadline=time.monotonic()+10
        while not (data["root"]/"effects.jsonl").exists():
            assert time.monotonic()<deadline,"fork not dispatched"
            time.sleep(.05)
        sender.kill();sender.communicate(timeout=5)
        assert cli("paseo-import","fork-status",store,session)=={"status":"pending"}
        assert "already attempted" in cli("paseo-import","reprepare-fork",store,session,intent["id"],ok=False)
        assert "already attempted" in cli("paseo-import","fork",store,session,"--take-control",ok=False)
    finally:
        if sender and sender.poll() is None:
            sender.kill();sender.communicate(timeout=5)
        stop(process)
    process=start(data,hold=True)
    try:
        result=cli("paseo-import","fork-status",store,session)
        assert result["status"]=="unknown",result
        assert "already attempted" in cli("paseo-import","reprepare-fork",store,session,intent["id"],ok=False)
        assert cli("paseo-import","prepare-fork",store,session)["id"]==intent["id"]
        assert "already attempted" in cli("paseo-import","fork",store,session,"--take-control",ok=False)
        assert len((data["root"]/"effects.jsonl").read_text().splitlines())==1
    finally:
        stop(process)


def stale_preparation(data):
    process=start(data)
    try:
        store,session=data["store"],data["session"]["id"]
        intent=cli("paseo-import","prepare-fork",store,session)
        cli("session",data["socket"],"--take-control","drain")
        assert "availability changed" in cli("paseo-import","fork",store,session,"--take-control",ok=False)
        cli("session",data["socket"],"--take-control","ready")
        assert "availability changed" in cli("paseo-import","fork",store,session,"--take-control",ok=False)
        assert cli("paseo-import","prepare-fork",store,session)==intent
        assert cli("paseo-import","fork-status",store,session)=={"status":"prepared"}
        assert not (data["root"]/"effects.jsonl").exists()
        # A failed reprepare does not discard the saved binding.
        cli("session",data["socket"],"--take-control","drain")
        assert "availability changed" in cli("paseo-import","reprepare-fork",store,session,intent["id"],ok=False)
        assert cli("paseo-import","prepare-fork",store,session)==intent
        cli("session",data["socket"],"--take-control","ready")
        # Concurrent controllers must name the same expected operation; one wins.
        workers=[subprocess.Popen([str(BIN),"paseo-import","reprepare-fork",str(store),session,intent["id"]],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True) for _ in range(2)]
        results=[]
        for worker in workers:
            out,error=worker.communicate(timeout=15)
            results.append((worker.returncode,out,error))
        assert sum(code==0 for code,_,_ in results)==1,results
        changed=json.loads(next(out for code,out,_ in results if code==0))
        assert changed["id"]!=intent["id"] and not changed["attempted"]
        assert changed["path"]==intent["path"]
        assert changed["target"]["admission_generation"]>intent["target"]["admission_generation"]
        assert "binding changed" in cli("paseo-import","reprepare-fork",store,session,intent["id"],ok=False)
        assert not (data["root"]/"effects.jsonl").exists()
    finally:
        stop(process)
    # Rebinding after Host replacement retains the archived context/export.
    cli("paseo-import","reprepare-fork",store,session,changed["id"],ok=False)
    assert cli("paseo-import","prepare-fork",store,session)==changed
    process=start(data)
    try:
        renewed=cli("paseo-import","reprepare-fork",store,session,changed["id"])
        assert renewed["target"]["host_id"]!=changed["target"]["host_id"]
        assert renewed["id"]!=changed["id"] and renewed["path"]==changed["path"]
        assert not (data["root"]/"effects.jsonl").exists()
        fork=cli("paseo-import","fork",store,session,"--take-control")["thread"]
        assert fork["id"]!=renewed["thread_id"]
        assert len((data["root"]/"effects.jsonl").read_text().splitlines())==1
        assert "already attempted" in cli("paseo-import","reprepare-fork",store,session,renewed["id"],ok=False)
    finally:
        stop(process)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--real",action="store_true")
    parser.add_argument("--opencodex",action="store_true",help="One external model turn through the existing loopback OpenCodex proxy; isolated provider state")
    parser.add_argument("--fixture",type=Path)
    args=parser.parse_args()
    if args.opencodex:
        args.real=True
    if args.fixture:
        data=fixture(args.fixture.resolve())
        print(json.dumps({k:str(v) if isinstance(v,Path) else v for k,v in data.items()}))
        return
    server=ThreadingHTTPServer(("127.0.0.1",0),Model) if args.real and not args.opencodex else None
    if server:
        threading.Thread(target=server.serve_forever,daemon=True).start()
    try:
        with tempfile.TemporaryDirectory(prefix="fp-fork-") as directory:
            root=Path(directory)
            thread=exercise(fixture(root/"complete"),args.real,server.server_port if server else 0,args.opencodex)
            if not args.real:
                interrupted(fixture(root/"interrupted"))
                stale_preparation(fixture(root/"stale"))
            print(json.dumps({"independent_fork":True,"archived_context_readback":True,"concurrent_prepare_one_binding":True,"duplicate_refused":True,"source_archive_unchanged":True,"interrupted_receipt_no_replay":"not_run" if args.real else "pass","explicit_reprepare_unattempted_only":"pass","real_codex_app_server":args.real,"model_request_contains_imported_context":"not_run" if args.opencodex else bool(Model.requests),"opencodex_model_recalled_imported_value":args.opencodex,"thread_id":thread}))
    finally:
        if server:
            server.shutdown();server.server_close()


if __name__=="__main__":
    main()
