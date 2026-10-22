import json, subprocess, re, hashlib, pathlib, sys, os
ROOT=pathlib.Path('/tmp/fp-history-2351-next'); REPO='/home/amlo/Projects/paseo-arucil'
SHAS=json.load(open('/tmp/fp-pinned-shas.json'))
def git(*args):return subprocess.check_output(['git','-C',REPO,*args],text=True)
def substantive(line):
 s=line[1:].strip()
 return line.startswith(('+','-')) and not line.startswith(('+++','---')) and bool(s) and not re.match(r'^(import\b|from\b|//|/\*|\*|#|[{}\[\]();,]+$|[A-Za-z_$][\w$]*,?$|type [A-Za-z_$][\w$]*,?$)' ,s)
def priority(path):
 if path.endswith(('AGENTS.md','CLAUDE.md')): return -100
 if any(x in path for x in ('lock.json','yarn.lock','pnpm-lock','.svg','.png','.jpg','.snap')):return -30
 if re.search(r'\.(tsx?|rs|py|js|sh|swift)$',path):
  return 50 - (15 if any(x in path.lower() for x in ('test','spec','script','e2e','poc-')) else 0)
 if path.endswith(('.json','.yml','.yaml','.toml')):return 15
 return 0
for ordinal in range(int(sys.argv[1]),int(sys.argv[2])+1):
 sha=SHAS[ordinal-1]
 if os.environ.get('FP_MERGE_ONLY') and len(git('show','-s','--format=%P',sha).split())<2:continue
 d=ROOT/'receipts'/f'{ordinal:04d}';d.mkdir(parents=True,exist_ok=True)
 meta=git('show','-s','--format=%H%n%aI%n%cI%n%P%n%s',sha).splitlines()
 comparison=([meta[3].split()[0],sha] if len(meta[3].split())>1 else [sha])
 paths=git('diff-tree','--root','--no-commit-id','--name-status','--no-renames','-r',*comparison)
 raw=(git('diff','--no-ext-diff','--no-renames','--unified=2',*comparison) if len(comparison)>1 else git('show','--format=','--root','--no-ext-diff','--no-renames','--unified=2',sha))
 (d/'paths.txt').write_text(paths)
 files=[]
 for block in re.split(r'(?=^diff --git )',raw,flags=re.M):
  if not block:continue
  path=re.match(r'diff --git a/(.*?) b/(.*?)\n',block).group(2)
  lines=block.splitlines(True)
  starts=[i for i,l in enumerate(lines) if l.startswith('@@ ')]
  hunks=[]
  for j,st in enumerate(starts):
   end=starts[j+1] if j+1<len(starts) else len(lines)
   hl=lines[st:end];count=sum(substantive(l) for l in hl)
   hunks.append((st,hl,count))
  files.append((path,lines,hunks))
 candidates=[]
 for fi,(path,lines,hunks) in enumerate(files):
  for hi,(st,hl,count) in enumerate(hunks):
   if count:candidates.append((priority(path) - (12 if not any(l.startswith("+") and substantive(l) for l in hl) else 0) - (8 if "StyleSheet.create" in hl[0] else 0),count,fi,hi))
 # Select one initial coherent implementation hunk; follow up on workflows requiring more context. No line-count floor.
 candidates.sort(key=lambda x:(x[0], min(x[1],30)),reverse=True)
 selected=[];total=0
 for pri,count,fi,hi in candidates:
  if selected: break
  path,lines,hunks=files[fi]; st,hl,n=hunks[hi]
  # For large hunks take a contiguous region around substantive changes, bounded to 11 lines in later batches (the initial 1001-1010 batch used 21).
  if len(hl)>12:
   inds=[i for i,l in enumerate(hl[1:],1) if substantive(l)]
   logic=[i for i,l in enumerate(hl[1:],1) if re.search(r"(?:export )?(?:async )?function |(?:async )?\w+\([^;]*\)[: {]|=> \{",l[1:]) and substantive(l)]
   a=max(1,(logic[0] if logic else inds[0])-1); b=min(len(hl),a+30)
   chunk=hl[a:b]; start=st+a
  else:chunk=hl;start=st
  sub=sum(substantive(l) for l in chunk)
  selected.append((fi,hi,start,chunk));total+=sub
 if not selected:
  # Metadata/binary-only: retain file headers, small config/docs if any.
  for fi,(path,lines,hunks) in enumerate(sorted(files,key=lambda f:priority(f[0]),reverse=True)[:2]):
   selected.append((files.index((path,lines,hunks)),-1,0,lines[:35]))
 # A receipt is exactly the displayed selected patch text; line ranges refer to original file and diff-block.
 receipt='';ranges=[]
 for fi,hi,start,chunk in sorted(selected):
  path,lines,hunks=files[fi]
  old=new=0
  if hi>=0:
   st,hl,count=hunks[hi]; m=re.match(r'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@',hl[0]);old=int(m[1]);new=int(m[3])
   for l in lines[st+1:start]:
    if not l.startswith('+'):old+=1
    if not l.startswith('-'):new+=1
  old_start=old;ns=new
  for l in chunk:
   if l.startswith(('@@','\\')):continue
   if l.startswith((' ','-')) and not l.startswith('---'):old+=1
   if l.startswith((' ','+')) and not l.startswith('+++'):new+=1
  ranges.append({'path':path,'old_lines':[old_start,max(old_start,old-1)] if hi>=0 else None,'new_lines':[ns,max(ns,new-1)] if hi>=0 else None,'diff_block_lines':[start+1,start+len(chunk)],'hunk_header':hunks[hi][1][0].strip() if hi>=0 else None,'substantive_changed_lines':sum(substantive(l) for l in chunk)})
  receipt+=f'FILE {path}\nRANGE {json.dumps(ranges[-1],separators=(",",":"))}\n'+''.join(chunk)
 (d/'selected.patch').write_text(receipt)
 data={'comparison_basis':'first-parent' if len(comparison)>1 else 'parent','ordinal':ordinal,'revision':sha,'date':meta[1],'committer_date':meta[2],'parents':meta[3].split(),'subject':meta[4], 'diff_tree_paths':paths.splitlines(),'source_receipt_digest':'sha256:'+hashlib.sha256(receipt.encode()).hexdigest(),'diff_tree_digest':'sha256:'+hashlib.sha256(paths.encode()).hexdigest(),'selected_hunk_ranges':ranges,'selected_substantive_changed_lines':total,'available_substantive_changed_lines':sum(h[2] for _,_,hs in files for h in hs),'receipt_file':str((d/'selected.patch').relative_to(ROOT))}
 (d/'metadata.json').write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n')
 print(f'\n=== {ordinal} {sha} {meta[4]} ===')
 print('PATHS', '; '.join(paths.splitlines()[:6]), f'(+{max(0,len(paths.splitlines())-6)} more)' if len(paths.splitlines())>6 else '')
 for r in ranges:print('RANGE',r['path'],r['old_lines'],r['new_lines'])
 print(''.join(l for l in receipt.splitlines(True) if not l.startswith(('FILE ','RANGE '))),end='')
