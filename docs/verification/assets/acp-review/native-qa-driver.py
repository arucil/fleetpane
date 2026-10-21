import json,os,signal,socket,subprocess,sys,time
from pathlib import Path
ROOT=Path('/tmp/fleetpane-acp-review-qa');REPO=Path('/home/amlo/Projects/Rust/fleetpane')
HOST=REPO/'target/debug/fleetpane-host';CONTROL=ROOT/'private/control.sock';STATE=ROOT/'identity.json'
def wait(probe):
 end=time.monotonic()+12
 while True:
  v=probe()
  if v:return v
  assert time.monotonic()<end,'QA observation timed out'
  time.sleep(.025)
def command(*args):return json.loads(subprocess.run([str(HOST),'session',str(CONTROL),'--take-control',*args],check=True,capture_output=True,text=True,timeout=12).stdout)['result']
def snapshot():
 with socket.socket(socket.AF_UNIX) as s:
  s.connect(str(CONTROL));s.sendall(b'{"command":"snapshot"}\n')
  return json.loads(s.makefile().readline())['data']
def read():
 with socket.socket(socket.AF_UNIX) as stream:
  stream.connect(str(CONTROL));stream.sendall(json.dumps({'command':'query','lease':snapshot()['owner'],'method':'thread/read','params':{'threadId':json.loads(STATE.read_text())['id'],'includeTurns':True}}).encode()+b'\n')
  return json.loads(stream.makefile().readline())['data']['message']['result']['thread']
def start():
 with (ROOT/'host.log').open('w') as f:
  p=subprocess.Popen([str(HOST),'serve-acp',str(CONTROL),str(ROOT),'python3',str(REPO/'scripts/acp-fixture.py')],stdout=subprocess.DEVNULL,stderr=f,start_new_session=True)
 (ROOT/'host.pid').write_text(str(p.pid));wait(lambda:CONTROL.exists() and 'ready:' in (ROOT/'host.log').read_text())
def stop():
 pid=int((ROOT/'host.pid').read_text());args=Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0')
 assert bytes(CONTROL) in args and b'serve-acp' in args and os.getpgid(pid)==pid
 os.killpg(pid,signal.SIGINT);wait(lambda:not CONTROL.exists())
action=sys.argv[1]
if action=='init':
 ROOT.mkdir();start()
 identity=command('create',str(ROOT))['thread']['id'];STATE.write_text(json.dumps({'id':identity}))
 command('send',identity,'[permission-review]')
 wait(lambda:len(snapshot()['pending_requests'])==1)
elif action=='other':
 other_dir=ROOT/'other-workspace';other_dir.mkdir()
 state=json.loads(STATE.read_text());state['other']=command('create',str(other_dir))['thread']['id'];STATE.write_text(json.dumps(state))
 command('send',state['other'],'[permission]');wait(lambda:len(snapshot()['pending_requests'])==2)
elif action=='settle':
 identity=json.loads(STATE.read_text())['id']
 assert (ROOT/f'cancel-{identity}-answered.json').exists()
 (ROOT/f'cancel-{identity}-settle').touch()
 wait(lambda:read()['turns'][-1]['status']=='interrupted')
elif action=='restart':stop();start()
elif action=='stop':stop();sys.exit()
elif action!='read':raise ValueError(action)
t=read();s=snapshot();(ROOT/f'{action}-receipt.json').write_text(json.dumps({'thread':t,'snapshot':s},indent=2,ensure_ascii=False))
print(json.dumps({'host':s['host_id'],'status':t['status'],'turns':len(t['turns']),'items':len(t['turns'][0]['items'])}))
