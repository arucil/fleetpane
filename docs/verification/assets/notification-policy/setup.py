from pathlib import Path
import json,os,socket,subprocess,time,sqlite3
r=Path('/tmp/fp-alert-policy');r.mkdir(exist_ok=True)
repo=Path('/home/amlo/Projects/Rust/fleetpane');binary=repo/'target/debug/fleetpane-host'
def cli(*args):
 p=subprocess.run([str(binary),*map(str,args)],capture_output=True,text=True,timeout=30)
 if p.returncode:raise RuntimeError(p.stderr)
 return json.loads(p.stdout)
def snap(endpoint):
 with socket.socket(socket.AF_UNIX) as c:
  c.connect(endpoint);c.sendall(b'{"command":"snapshot"}\n');return json.loads(c.makefile().readline())['data']
hosts=[];processes=[]
for i,name in enumerate(['Codex','DeepSeek-A','DeepSeek-B']):
 home=r/name;home.mkdir(mode=0o700,exist_ok=True);endpoint=home/'host/control.sock'
 with (home/'log').open('w') as out:
  p=subprocess.Popen([str(binary),'serve' if not i else 'serve-acp',str(endpoint),str(home),'python3',str(repo/'scripts'/('provider-fixture.py' if not i else 'acp-fixture.py'))],stdout=subprocess.DEVNULL,stderr=out,env={**os.environ,'CODEX_HOME':str(home/'codex'),'DSH_HOME':str(home/'dsh')},start_new_session=True)
 processes.append(p.pid);hosts.append({'name':name,'socket':str(endpoint)})
 deadline=time.monotonic()+15
 while not endpoint.exists():
  assert p.poll() is None,(home/'log').read_text()
  assert time.monotonic()<deadline
  time.sleep(.05)
(r/'hosts.json').write_text(json.dumps({'version':1,'hosts':hosts}))
workspaces=[]
for i,name,prompt in [(0,'alpha 中文','[questions]'),(0,'beta','[approval]'),(1,'alpha 中文','[permission]'),(2,'alpha 中文','[permission]')]:
 path=r/name;path.mkdir(exist_ok=True);(path/'notes.md').write_text('# '+name+'\n\nNext pending request validation.\n')
 w=cli('workspace',r/'hosts.creations','add',r/'hosts.json',hosts[i]['name'],'folder',path)
 t=cli('session',hosts[i]['socket'],'--take-control','create',path)['result']['thread']
 cli('session',hosts[i]['socket'],'--take-control','send',t['id'],prompt)
 workspaces.append({'workspace':w,'session':t['id'],'host_id':snap(hosts[i]['socket'])['host_id']})
(r/'baseline.json').write_text(json.dumps({h['name']:snap(h['socket']) for h in hosts}))
(r/'setup.json').write_text(json.dumps({'hosts':hosts,'pids':processes,'workspaces':workspaces},ensure_ascii=False,indent=2))
print(json.dumps({'pids':processes,'sessions':[(w['workspace']['endpoint']['name'],w['session']) for w in workspaces],'requests':[len(snap(h['socket'])['pending_requests']) for h in hosts]},indent=2))

path=r/'results';path.mkdir(exist_ok=True)
t=cli('session',hosts[0]['socket'],'--take-control','create',path)['result']['thread']
for prompt in ['hello results','[fail]']:
 before=len(snap(hosts[0]['socket'])['attention']['records'])
 cli('session',hosts[0]['socket'],'--take-control','send',t['id'],prompt)
 deadline=time.monotonic()+15
 while len(snap(hosts[0]['socket'])['attention']['records'])<=before:
  assert time.monotonic()<deadline
  time.sleep(.05)
(r/'baseline.json').write_text(json.dumps({h['name']:snap(h['socket']) for h in hosts}))
print(json.dumps({'pending':[len(snap(h['socket'])['pending_requests']) for h in hosts],'results':snap(hosts[0]['socket'])['attention']['records']}))
