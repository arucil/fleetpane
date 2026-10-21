import json,subprocess,sys,time
from pathlib import Path
repo=Path('/home/amlo/Projects/Rust/fleetpane');root=Path('/tmp/fp-text-preview');folder=root/'work 中文'
history=root/'history.json'
history.write_text(json.dumps([{'id':'preview-link','cwd':str(folder),'name':'Source anchors','preview':'Source anchors','status':{'type':'idle'},'turns':[{'id':'saved','status':'completed','items':[{'id':'answer','type':'agentMessage','text':'[Source line 1000](source.rs#L1000)\n\n[Missing line 9999](source.rs#L9999)'}]}]}]))
endpoint=root/'links/control.sock';log=root/'links.log'
with log.open('w') as out:
 p=subprocess.Popen([str(repo/'target/debug/fleetpane-host'),'serve',str(endpoint),str(folder),sys.executable,str(repo/'scripts/provider-fixture.py'),'--history',str(history)],stdout=subprocess.DEVNULL,stderr=out,start_new_session=True)
(root/'link-pid').write_text(str(p.pid));end=time.monotonic()+15
while not endpoint.exists() or 'ready:' not in log.read_text():
 assert p.poll() is None,log.read_text()
 assert time.monotonic()<end;time.sleep(.025)
subprocess.run([str(repo/'target/debug/fleetpane-host'),'session',str(endpoint),'--take-control','list'],check=True,stdout=subprocess.DEVNULL)
config=root/'hosts.json';data=json.loads(config.read_text());data['hosts'].append({'name':'Source links','socket':str(endpoint)});config.write_text(json.dumps(data))
print('Source links ready')
