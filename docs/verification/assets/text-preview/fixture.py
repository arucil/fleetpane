import json,os,socket,subprocess,sys,time
from pathlib import Path
repo=Path('/home/amlo/Projects/Rust/fleetpane');root=Path('/tmp/fp-text-preview')
folder=root/'work 中文';folder.mkdir(exist_ok=True)
source='// START 原文\r\nfn main() {\r\n    let text = "中文 <not html>  ";\r\n}\r\n'
source+='// LONG '+ 'abcdef 中文 '*70+' LONG_END\r\n'
source+=''.join(f'// line {i:04}: 中文 exact  \r\n' for i in range(6,1201))
source+='// END_MARKER final  \r\n'
(folder/'source.rs').write_bytes(source.encode())
(folder/'empty.txt').write_bytes(b'')
(folder/'unknown.xyz').write_text('plaintext 原文\nsecond line\n')
control=root/'host/control.sock';log=root/'host.log';binary=repo/'target/debug/fleetpane-host'
with log.open('w') as out:
 p=subprocess.Popen([str(binary),'serve-acp',str(control),str(folder),sys.executable,str(repo/'scripts/acp-fixture.py')],stdout=subprocess.DEVNULL,stderr=out,start_new_session=True)
(root/'pid').write_text(str(p.pid))
end=time.monotonic()+15
while not control.exists() or 'ready:' not in log.read_text():
 assert p.poll() is None,log.read_text()
 assert time.monotonic()<end
 time.sleep(.025)
def call(*args):return json.loads(subprocess.check_output([str(binary),*map(str,args)],text=True,timeout=15))
thread=call('session',control,'--take-control','create',folder)['result']['thread']['id']
call('session',control,'--take-control','send',thread,'[permission]')
config=root/'hosts.json';config.write_text(json.dumps({'version':1,'hosts':[{'name':'Text fixture','socket':str(control)}]}))
ws=call('workspace',config.with_suffix('.creations'),'add',config,'Text fixture','folder',folder)
(root/'workspace.json').write_text(json.dumps(ws))
while True:
 with socket.socket(socket.AF_UNIX) as s:
  s.connect(str(control));s.sendall(b'{"command":"snapshot"}\n');snap=json.loads(s.makefile().readline())['data']
 if snap['pending_requests']:break
 assert time.monotonic()<end;time.sleep(.025)
(root/'before.json').write_text(json.dumps(snap,indent=2))
print(json.dumps({'workspace':ws['id'],'thread':thread,'pid':p.pid,'source_bytes':len(source.encode())}))
