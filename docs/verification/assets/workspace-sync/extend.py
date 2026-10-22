import json, subprocess, sqlite3
from pathlib import Path
root=Path(__file__).resolve().parent
setup=json.loads((root/'setup.json').read_text())
binary='/home/amlo/Projects/Rust/fleetpane/target/debug/fleetpane-host'
def w(*args):
 p=subprocess.run([binary,'workspace',setup['store'],*map(str,args)],text=True,capture_output=True,timeout=15)
 assert p.returncode==0,(args,p.stderr)
 return json.loads(p.stdout)
shared=Path(setup['workspaces'][0]['directory']['path']);linked=root/'linked 中文'
def git(*args):
 subprocess.run(['git','-C',str(shared),'-c','user.name=Fixture','-c','user.email=fixture@example.invalid','-c','core.hooksPath=/dev/null',*args],check=True,capture_output=True)
git('init','-q');git('commit','--allow-empty','-qm','fixture');git('worktree','add','-qb','linked-fixture',str(linked))
other=w('add',setup['config'],setup['hosts'][1]['name'],'git',shared)
main=w('add',setup['config'],setup['hosts'][0]['name'],'git',shared)
sibling=w('add',setup['config'],setup['hosts'][0]['name'],'git',linked)
main=w('rename',main['id'],0,'Main');sibling=w('rename',sibling['id'],0,'Feature')
assert main['project_id']==sibling['project_id']!=other['project_id']
w('pin',setup['workspaces'][0]['id'],'off');w('pin',main['id'],'on')
setup['projects']={'main':main,'linked':sibling,'other':other}
(root/'setup.json').write_text(json.dumps(setup,ensure_ascii=False,indent=2))
tabs=[]
for record,i in [(main,0),(sibling,None),(setup['workspaces'][1],1)]:
 d=record['directory'];target=None if i is None else {k:setup['draft_keys'][i][k] for k in ['host_id','thread_id','cwd']}
 if target:target['archived']=False
 tabs.append({'workspace':record['id'],'socket':record['endpoint']['socket'],'directory':d['path'],'device':d['device'],'inode':d['inode'],'session':target,'panel':'conversation','files':{'directory':'','file':None},'terminal':None,'sidebar_width':320,'sidebar_visible':True})
with sqlite3.connect(Path(setup['store'])/'creations.sqlite3') as db:
 db.execute('INSERT INTO workspace_layout(singleton,revision,data) VALUES(1,1,?)',(json.dumps({'tabs':tabs,'selected':main['id'],'visible':True}),))
print(json.dumps({'main':main['id'],'linked':sibling['id'],'project':main['project_id'],'other':other['project_id']}))
