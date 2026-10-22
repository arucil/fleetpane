import json,sqlite3,socket,sys,hashlib
from pathlib import Path
r=Path(__file__).resolve().parent;s=json.loads((r/'setup.json').read_text());store=Path(s['store'])/'creations.sqlite3'
def snapshots():
 result=[]
 for h in s['hosts']:
  with socket.socket(socket.AF_UNIX) as c:
   c.connect(h['socket']);c.sendall(b'{"command":"snapshot"}\n');result.append(json.loads(c.makefile().readline())['data'])
 return result
def state():
 with sqlite3.connect(store) as d:
  names=[x[0] for x in d.execute("SELECT name FROM sqlite_master WHERE type='table'")]
  return {t:d.execute('SELECT * FROM '+t).fetchall() for t in ['workspace_pins','pin_audit','drafts','workspace_layout','workspace_names'] if t in names}
a=sys.argv[1]
if a=='baseline':
 (r/'baseline.json').write_text(json.dumps({'state':state(),'hosts':snapshots()}))
elif a=='audit':
 with sqlite3.connect(store) as d:
  d.executescript("CREATE TABLE pin_audit(action TEXT,workspace TEXT);CREATE TRIGGER audit_pin_insert AFTER INSERT ON workspace_pins BEGIN INSERT INTO pin_audit VALUES('insert',NEW.workspace);END;CREATE TRIGGER audit_pin_delete AFTER DELETE ON workspace_pins BEGIN INSERT INTO pin_audit VALUES('delete',OLD.workspace);END;")
 (r/'baseline.json').write_text(json.dumps({'state':state(),'hosts':snapshots()}))
elif a=='refuse':
 with sqlite3.connect(store) as d:d.executescript("CREATE TRIGGER refuse_shortcut_pin BEFORE INSERT ON workspace_pins BEGIN SELECT RAISE(ABORT,'owned shortcut pin refusal');END;CREATE TRIGGER refuse_shortcut_unpin BEFORE DELETE ON workspace_pins BEGIN SELECT RAISE(ABORT,'owned shortcut pin refusal');END;")
elif a=='allow':
 with sqlite3.connect(store) as d:d.executescript('DROP TRIGGER refuse_shortcut_pin;DROP TRIGGER refuse_shortcut_unpin;')
else:
 receipt={'state':state(),'hosts':snapshots()};(r/(a+'.json')).write_text(json.dumps(receipt,indent=2))
 with sqlite3.connect(store) as d:
  print(json.dumps({'pins':d.execute('SELECT * FROM workspace_pins').fetchall(),'actions':d.execute('SELECT * FROM pin_audit').fetchall(),'selected':json.loads(d.execute('SELECT data FROM workspace_layout').fetchone()[0])['selected'],'host_snapshots_unchanged':receipt['hosts']==json.loads((r/'baseline.json').read_text())['hosts']},indent=2))
