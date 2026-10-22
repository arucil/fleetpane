from pathlib import Path
import socket,json,sys,sqlite3
r=Path('/tmp/fp-next-request');s=json.loads((r/'setup.json').read_text())
def snap(endpoint):
 with socket.socket(socket.AF_UNIX) as c:
  c.connect(endpoint);c.sendall(b'{"command":"snapshot"}\n');return json.loads(c.makefile().readline())['data']
if __name__=='__main__':
 states={h['name']:snap(h['socket']) for h in s['hosts']}
 (r/(sys.argv[1]+'.json')).write_text(json.dumps(states,indent=2))
 print({k:{f:v[f] for f in ['host_pid','provider_pid','owner','journal_total']} for k,v in states.items()})
 with sqlite3.connect(r/'hosts.creations/creations.sqlite3') as db:
  print(db.execute("SELECT name,sql FROM sqlite_master WHERE name in ('drafts','workspace_layout')").fetchall())
