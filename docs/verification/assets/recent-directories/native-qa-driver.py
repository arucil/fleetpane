import json,os,pathlib,signal,socket,subprocess,sys,time
ROOT=pathlib.Path('/tmp/fleetpane-recent-qa'); REPO=pathlib.Path('/home/amlo/Projects/Rust/fleetpane'); HOST=REPO/'target/debug/fleetpane-host'
CONFIG=ROOT/'hosts.json'; STORE=ROOT/'hosts.creations'
def wait(probe):
    deadline=time.monotonic()+10
    while True:
        v=probe()
        if v:return v
        assert time.monotonic()<deadline,'native QA observation timeout'
        time.sleep(.025)
def raw(endpoint,request):
    with socket.socket(socket.AF_UNIX) as s:
        s.settimeout(5);s.connect(str(endpoint));s.sendall(json.dumps(request).encode()+b'\n');return json.loads(s.makefile().readline())['data']
def snapshot(endpoint):return raw(endpoint,{'command':'snapshot'})
def recent(verb,endpoint,*tail):
    return json.loads(subprocess.run([str(HOST),'recent',str(STORE),verb,str(endpoint),*map(str,tail)],capture_output=True,text=True,check=True,timeout=8).stdout)
def read():
    out={}
    for entry in json.loads(CONFIG.read_text())['hosts']:
        e=entry['socket'];s=snapshot(e)
        page=raw(e,{'command':'query','lease':s['owner'],'method':'thread/list','params':{'limit':50}}) if s['owner'] else None
        out[entry['name']]={'snapshot':s,'list':page,'recent':recent('list',e)}
    (ROOT/'read-receipt.json').write_text(json.dumps(out,ensure_ascii=False,indent=2)); print(json.dumps({k:{'recent':v['recent'],'provider':v['snapshot']['provider_pid']} for k,v in out.items()},ensure_ascii=False))
if __name__ == "__main__":
    if sys.argv[1]=='init':
        ROOT.mkdir()
        for name in ['alpha 中文','beta space','gamma','delta','save-failure','other-host']:(ROOT/name).mkdir()
        entries=[];pids=[]
        for n,acp in enumerate([False,True]):
            endpoint=ROOT/f'host-{n}/control.sock'; log=ROOT/f'host-{n}.log'
            with log.open('w') as f:
                p=subprocess.Popen([str(HOST),'serve-acp' if acp else 'serve',str(endpoint),str(ROOT),'python3',str(REPO/'scripts'/('acp-fixture.py' if acp else 'provider-fixture.py'))],stdout=subprocess.DEVNULL,stderr=f,start_new_session=True)
            pids.append(p.pid);wait(lambda:endpoint.exists() and 'ready:' in log.read_text());entries.append({'name':'DeepSeek QA' if acp else 'OpenCodex QA','socket':str(endpoint)})
        CONFIG.write_text(json.dumps({'version':1,'hosts':entries}));(ROOT/'host-pids.json').write_text(json.dumps(pids))
        recent('remember',entries[1]['socket'],ROOT/'other-host')
    elif sys.argv[1]=='stop':
        for pid,entry in zip(json.loads((ROOT/'host-pids.json').read_text()),json.loads(CONFIG.read_text())['hosts']):
            args=pathlib.Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0');assert entry['socket'].encode() in args and os.getpgid(pid)==pid;os.killpg(pid,signal.SIGINT);wait(lambda:not pathlib.Path(entry['socket']).exists())
        sys.exit()
    elif sys.argv[1]!='read':raise ValueError(sys.argv[1])
    read()
