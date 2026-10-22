import json,socket,subprocess,sys,time
from pathlib import Path
repo=Path('/home/amlo/Projects/Rust/fleetpane');root=Path('/tmp/fp-model-search');folder=root/'work 中文';folder.mkdir(exist_ok=True)
binary=repo/'target/debug/fleetpane-host'
def cli(*args):return json.loads(subprocess.check_output([str(binary),*map(str,args)],text=True,timeout=20))
def snap(endpoint):
 with socket.socket(socket.AF_UNIX) as s:
  s.connect(str(endpoint));s.sendall(b'{"command":"snapshot"}\n');return json.loads(s.makefile().readline())['data']
models=root/'models.json'
models.write_text(json.dumps([{'id':f'picker-{i}', 'model':f'wire-{i}', 'displayName': 'Same model' if i in (5,6) else f'模型 {i}', 'description':'Searchable descriptions' if i != 71 else 'Unique keyword cobalt', 'hidden':i==70, 'isDefault':i==0, 'defaultReasoningEffort':'future-effort', 'supportedReasoningEfforts':[{'reasoningEffort':'future-effort','description':'Future'},{'reasoningEffort':'low','description':'Low'}]} for i in range(72)]))
history=root/'history.json';history.write_text(json.dumps([{'id':'sidebar-a','cwd':str(folder),'name':'Session alpha long title','preview':'alpha','status':{'type':'idle'},'turns':[{'id':'saved','status':'completed','items':[{'id':'answer','type':'agentMessage','text':'Sidebar resize fixture 中文.'}]}]},{'id':'sidebar-b','cwd':str(folder),'name':'Session beta','preview':'beta','status':{'type':'idle'},'turns':[]}]))
hosts=[]
for name,acp in [('Keys',False),('ACP choices',True)]:
 home=root/('acp' if acp else 'codex');home.mkdir(exist_ok=True);endpoint=home/'host/control.sock';log=home/'host.log'
 with log.open('w') as out:
  p=subprocess.Popen([str(binary),'serve-acp' if acp else 'serve',str(endpoint),str(home),sys.executable,str(repo/'scripts'/('acp-fixture.py' if acp else 'provider-fixture.py')),*([] if acp else ['--history',str(history),'--models',str(models),'--send-effects',str(root/'send-effects.jsonl')])],stdout=subprocess.DEVNULL,stderr=out,start_new_session=True)
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
  cli('session',endpoint,'--take-control','create',folder)
  (root/'independent-before.json').write_text(json.dumps(snap(endpoint),indent=2)+'\n')
config=root/'hosts.json';config.write_text(json.dumps({'version':1,'hosts':hosts}))
store=config.with_suffix('.creations');w=cli('workspace',store,'add',config,'Keys','folder',folder)
cli('session',hosts[0]['socket'],'--take-control','list')
(root/'workspace.json').write_text(json.dumps(w));print(json.dumps({'workspace':w['id']}))
