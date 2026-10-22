import json,sqlite3,subprocess,sys
from pathlib import Path
root=Path(__file__).resolve().parent
s=json.loads((root/'setup.json').read_text())
binary='/home/amlo/Projects/Rust/fleetpane/target/debug/fleetpane-host'
a=sys.argv[1]
def call(*args):
 p=subprocess.run([binary,'workspace',s['store'],*args],text=True,capture_output=True,timeout=15)
 assert p.returncode==0,(args,p.stderr)
 return json.loads(p.stdout)
with sqlite3.connect(Path(s['store'])/'creations.sqlite3') as db:
 if a=='refuse-pins': db.execute("CREATE TRIGGER refuse_pins BEFORE DELETE ON workspace_pins BEGIN SELECT RAISE(ABORT,'owned pin refusal'); END")
 elif a=='allow-pins': db.execute('DROP TRIGGER refuse_pins')
 elif a=='corrupt': db.execute('UPDATE project_names SET value=? WHERE project=?',('invalid\nname',s['projects']['main']['project_id']))
 elif a=='repair': db.execute('UPDATE project_names SET value=? WHERE project=?',('同步项目 🚀',s['projects']['main']['project_id']))
 elif a=='reverse':
  pins=call('pins');expected=[w['id'] for w in pins]
  print(json.dumps(call('reorder-pins',json.dumps(expected),json.dumps(expected[::-1])),ensure_ascii=False))
 elif a in ['unpin','pin']:
  print(json.dumps(call('pin',s['projects']['main']['id'],'off' if a=='unpin' else 'on'),ensure_ascii=False))
 else: raise ValueError(a)
