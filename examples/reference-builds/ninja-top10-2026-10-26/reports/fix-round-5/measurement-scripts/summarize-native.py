"""Summarize the completed CLI measurements; never recalculate builds."""
import base64,collections,csv,hashlib,json,subprocess,zlib
from datetime import datetime,timezone
from pathlib import Path
import xml.etree.ElementTree as ET
import sys
phase=sys.argv[1] if len(sys.argv)>1 else 'after'
root=Path(__file__).resolve().parents[2];cache=Path(__file__).resolve().parent
source=root/'examples/reference-builds/ninja-top10-2026-10-26'
out=source/'reports/fix-round-5';out.mkdir(exist_ok=True)
triage=json.loads((cache/f'{phase}.json').read_text())
assert len(triage['results'])==307 and not triage['failures']
inputs=json.loads((source/'reports/post-fix-230/inputs.json').read_text())
inputs.update(started_at=json.loads((cache/f'{phase}-measurement-inputs.json').read_text())['started_at'],git_commit=json.loads((cache/f'{phase}-measurement-inputs.json').read_text())['git_commit'],command=f'POBR_BIN=target/debug/pobr python3 .cache/accuracy-round5-2026-10-26/run-triage-save-{phase}.py examples/wasm-benchmark/builds examples/reference-builds/tuned/builds examples/reference-builds/ninja-top10-2026-10-26/builds --jobs 4 --top 60 --json .cache/accuracy-round5-2026-10-26/{phase}.json')
provenance=None
records=[]; differences=[];counts=collections.Counter(); per_class={}
for row in inputs['builds']:
 name=row['name'];code=(root/row['source']).read_text().strip()
 assert hashlib.sha256(code.encode()).hexdigest()==row['code_sha256'],name
 raw_xml=zlib.decompress(base64.urlsafe_b64decode(code+'='*(-len(code)%4)))
 xml=ET.fromstring(raw_xml)
 row['xml_sha256']=hashlib.sha256(raw_xml).hexdigest()
 d=json.loads((cache/f'{phase}-builds'/f'{name}.json').read_text());ref=d['reference']
 if provenance is None:provenance=d['provenance']
 assert d['provenance']==provenance,name
 assert ref['compared']>0 and len(ref['differing'])==ref['compared']-ref['agreeing']
 assert d['reference_basis'] and ref['tolerance']==.05
 assert not d['missing_items'],name
 for damage_type, terms in (d['output'].get('ehp') or {}).get('by_type',{}).items():
  pool=terms.get('total_hit_pool')
  assert pool is None or pool>=0,(name,damage_type,pool)
 meta=json.loads((source/'builds'/name/'meta.json').read_text())
 minion_ref=next((float(e.attrib['value']) for e in xml.findall('.//MinionStat') if e.get('stat')=='TotalDPS'),0)
 stat='MinionTotalDPS' if minion_ref else 'TotalDPS'
 record={'build':name,'ascendancy':row['ascendancy'],'main_skill':d['main_skill'],'compared_rows':ref['compared'],'agreed_rows':ref['agreeing'],'damage_stat':stat,'damage_agrees':not any(r['stat']==stat for r in ref['differing']),'unsupported_modifier_lines':len(d['unsupported_modifiers']),'missing_items':d['missing_items'],'differences':ref['differing']}
 records.append(record)
 group=per_class.setdefault(row['ascendancy'],{'builds':0,'compared_rows':0,'agreed_rows':0,'damage_agrees':0})
 for key in ['compared_rows','agreed_rows','damage_agrees']:group[key]+=record[key]
 group['builds']+=1
 for r in ref['differing']:
  counts[r['stat']]+=1;differences.append({'build':name,'ascendancy':row['ascendancy'],**r})
assert len(records)==230
summary={'git_commit':inputs['git_commit'],'data_version':inputs['data_version'],'source_snapshot':inputs['source_snapshot'],'builds_imported_and_calculated':230,'failed_imports_or_calculations':0,'missing_items_reported':0,'negative_typed_hit_pools':0,'agreed_rows':sum(r['agreed_rows'] for r in records),'compared_rows':sum(r['compared_rows'] for r in records),'builds_with_agreeing_damage':sum(r['damage_agrees'] for r in records),'builds_with_all_compared_rows_agreeing':sum(r['agreed_rows']==r['compared_rows'] for r in records),'unsupported_line_occurrences':sum(r['unsupported_modifier_lines'] for r in records),'builds_with_unsupported_lines':sum(r['unsupported_modifier_lines']>0 for r in records),'per_ascendancy':per_class,'most_frequent_differences':dict(counts.most_common()),'tolerance':.05,'measurement':'Native CLI --reference-basis --usage with standard panel and scoped recovery/cost/capacity breakdowns, through the repository triage; serialized native values. Same reference-check library as the accuracy dashboard. No second calculation run for this summary.','limits':['Saved exporter values and pinned PoB2 are cross-checks, not game truth.','FullDPS is not calculated; damage uses selected main minion TotalDPS when its saved reference is nonzero.','Exporter-absent rules and timing models are disabled for reference comparison; focused native regressions cover the added rules.','Runtime errors fail triage, but numerical disagreement is reported rather than treated as a test failure.','No individual holdout inspected.','Structural audit of this frozen corpus is separate; multi-set/historical/disabled/malformed import cases were not present in that corpus.'],'builds':records}
inputs['reproduction_command']='mise run triage examples/wasm-benchmark/builds examples/reference-builds/tuned/builds examples/reference-builds/ninja-top10-2026-10-26/builds --jobs 4 --json /tmp/pobr-fix-round-5.json'
inputs['provenance']=provenance
summary['provenance']=provenance
summary['differing_rows']=summary['compared_rows']-summary['agreed_rows']
summary['row_agreement_percent']=round(100*summary['agreed_rows']/summary['compared_rows'],2)
summary['damage_agreement_percent']=round(100*summary['builds_with_agreeing_damage']/230,2)
for file,value in [('inputs.json',inputs),('native-summary.json',summary)]:
 (out/file).write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n')
with (out/'differences.csv').open('w',encoding='utf-8-sig',newline='') as f:
 keys=list(dict.fromkeys(k for row in differences for k in row));w=csv.DictWriter(f,fieldnames=keys,lineterminator="\n");w.writeheader();w.writerows(differences)
print(json.dumps({k:v for k,v in summary.items() if k not in ['builds','limits','per_ascendancy','most_frequent_differences']},ensure_ascii=False,indent=2))
