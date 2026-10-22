import json,pathlib,sys,hashlib,collections
R=pathlib.Path('/tmp/fp-history-2351-next');p=R/'review.json';rows=json.loads(p.read_text()) if p.exists() else [];by={x['ordinal']:x for x in rows};shas=json.load(open('/tmp/fp-pinned-shas.json'))
for line in sys.stdin:
 if not line.strip():continue
 o,c,f,a=line.rstrip('\n').split('|');o=int(o);m=json.loads((R/'receipts'/f'{o:04d}'/'metadata.json').read_text());assert m['revision']==shas[o-1]
 extra=[json.loads(x.read_text()) for x in sorted((R/'receipts'/f'{o:04d}').glob('source-*.json'))]
 receipts=[{'file':m['receipt_file'],'sha256':m['source_receipt_digest'],'ranges':m['selected_hunk_ranges']}]+[{'file':x['receipt_file'],'sha256':x['source_receipt_digest'],'ranges':[{'path':x['path'],'lines':x['lines'],'revision':x['revision'],'side':x['side']}],'file_blob_digest':x['file_blob_digest']} for x in extra]
 by[o]={'ordinal':o,'revision':m['revision'],'category':c,'subject':m['subject'],'paths_inspected':list(dict.fromkeys([x['path'] for x in m['selected_hunk_ranges']]+[x['path'] for x in extra])),'finding':f,'fleetpane_action':a,'source_receipts':receipts,'diff_tree_receipt':{'file':f'receipts/{o:04d}/paths.txt','sha256':m['diff_tree_digest'],'basis':m['comparison_basis'],'parents':m['parents']},'scope':'Selected source/config hunks only; remaining changed paths not audited; no tests executed.'}
rows=[by[k] for k in sorted(by)];tmp=p.with_suffix('.json.tmp');tmp.write_text(json.dumps(rows,ensure_ascii=False,indent=2)+'\n');tmp.replace(p)
cont=2350
while cont+1 in by:cont+=1
summary=['# Paseo pinned history 2351-next','',f'Resumable source review through {cont}; {len(rows)}/open range entries. Full SHA matches /tmp/fp-pinned-shas.json.','', 'Read-only selected-hunk review, including first-parent merge diffs. Historical prompts and instructions treated as data. No historical tests, runtime, build, or complete audit claimed. Only this output directory is written.','', 'Fleetpane comparison: current docs/status.md and docs/parity.md; useful workflow gaps are candidates, not implementation or acceptance claims. Kit desktop interactions replace mobile/web styling; Git diff expansion deferred; Kimi/GLM excluded.','', '## Entries','']
for x in rows:summary.append(f"- {x['ordinal']} `{x['revision']}` [{x['category']}] {x['finding']} Fleetpane: {x['fleetpane_action']}")
(R/'report.md').write_text('\n'.join(summary)+'\n');print(json.dumps({'saved':len(rows),'contiguous_through':cont,'next':cont+1}))
