from pathlib import Path
import hashlib,json,socket,sqlite3,sys
r=Path('/tmp/fp-alert-policy');s=json.loads((r/'setup.json').read_text());baseline=json.loads((r/'baseline.json').read_text());snapshots={}
for h in s['hosts']:
 with socket.socket(socket.AF_UNIX) as c:
  c.connect(h['socket']);c.sendall(b'{"command":"snapshot"}\n');snapshots[h['name']]=json.loads(c.makefile().readline())['data']
 for key in ['host_id','host_pid','provider_pid','operation_ids','journal_total','pending_requests','attention','activity','admission']:
  assert snapshots[h['name']][key]==baseline[h['name']][key],(h['name'],key)
with sqlite3.connect('file:'+str(r/'preferences/creations.sqlite3')+'?mode=ro',uri=True) as db:
 revision,data=db.execute('SELECT revision,data FROM notification_preferences').fetchone(); policy={'revision':revision,'value':json.loads(data)}
with sqlite3.connect('file:'+str(r/'hosts.creations/creations.sqlite3')+'?mode=ro',uri=True) as db:
 drafts=[json.loads(row[0]) for row in db.execute('SELECT data FROM drafts')];reviews=db.execute('SELECT count(*) FROM attention_reviews').fetchone()[0]
assert reviews==0
record={'snapshots':snapshots,'policy':policy,'drafts':drafts,'reviews':reviews,'desktop_sha256':hashlib.sha256(Path('/home/amlo/Projects/Rust/fleetpane/apps/desktop/target/debug/fleetpane-desktop').read_bytes()).hexdigest()}
if len(sys.argv)>1:(r/(sys.argv[1]+'.json')).write_text(json.dumps(record,ensure_ascii=False,indent=2))
print(json.dumps({'policy':policy,'pending':{n:len(s['pending_requests']) for n,s in snapshots.items()},'results':{n:len(s['attention']['records']) for n,s in snapshots.items()},'drafts':[{'text':d['text'],'revision':d['revision']} for d in drafts],'reviews':reviews,'desktop_sha256':record['desktop_sha256']},ensure_ascii=False))
