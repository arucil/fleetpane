import json, os, signal, socket, subprocess, sys, time
from pathlib import Path
ROOT=Path('/tmp/fleetpane-acp-info-qa')
REPO=Path('/home/amlo/Projects/Rust/fleetpane')
HOST=REPO/'target/debug/fleetpane-host'
CONTROL=ROOT/'private/control.sock'
STATE=ROOT/'identity.json'
INBOX=ROOT/'metadata.jsonl'
def wait(probe):
 end=time.monotonic()+12
 while True:
  result=probe()
  if result:return result
  assert time.monotonic()<end,'QA observation timeout'
  time.sleep(.025)
def command(*args):
 return json.loads(subprocess.run([str(HOST),'session',str(CONTROL),'--take-control',*args],check=True,capture_output=True,text=True).stdout)['result']
def snapshot():
 with socket.socket(socket.AF_UNIX) as s:
  s.connect(str(CONTROL));s.sendall(b'{"command":"snapshot"}\n')
  return json.loads(s.makefile().readline())['data']
def start():
 log=ROOT/'host.log'
 with log.open('w') as f:
  p=subprocess.Popen([str(HOST),'serve-acp',str(CONTROL),str(ROOT),'python3',str(REPO/'scripts/acp-fixture.py')],stdout=subprocess.DEVNULL,stderr=f,start_new_session=True,env={**os.environ,'FLEETPANE_FIXTURE_METADATA_INBOX':str(INBOX)})
 (ROOT/'host.pid').write_text(str(p.pid))
 wait(lambda: CONTROL.exists() and 'ready:' in log.read_text())
def stop():
 pid=int((ROOT/'host.pid').read_text())
 args=Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0')
 assert bytes(CONTROL) in args and b'serve-acp' in args and os.getpgid(pid)==pid
 os.killpg(pid,signal.SIGINT)
 wait(lambda: not CONTROL.exists())
def patch(title):
 info={'sessionUpdate':'session_info_update','title':title,'updatedAt':'2026-10-21T09:08:07+08:00','_meta':{'qa':[True,None,'中文']}}
 with INBOX.open('a') as f:f.write(json.dumps({'sessionId':json.loads(STATE.read_text())['id'],'update':info})+'\n')
def read():return command('open',json.loads(STATE.read_text())['id'])['thread']
action=sys.argv[1]
if action=='init':
 ROOT.mkdir();INBOX.touch();start()
 identity=command('create',str(ROOT))['thread']['id']
 STATE.write_text(json.dumps({'id':identity}))
 patch('Initial idle 中文');wait(lambda:read()['name']=='Initial idle 中文')
elif action=='patch':
 title=None if sys.argv[2]=='NULL' else sys.argv[2]
 patch(title);wait(lambda:read()['name']==title)
elif action=='permission':
 command('send',json.loads(STATE.read_text())['id'],'[permission]');wait(lambda:snapshot()['pending_requests'])
elif action=='restart':
 stop();INBOX.write_text('');start()
elif action=='stop':stop()
elif action!='read':raise ValueError(action)
if action!='stop':
 result={'thread':read(),'snapshot':snapshot()}
 (ROOT/f'{action}-receipt.json').write_text(json.dumps(result,indent=2,ensure_ascii=False))
 print(json.dumps({'id':result['thread']['id'],'name':result['thread']['name'],'host_id':result['snapshot']['host_id'],'pending':len(result['snapshot']['pending_requests'])}))
