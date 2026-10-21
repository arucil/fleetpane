import json,hashlib,pathlib,subprocess,re
R=pathlib.Path(__file__).resolve().parent;REPO='/home/amlo/Projects/paseo-arucil';P=pathlib.Path('/tmp/fp-pinned-shas.json');pins=json.loads(P.read_text());rows=json.loads((R/'review.json').read_text());errors=[];blobs={};selected=0;supplements=0;merges=0

def git(*args):return subprocess.check_output(['git','-C',REPO,*args])
def digest(b):return 'sha256:'+hashlib.sha256(b).hexdigest()
def check(ok,msg):
 if not ok:errors.append(msg)
check([x['ordinal'] for x in rows]==list(range(1401,1651)),'ordinal continuity')
for x in rows:
 o=x['ordinal'];sha=x['revision'];check(sha==pins[o-1],f'{o} pin mismatch');check(bool(re.fullmatch('[0-9a-f]{40}',sha)),f'{o} full SHA');d=R/'receipts'/f'{o:04d}';m=json.loads((d/'metadata.json').read_text());parents=git('show','-s','--format=%P',sha).decode().split();merge=len(parents)>1;merges+=merge;cmp=[parents[0],sha] if merge else [sha]
 check(parents==x['diff_tree_receipt']['parents'],f'{o} parents')
 tree=git('diff-tree','--root','--no-commit-id','--name-status','--no-renames','-r',*cmp)
 check(tree==(d/'paths.txt').read_bytes(),f'{o} paths source');check(digest(tree)==x['diff_tree_receipt']['sha256'],f'{o} paths digest');check(x['diff_tree_receipt']['basis']==('first-parent' if merge else 'parent'),f'{o} comparison')
 raw=(git('diff','--no-ext-diff','--no-renames','--unified=2',*cmp) if merge else git('show','--format=','--root','--no-ext-diff','--no-renames','--unified=2',sha)).decode()
 blocks={}
 for block in re.split(r'(?=^diff --git )',raw,flags=re.M):
  if not block:continue
  match=re.match(r'diff --git a/(.*?) b/(.*?)\n',block)
  if match:blocks[match[2]]=block.splitlines(True)
 reconstructed=''
 for z in m['selected_hunk_ranges']:
  a,b=z['diff_block_lines'];chunk=''.join(blocks[z['path']][a-1:b]);reconstructed+=f"FILE {z['path']}\nRANGE {json.dumps(z,separators=(',',':'))}\n"+chunk
 check(reconstructed==(d/'selected.patch').read_text(),f'{o} selected source reconstruction');selected+=1
 for receipt in x['source_receipts']:
  b=(R/receipt['file']).read_bytes();check(digest(b)==receipt['sha256'],f'{o} receipt digest {receipt["file"]}')
 for f in sorted(d.glob('source-*.json')):
  z=json.loads(f.read_text());spec=z['revision']+z['side']+':'+z['path'];blob=blobs.get(spec)
  if blob is None:blob=git('show',spec);blobs[spec]=blob
  a,b=z['lines'];expected=''.join(blob.decode().splitlines(True)[a-1:b]).encode();check(expected==(R/z['receipt_file']).read_bytes(),f'{o} supplemental range {f.name}');check(digest(blob)==z['file_blob_digest'],f'{o} blob digest');check(z['revision']==sha,f'{o} supplemental revision');supplements+=1
result={'review_entries':len(rows),'first_ordinal':1401,'last_ordinal':1650,'first_revision':rows[0]['revision'],'last_revision':rows[-1]['revision'],'next_ordinal':1651,'next_revision':pins[1650],'first_parent_merge_entries':merges,'selected_patch_receipts_checked':selected,'supplemental_source_receipts_checked':supplements,'pinned_index_sha256':digest(P.read_bytes()),'review_json_sha256':digest((R/'review.json').read_bytes()),'checks':['full SHA/index and contiguous ordinals','parents and first-parent merge comparison basis','git diff-tree raw paths and SHA256','selected patch reconstructed from indexed git source diff ranges','supplemental source exact blob/range and SHA256'],'errors':errors,'tests_executed':False,'scope':'Receipt integrity validation only; not historical application tests or full source audit.'}
(R/'validation.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2));raise SystemExit(bool(errors))
