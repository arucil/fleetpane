import socket,json,time,threading
from pathlib import Path
r=Path('/tmp/fp-next-request');p=r/'gate.sock';s=socket.socket(socket.AF_UNIX);s.bind(str(p));s.listen();counter=0
snapshot=json.loads((r/'baseline.json').read_text())['DeepSeek-B'];snapshot['pending_requests']=[];snapshot['activity']['turns']=[]
def run(c,n):
 with c:
  request=json.loads(c.makefile().readline());assert request=={'command':'snapshot'},request
  if (r/'hold-gate').exists():
   (r/f'gate-wait-{n}').write_text('ready')
   deadline=time.monotonic()+15
   while not (r/f'gate-release-{n}').exists():
    if time.monotonic()>deadline:raise TimeoutError('owned gate expired')
    time.sleep(.01)
  c.sendall(json.dumps({'response':'snapshot','data':snapshot}).encode()+b'\n')
  (r/f'gate-done-{n}').write_text('sent')
while True:
 c,_=s.accept();counter+=1;threading.Thread(target=run,args=(c,counter),daemon=True).start()
