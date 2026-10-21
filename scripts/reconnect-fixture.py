#!/usr/bin/env python3
"""Owned synthetic Hosts and an inspectable Unix proxy for native reconnect QA."""
import argparse, json, os, pathlib, signal, socket, subprocess, threading, time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('root', type=pathlib.Path, help='New absolute fixture directory')
ROOT = parser.parse_args().root
if not ROOT.is_absolute():
    parser.error('root must be absolute')
REPO = pathlib.Path(__file__).resolve().parent.parent
HOST = REPO / 'target/debug/fleetpane-host'
ROOT.mkdir(mode=0o700)
stop = threading.Event()
processes = []
servers = []
entries = []

def wait(probe):
    deadline = time.monotonic() + 15
    while not probe():
        assert time.monotonic() < deadline, 'fixture startup timeout'
        time.sleep(.025)

def raw(path, req):
    with socket.socket(socket.AF_UNIX) as s:
        s.settimeout(7)
        s.connect(str(path))
        s.sendall(json.dumps(req).encode() + b'\n')
        return json.loads(s.makefile().readline())

def serve(stream, index, actual):
    with stream:
        stream.settimeout(7)
        req = json.loads(stream.makefile().readline())
        mode = json.loads((ROOT / f'mode-{index}.json').read_text())['mode']
        with (ROOT / f'requests-{index}.jsonl').open('a') as f:
            f.write(json.dumps({'time':time.time(), 'mode':mode, 'request':req})+'\n')
        if mode == 'offline':
            return
        if mode == 'gap' and req['command'] == 'events':
            result = {'response':'error', 'data':{'code':'resync_required', 'message':'fixture expired cursor'}}
        else:
            result = raw(actual, req)
        if mode == 'replaced' and req['command'] == 'snapshot':
            result['data']['host_id'] = '00000000-0000-4000-8000-000000000123'
            result['data']['owner'] = None
        if mode == 'incompatible' and req['command'] == 'snapshot':
            result['data']['protocol'] = 999
        stream.sendall(json.dumps(result).encode()+b'\n')

def listen(server,index,actual):
    while not stop.is_set():
        try:
            stream,_ = server.accept()
        except socket.timeout:
            continue
        threading.Thread(target=serve,args=(stream,index,actual),daemon=True).start()

try:
    (ROOT / 'reasoning.json').write_text('{"stage":0}')
    for index,acp in enumerate([False,True]):
        home = ROOT / str(index)
        home.mkdir()
        actual = home / 'host/control.sock'
        with (home/'host.log').open('w') as f:
            p = subprocess.Popen([str(HOST),'serve-acp' if acp else 'serve',str(actual),str(home),'python3',str(REPO/'scripts'/('acp-fixture.py' if acp else 'provider-fixture.py')),*([] if acp else ['--reasoning-control',str(ROOT/'reasoning.json')])],stdout=subprocess.DEVNULL,stderr=f,start_new_session=True)
        processes.append(p)
        wait(lambda: 'ready:' in (home/'host.log').read_text() or p.poll() is not None)
        assert p.poll() is None
        def cmd(*args):
            return json.loads(subprocess.run([str(HOST),'session',str(actual),'--take-control',*args],text=True,capture_output=True,check=True,timeout=15).stdout)
        thread = cmd('create',str(home))['result']['thread']['id']
        cmd('send',thread,'[permission]' if acp else '[reasoning]')
        (ROOT/f'thread-{index}').write_text(thread)
        (ROOT / f'mode-{index}.json').write_text('{"mode":"online"}')
        proxy = ROOT/f'proxy-{index}.sock'
        server=socket.socket(socket.AF_UNIX)
        server.bind(str(proxy)); server.listen();server.settimeout(.1)
        servers.append(server)
        threading.Thread(target=listen,args=(server,index,actual),daemon=True).start()
        entries.append({'name':'DeepSeek permission' if acp else 'OpenCodex reconnect','socket':str(proxy)})
    (ROOT/'hosts.json').write_text(json.dumps({'version':1,'hosts':entries}))
    (ROOT/'pids.json').write_text(json.dumps([p.pid for p in processes]))
    signal.signal(signal.SIGTERM,lambda *_:stop.set())
    signal.signal(signal.SIGINT,lambda *_:stop.set())
    print('READY '+str(ROOT/'hosts.json'),flush=True)
    stop.wait()
finally:
    stop.set()
    for p in processes:
        if p.poll() is None: os.killpg(p.pid,signal.SIGTERM)
        p.wait(timeout=10)
    for s in servers:s.close()
