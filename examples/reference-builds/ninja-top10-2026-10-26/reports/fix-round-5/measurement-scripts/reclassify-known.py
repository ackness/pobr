"""Reclassify only newly documented rows from saved engine/oracle files."""
import importlib.util,json
from pathlib import Path
root=Path(__file__).resolve().parents[2];cache=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('triage_saved',root/'devs/scripts/triage.py');t=importlib.util.module_from_spec(spec);spec.loader.exec_module(t)
p=json.loads((cache/'after.json').read_text());entries=json.loads((cache/'added-known-entries.json').read_text());known=t.load_known();commit=json.loads((cache/'after-measurement-inputs.json').read_text())['vendor_commit'];changed=[]
for index,row in enumerate(p['results']):
 if not any(e['build'] in row['build'] for e in entries):continue
 report=json.loads((cache/'after-builds'/f"{row['build']}.json").read_text());code=(root/row['set']/row['build']/'code.txt').read_text();xml=t.compare.decode_code(code)
 panels={x['stat']:x for x in report['breakdowns']};names=t.compare.oracle_names([r['stat'] for r in report['reference']['differing']],panels);oracle_path=t.compare.oracle_cache_path(code,names,commit);assert oracle_path.is_file(),oracle_path
 after=t.analyse(row['build'],row['set'],report,json.loads(oracle_path.read_text()),xml,known)
 for before,new in zip(row['rows'],after['rows']):
  assert all(before[k]==new[k] for k in ['stat','pobr','code','pob2'])
  if before['class']!=new['class']:changed.append({'build':row['build'],'stat':new['stat'],'before':before['class'],'after':new['class']})
 p['results'][index]=after
r=p['results'];p.update(leads=[{'kind':k[0],'type':k[1],'key':k[2],**e} for k,e in t.gather_leads(r)],unsupported=[{'template':k,**e} for k,e in t.gather_unsupported(r)],unmodelled=[{'template':k,**e} for k,e in t.gather_lines(r,'unmodelled')],no_modifier=[{'template':k,**e} for k,e in t.gather_lines(r,'no_modifier')],unread=[{'name':k,**e} for k,e in t.gather_unread(r)],conditions=[{'condition':k,**e} for k,e in t.gather_conditions(r)],gem_stats=[{'owner':k[0],'stat':k[1],**e} for k,e in t.gather_gem_stats(r)])
(cache/'after-classified.json').write_text(json.dumps(p,indent=1,default=t.to_json)+'\n');(cache/'after-classified.log').write_text(t.render(r,p['failures'],commit,60));(cache/'known-after-validation.json').write_text(json.dumps({'mode':'saved-file reclassification; no engine or oracle execution','changed_rows':changed,'numeric_values_unchanged':True},indent=2)+'\n');print(json.dumps(changed))
