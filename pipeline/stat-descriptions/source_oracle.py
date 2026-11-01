#!/usr/bin/env python3
"""Independent source inventory for the ordinary global/passive/aura catalog.

This is an evidence tool, not a producer. The supported numerical/marker policy
is explicit; records that need relational names remain complete diagnostics.
Pass --candidate after Rust production to compare all target/compatibility IDs.
"""
import json,hashlib,collections,re,string,pathlib,copy
parser = __import__("argparse").ArgumentParser(description="Independent ordinary CSD ownership/compatibility inventory; never produces runtime data")
parser.add_argument("--source",type=pathlib.Path,required=True)
parser.add_argument("--legacy",type=pathlib.Path,required=True)
parser.add_argument("--out",type=pathlib.Path,required=True)
parser.add_argument("--candidate",type=pathlib.Path)
parser.add_argument("--lookup-tables",type=pathlib.Path,help="Optional pinned 9-table source; enables only the seven explicit PoE2 joins")
parser.add_argument("--diagnostic-ownership",choices=["explicit-no-description/v1"],help="Own explicit no_description as unrendered diagnostics, never as an effectless law")
parser.add_argument("--descriptor-equivalence",choices=["ordinary-plain-display/v1"],help="Compare all English fields exactly after the ordinary consumer's display-markup projection")
args=parser.parse_args()
source=args.source
args.out.mkdir(parents=True,exist_ok=False)

csd=json.loads(source.read_bytes()); old=json.loads(args.legacy.read_bytes()); enc=lambda v:(json.dumps(v,sort_keys=True,ensure_ascii=False,separators=(',',':'))+'\n').encode();sha=lambda v:hashlib.sha256(enc(v)).hexdigest()
supported={'negate','double','negate_and_double','times_twenty','multiply_by_four','add_one','subtract_one','plus_two_hundred','multiplicative_damage_modifier','divide_by_one_hundred_and_negate','divide_by_one_hundred','divide_by_one_hundred_0dp','divide_by_one_hundred_1dp','divide_by_one_hundred_2dp','divide_by_one_hundred_2dp_if_required','divide_by_ten_0dp','divide_by_ten_1dp','divide_by_ten_1dp_if_required','deciseconds_to_seconds','divide_by_two_0dp','divide_by_fifteen_0dp','divide_by_twenty_then_double_0dp','divide_by_three','divide_by_four','divide_by_five','divide_by_fifty','per_minute_to_per_second','per_minute_to_per_second_0dp','per_minute_to_per_second_1dp','per_minute_to_per_second_2dp','per_minute_to_per_second_2dp_if_required','milliseconds_to_seconds','milliseconds_to_seconds_0dp','milliseconds_to_seconds_1dp','milliseconds_to_seconds_2dp','milliseconds_to_seconds_2dp_if_required','canonical_stat'}
if args.lookup_tables:
 tables=json.loads(args.lookup_tables.read_bytes())
 required={"BaseItemTypes","ItemClasses","PassiveKeystoneList","PassiveSkills","PassiveTreeExpansionJewelSizes","SkillGems","SkillGemsForUniqueStat","UltimatumWagerTypes","UniqueMagesLegacy"}
 assert set(tables)==required
 supported.update({"passive_hash","passive_keystone_index","specific_skill","mod_value_to_item_class","ultimatum_wager_type_hash","tree_expansion_jewel_passive","mages_legacy_index"})
 def plain(text):
  text=re.sub(r'<[A-Za-z0-9_]+>\{([^{}]*)\}',r'\1',text)
  return re.sub(r'\[([^\[\]]+)\]',lambda m:m[1].split('|')[-1],text)
 lookup_values={}
 for handler,table,field,start,key_field in [
  ("mod_value_to_item_class","ItemClasses","Name",0,None),
  ("passive_keystone_index","PassiveKeystoneList","DisplayText",1,None),
  ("tree_expansion_jewel_passive","PassiveTreeExpansionJewelSizes","Name",0,None),
  ("mages_legacy_index","UniqueMagesLegacy","DisplayText",1,None),
  ("ultimatum_wager_type_hash","UltimatumWagerTypes","DisplayText",0,"HASH16"),
  ("passive_hash","PassiveSkills","Name",0,"PassiveSkillGraphId")]:
  mapping={}
  for row in tables[table]['rows']:
   raw=row['values'];key=raw[key_field] if key_field else row['_row']+start
   assert key not in mapping
   mapping[key]=plain(raw[field])
   if handler=='passive_hash' and key>=32768:mapping[key-65536]=plain(raw[field])
  lookup_values[handler]={str(k):v for k,v in mapping.items()}
 mapping={}
 for row in tables['SkillGemsForUniqueStat']['rows']:
  raw=row['values'];key=raw['Index'];assert key not in mapping
  mapping[key]=plain(' and '.join(tables['BaseItemTypes']['rows'][tables['SkillGems']['rows'][r]['values']['BaseItemType']]['values']['Name'] for r in raw['SkillGems']))
 lookup_values['specific_skill']={str(k):v for k,v in mapping.items()}


def display_plain(text):
 text=re.sub(r'<[A-Za-z0-9_]+>\{([^{}]*)\}',r'\1',text)
 return re.sub(r'\[([^\[\]]+)\]',lambda m:m[1].split('|')[-1],text)

def signature(r):
 r=copy.deepcopy(r);r.pop('raw_slice_sha256',None);r['groups']=[g for g in r['groups'] if g['language_origin']=='implicit-default' and g['declared_language'] is None]
 if args.descriptor_equivalence:
  for g in r['groups']:
   for b in g['branches']:b['template_raw']=display_plain(b['template_raw'])
 def strip(v):
  if isinstance(v,dict):return {k:strip(x) for k,x in v.items() if k not in {'span','language_declaration_span','template_character_span'}}
  if isinstance(v,list):return [strip(x) for x in v]
  return v
 return strip(r)
def owners(path):
 own=collections.defaultdict(list)
 for i,r in enumerate(csd['files'][path]['records']):
  if r['kind'] in ('description','no_description'):
   for id in r['ids']:own[id].append((r,f'/files/{path.replace("~","~0").replace("/","~1")}/records/{i}'))
 for id,rs in own.items():
  if len(rs)>1 and all(r['kind']=='description' for r,p in rs) and all(signature(rs[0][0])==signature(r) for r,p in rs[1:]):own[id]=rs[:1]
 for r in csd['files'][path]['records']:
  if r['kind']=='include':
   for id,rs in owners(r['target']).items():own.setdefault(id,rs)
 return own
compat={};stats={}; success={};diagnostics={}
for scope,legacy in old['scopes'].items():
 path=f'Data/StatDescriptions/{scope}.csd'; effective=owners(path);own=collections.defaultdict(list)
 for r in csd['files'][path]['records']:
  if r['kind'] in ('description','no_description'):
   for id in r['ids']:own[id].append(r)
 oldids=set(legacy.get('single',{}))|set(legacy.get('unrendered',[]))|{id for c in legacy.get('compound',{}).values() for id in c['member_stats']}
 completed=set();diagnostic_ids=set();reasons=collections.Counter();k=lambda id:id if scope=='stat_descriptions' else scope+'::'+id
 for id in sorted(own):
  oldids.discard(id)
  if id in completed:continue
  def check():
   rs=effective[id]
   if len(rs)!=1:return f'ambiguous CSD descriptor ownership for {id}'
   r,p=rs[0]
   if r['kind']!='description':
    rendered=id in legacy.get('single',{}) or any(id in c['member_stats'] for c in legacy.get('compound',{}).values()) or any(id in d['member_stats'] for d in legacy.get('descriptors',{}).values())
    if args.diagnostic_ownership and len(own[id])==1 and r['ids']==[id] and not rendered:
     completed.add(id);diagnostic_ids.add(id);return None
    return f'no_description for {id} is not an effectless law'
   for member in r['ids']:
    if len(effective[member])!=1 or effective[member][0][1]!=p:return f'conflicting CSD compound ownership for {member}'
   groups=[g for g in r['groups'] if g['language_origin']=='implicit-default' and g['declared_language'] is None]
   if len(groups)!=1:return 'ambiguous default CSD language'
   n=0
   for b in groups[0]['branches']:
    if b['restrictions_raw']:
     if all(r in {'table_only','gem_quality'} for r in b['restrictions_raw']):continue
     return f'unsupported CSD restrictions for {id}'
    if b['template_syntax_diagnostics']:return f'unsupported CSD syntax for {id}'
    n+=1
    for q in b['quantifiers_lexical']:
     name=q['name_raw'];handler_args=q['argument_tokens_raw']
     if name=='canonical_line' and not handler_args:continue
     if len(handler_args)!=1:return 'unsupported CSD handler arity'
     if not handler_args[0].isdigit() or not 1<=int(handler_args[0])<=len(r['ids']):return 'CSD handler index out of bounds'
     if name not in supported:return f'unsupported CSD handler {json.dumps(name)}'
    nextindex=0
    try:
     for literal,key,fmt,conv in string.Formatter().parse(b['template_raw']):
      if key is None:continue
      if key=='':idx=nextindex
      elif key.isdigit():idx=int(key)
      else:return 'invalid CSD placeholder index'
      nextindex=idx+1
      if idx>=len(r['ids']):return 'CSD placeholder out of bounds'
      if fmt not in ('','d','+d','-d'):return f'unsupported CSD placeholder format {fmt}'
    except ValueError as e:return 'unclosed CSD placeholder'
   if not n:return 'no ordinary CSD branches'
   completed.update(r['ids']);return None
  reason=check()
  if reason:
   comps={key:{'member_stats':c['member_stats'],'template':c['template']} for key,c in legacy.get('compound',{}).items() if id in c['member_stats']}
   compat[k(id)]={'scope':scope,'reason':reason,'source_records_sha256':sha(own[id]),'legacy_single':legacy.get('single',{}).get(id),'legacy_compounds':comps,'legacy_unrendered':id in legacy.get('unrendered',[])}
   reasons[reason]+=1
 for id in oldids:compat[k(id)]={'scope':scope,'reason':'absent from target CSD; legacy compatibility only'};reasons['absent from target CSD; legacy compatibility only']+=1
 stats[scope]={'migrated_ids':len(completed),'compatibility':dict(reasons)};success[scope]=sorted(completed);diagnostics[scope]=sorted(diagnostic_ids)
out=args.out;(out/'compatibility.json').write_bytes(enc(compat));(out/'source-inventory.json').write_text(json.dumps({'compatibility_sha256':sha(compat),'scopes':stats,'migrated_ids':success},indent=2)+'\n');print('binding',sha(compat));print(json.dumps(stats,indent=2))

if args.candidate:
 receipt=json.loads((args.candidate/'producer-receipt.json').read_bytes())['conversion']
 if args.lookup_tables:
  assert receipt['lookups']['values_sha256']==sha(lookup_values),"lookup values differ from exact source joins"
 assert receipt['compatibility']==compat,"producer differs from independent source inventory"
 for scope,ids in success.items():
  expected={id if scope=='stat_descriptions' else scope+'::'+id for id in ids}
  actual={key for key,v in receipt['target'].items() if v['scope']==scope}
  assert expected==actual,(scope,expected^actual)
 if args.diagnostic_ownership:
  candidate=json.loads((args.candidate/'overlay/stat_descriptions.json').read_bytes())
  for scope,ids in diagnostics.items():
   for id in ids:
    key=id if scope=='stat_descriptions' else scope+'::'+id
    assert receipt['target'][key]['ownership']=='unrendered-diagnostic'
    assert receipt['target'][key]['effectless'] is False
    assert id in candidate['scopes'][scope]['unrendered']
    assert id not in candidate['scopes'][scope].get('single',{})
    assert not any(id in d['member_stats'] for d in candidate['scopes'][scope].get('descriptors',{}).values())
 print("Producer target membership and full compatibility source bindings agree")
