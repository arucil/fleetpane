import json,socket,subprocess,sys,time
from pathlib import Path
repo=Path('/home/amlo/Projects/Rust/fleetpane');root=Path('/tmp/fp-modified-enter');folder=root/'work 中文';folder.mkdir(exist_ok=True)
binary=repo/'target/debug/fleetpane-host'
def cli(*args):return json.loads(subprocess.check_output([str(binary),*map(str,args)],text=True,timeout=20))
def snap(endpoint):
 with socket.socket(socket.AF_UNIX) as s:
  s.connect(str(endpoint));s.sendall(b'{"command":"snapshot"}\n');return json.loads(s.makefile().readline())['data']
hosts=[]
for name,acp in [('Keys',False),('Independent ACP',True)]:
 home=root/('acp' if acp else 'codex');home.mkdir(exist_ok=True);endpoint=home/'host/control.sock';log=home/'host.log'
 with log.open('w') as out:
  p=subprocess.Popen([str(binary),'serve-acp' if acp else 'serve',str(endpoint),str(home),sys.executable,str(repo/'scripts'/('acp-fixture.py' if acp else 'provider-fixture.py'))],stdout=subprocess.DEVNULL,stderr=out,start_new_session=True)
 (home/'pid').write_text(str(p.pid));end=time.monotonic()+15
 while not endpoint.exists() or 'ready:' not in log.read_text():
  assert p.poll() is None,log.read_text()
  assert time.monotonic()<end;time.sleep(.02)
 hosts.append({'name':name,'socket':str(endpoint)})
 if acp:
  thread=cli('session',endpoint,'--take-control','create',folder)['result']['thread']['id']
  cli('session',endpoint,'--take-control','send',thread,'[permission]')
  while not snap(endpoint)['pending_requests']:
   assert time.monotonic()<end;time.sleep(.02)
  (root/'independent-before.json').write_text(json.dumps(snap(endpoint),indent=2)+'\n')
config=root/'hosts.json';config.write_text(json.dumps({'version':1,'hosts':hosts}))
store=config.with_suffix('.creations');w=cli('workspace',store,'add',config,'Keys','folder',folder)
t=cli('terminal',store,w['id'],'--take-control','create','Modifier capture')
(root/'workspace.json').write_text(json.dumps(w));(root/'terminal.json').write_text(json.dumps(t));print(json.dumps({'workspace':w['id'],'terminal':t['id']}))
