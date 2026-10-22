import json, sqlite3, socket, sys, hashlib
from pathlib import Path
root=Path(__file__).resolve().parent
setup=json.loads((root/'setup.json').read_text())
def snapshots():
 result=[]
 for host in setup['hosts']:
  with socket.socket(socket.AF_UNIX) as stream:
   stream.connect(host['socket']);stream.sendall(b'{"command":"snapshot"}\n')
   result.append(json.loads(stream.makefile().readline())['data'])
 return result
def read_state():
 with sqlite3.connect(Path(setup['store'])/'creations.sqlite3') as db:
  tables=['drafts','workspace_names','workspace_pins','workspace_layout']
  return {table:db.execute('SELECT * FROM '+table+' ORDER BY 1').fetchall() for table in tables}
def digest(value):return hashlib.sha256(json.dumps(value,sort_keys=True).encode()).hexdigest()
a=sys.argv[1]
if a=='seed':
 hosts=snapshots();tabs=[]
 for i,w in enumerate(setup['workspaces']):
  registration=w;d=registration['directory']
  target=None if i==2 else {'host_id':hosts[i]['host_id'],'thread_id':setup['threads'][i],'cwd':d['path'],'archived':False}
  tabs.append({'workspace':w['id'],'socket':registration['endpoint']['socket'],'directory':d['path'],'device':d['device'],'inode':d['inode'],'session':target,'panel':'conversation','files':{'directory':'','file':None},'terminal':None,'sidebar_width':320,'sidebar_visible':True})
 with sqlite3.connect(Path(setup['store'])/'creations.sqlite3') as db:
  db.execute('INSERT INTO workspace_layout(singleton,revision,data) VALUES(1,1,?)',(json.dumps({'tabs':tabs,'selected':tabs[0]['workspace'],'visible':True}),))
elif a in ['refuse','allow']:
 with sqlite3.connect(Path(setup['store'])/'creations.sqlite3') as db:
  db.executescript("CREATE TRIGGER refuse_order BEFORE INSERT ON workspace_pins BEGIN SELECT RAISE(ABORT,'owned native order refusal');END;" if a=='refuse' else 'DROP TRIGGER refuse_order;')
else:
 state=read_state();hosts=snapshots();order=[v[1] for v in reversed(state['workspace_pins'])]
 receipt={'state':state,'hosts':hosts}
 (root/(a+'.json')).write_text(json.dumps(receipt,ensure_ascii=False,indent=2))
 print(json.dumps({'capture':a,'pins':order,'drafts_hash':digest(state['drafts']),'names_hash':digest(state['workspace_names']),'hosts_hash':digest(hosts)},ensure_ascii=False))
