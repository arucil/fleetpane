import json,sys,pathlib,subprocess,hashlib
ROOT=pathlib.Path('/tmp/fp-history-2351-next');SHAS=json.load(open('/tmp/fp-pinned-shas.json'));REPO='/home/amlo/Projects/paseo-arucil'
o=int(sys.argv[1]);path=sys.argv[2];a=int(sys.argv[3]);b=int(sys.argv[4]);rev=SHAS[o-1];side=sys.argv[5] if len(sys.argv)>5 else ''
raw=subprocess.check_output(['git','-C',REPO,'show',f'{rev}{side}:{path}']);lines=raw.decode().splitlines(True);part=''.join(lines[a-1:b]);d=ROOT/'receipts'/f'{o:04d}';d.mkdir(parents=True,exist_ok=True);n=len(list(d.glob('source-*.txt')))+1;f=d/f'source-{n:02d}.txt';f.write_text(part)
m={'path':path,'revision':rev,'side':side,'lines':[a,min(b,len(lines))],'file_blob_digest':'sha256:'+hashlib.sha256(raw).hexdigest(),'source_receipt_digest':'sha256:'+hashlib.sha256(part.encode()).hexdigest(),'receipt_file':str(f.relative_to(ROOT))};(d/f'source-{n:02d}.json').write_text(json.dumps(m,indent=2)+'\n');print(json.dumps(m));print(part)
