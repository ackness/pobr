"""Read retained tactician-06 inputs and reconstruct pre-S12 damage; no engine/oracle calls."""
import base64
import hashlib
import json
import math
import subprocess
from pathlib import Path
import xml.etree.ElementTree as ET
import zlib

ROOT = Path('/root/codes/pobr')
OUT = Path(__file__).resolve().parent
DATA = ROOT / 'data/0.5.5.4.2'
paths = {
    'before': ROOT / '.cache/accuracy-round4-2026-10-26/after-builds/tactician-06.json',
    'after': ROOT / '.cache/accuracy-round5-2026-10-26/pre-banner/after-builds/tactician-06.json',
    'details': OUT / 'current-details.json',
    'pinned': ROOT / '.cache/pob2-oracle/37191f90cb4dcad1-eae7b54469.json',
    'code': ROOT / 'examples/reference-builds/ninja-top10-2026-10-26/builds/tactician-06/code.txt',
}
load = lambda p: json.loads(p.read_text())
before, after, details, pinned = (load(paths[k]) for k in ('before', 'after', 'details', 'pinned'))
code = ''.join(paths['code'].read_text().split())
xml = zlib.decompress(base64.urlsafe_b64decode(code + '=' * (-len(code) % 4)))
assert xml == paths['pinned'].with_suffix('.xml').read_bytes()
build = ET.fromstring(xml)
node = next(n for n in load(DATA / 'base/passive_tree.json') if n['skill'] == 16249)
weapon_base = next(i for i in load(DATA / 'base/base_items.json') if i['name'] == 'Giant Maul')
weapon_item = next(i for i in build.findall('./Items/Item') if i.get('id') == '14')
sceptre_item = next(i for i in build.findall('./Items/Item') if i.get('id') == '21')
weapon_min, weapon_max = [weapon_base['weapon'][k] * (1 + (397 + 60) / 100) * 1.2 for k in ('physical_min','physical_max')]
shared_min, shared_max = weapon_min * .25, weapon_max * .25
by_stat = {b['stat']: b['breakdown'] for b in details['breakdowns']}
for stat, value in [('PhysicalDamageMin',shared_min), ('PhysicalDamageMax',shared_max)]:
    row = next(m for m in by_stat['Minion:'+stat]['mods'] if m.get('origin_id') == 'node.16249')
    assert abs(row['value'] - value) < 1e-12
wolf = next(m for m in load(DATA / 'overlay/minions.json')['minions'] if m['id'] == 'WolfMinion')
ally_damage = load(DATA / 'base/monster_scaling.json')['ally_damage'][38-1]
assert ally_damage == 180.29
base_avg = ally_damage * wolf['damage']
sceptre_flat = {'Physical': [14,31], 'Cold': [22,35], 'Lightning': [1,40]}
crit, crit_mult, rate, inc_mult = .0675, 2.15, 1.4375, 2.83
armour, ratio, taken = 8062.5, 10.0, 1.06
crit_effect = 1 + crit * (crit_mult - 1)
def physical_hit(raw, armour_value=armour):
    return raw * (1 - armour_value / (armour_value + ratio * raw)) * taken
def physical_average(raw):
    return (1-crit)*physical_hit(raw) + crit*physical_hit(raw*crit_mult)
# Infer only the unchanged elemental mitigation from the retained total and
# independently reconstructed physical component. This does NOT independently
# prove why PoBR's elemental resistance differs from the pinned value.
current_physical = (base_avg + 22.5*.7 + (shared_min+shared_max)/2*.7) * inc_mult * 2.29
current_elemental = 49*.7 * inc_mult * 2.29
current_effective = by_stat['Minion:TotalDPS']['formula']['steps'][1]['result']
inferred_elemental_multiplier = (current_effective - physical_average(current_physical)) / (current_elemental * crit_effect)
assert abs(inferred_elemental_multiplier - 1.06*.99) < 1e-10
# Round the inferred value to the exact one-percent-resistance equivalent for
# a reproducible arithmetic profile; retain the unrounded inference separately.
elemental_multiplier = 1.06*.99
four_way = []
for share, banner in [(False,False),(True,False),(False,True),(True,True)]:
    more = 2.29 if banner else 1.3
    physical = (base_avg + 22.5*.7 + ((shared_min+shared_max)/2*.7 if share else 0))*inc_mult*more
    elemental = 49*.7*inc_mult*more
    physical_effective = physical_average(physical)
    elemental_effective = elemental*crit_effect*elemental_multiplier
    four_way.append({'weapon_share':share,'banner_damage':banner,'kind':'analytical_counterfactual',
        'damage_more':more,'physical_noncrit_average':physical,'elemental_noncrit_average':elemental,
        'physical_effective_average':physical_effective,'elemental_effective_average':elemental_effective,
        'action_rate_held_fixed':rate,'dps':(physical_effective+elemental_effective)*rate})
values = {(r['weapon_share'],r['banner_damage']):r['dps'] for r in four_way}
observed_before, observed_after = before['output']['minions'][0]['dps'], after['output']['minions'][0]['dps']
assert abs(values[False,True]-observed_before)<2e-9
assert abs(values[True,True]-observed_after)<2e-9
mh=pinned['minionMainHand']
pinned_reconstructed=((1-crit)*physical_hit(mh['PhysicalStoredHitAvg'],8063)+crit*physical_hit(mh['PhysicalStoredCritAvg'],8063)
    +(mh['ColdStoredHitAvg']+mh['LightningStoredHitAvg'])*crit_effect*.98*taken)*rate
assert abs(pinned_reconstructed-mh['TotalDPS'])<1e-6
# Pinned weapon sharing is proven by the exact stored base ranges.
pinned_base=[]
for bound, base_bound, item_bound, weapon_bound in [('Min',100,14,642),('Max',151,31,962)]:
    shared=math.ceil(weapon_bound*.25)
    total=base_bound+(item_bound+shared)*.7
    assert abs(total-mh['Physical'+bound+'Base'])<1e-10
    pinned_base.append({'bound':bound,'wolf_floor':base_bound,'sceptre_flat':item_bound,
        'weapon_rounded':weapon_bound,'weapon_share_percent_stat_ceil':shared,'added_effectiveness':.7,
        'sum':total,'cached':mh['Physical'+bound+'Base']})
banner_native = next(x for x in load(DATA/'base/granted_effect_stat_sets.json') if x['effect_id']=='WarBannerPlayer')['sets'][0]
banner_level = next(x for x in banner_native['levels'] if x['gem_level']==18)
banner_aura = next(x for x in pinned['auraEffectList'] if x['name']=='War Banner')
banner_sums={}
for m in banner_aura['mods']:banner_sums[m['name']]=banner_sums.get(m['name'],0)+m['value']
banner_factor=(1+banner_sums['AuraEffect']/100)*(1+banner_sums['Magnitude']/100)
map_catalog=load(DATA/'overlay/skill_stat_map.json')
speed_stat='base_skill_buff_banner_attack_speed_+%_to_apply'
assert next(x['value'] for x in banner_level['stats'] if x['stat']==speed_stat)==25
assert speed_stat not in map_catalog['global']
assert speed_stat not in map_catalog['per_stat_set']['WarBannerPlayer']['1']
stat_rules=subprocess.check_output(['git','show','23e013775504fb933006c50a060298728545ccb0:crates/pobr-core/src/rules/skill_stats.rs'],cwd=ROOT,text=True)
# This receipt is deliberately tied to pre-S12 source, not a future recomputation.
assert speed_stat not in stat_rules
proof = {
 'schema':'tactician-06-s04-causal-evidence/v1','source_commit':'23e013775504fb933006c50a060298728545ccb0',
 'measurement_stage':'pre-S12; do not replace with post-fix numbers',
 'classification':'leave whole row unclassified; no S04 rollback; pinned implements S04; baseline agreement was cancellation',
 'provenance':{k:{'path':str(p),'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for k,p in paths.items()},
 'cli_receipt':{'calls':1,'binary':str(ROOT/'target/debug/pobr'),'sha256':'cc2200bf2f8b3bce9c0b145c8a6a72cd3b6e72beeba0c9483a15a7f86829e1b0','reference_basis':True,'build_code_changed':False,'oracle_runs':0},
 'observed':{'before':observed_before,'after':observed_after,'saved':1201.5931816863,'pinned':mh['TotalDPS'],
   'delta_after_minus_before':observed_after-observed_before,'gap_after_minus_pinned':observed_after-mh['TotalDPS'],
   'baseline_minus_pinned':observed_before-mh['TotalDPS']},
 'native_share_source':{'node':node,'stat_id':'aura_grant_%_base_main_hand_attack_damage_to_nearby_allies',
   'ledger_before':[x for x in before['usage']['lines'] if x.get('source_id')=='node.16249'],
   'ledger_after':[x for x in after['usage']['lines'] if x.get('source_id')=='node.16249'],
   'weapon_item_id':14,'weapon_slot':'weapon1','active_item_set':1,'weapon_base':weapon_base,
   'weapon_raw_text':weapon_item.text.strip(),'sceptre_raw_text':sceptre_item.text.strip(),
   'local_physical_increased_percent':397+60,'quality_percent':20,'weapon_range':[weapon_min,weapon_max],
   'shared_range':[shared_min,shared_max],'shared_effective_range':[shared_min*.7,shared_max*.7],
   'no_player_global_damage_used':True},
 'selected_minion':{'id':'WolfMinion','summoned_by':'WolfPackPlayer','skill':'MeleeAtAnimationSpeedWolfCompanion',
   'gem_level':19,'quality':20,'monster_level':38,'native_monster':wolf,'ally_damage_table':ally_damage,
   'virtual_weapon_range':[base_avg*.8,base_avg*1.2],'added_damage_effectiveness':.7,'sceptre_flat':sceptre_flat,
   'increased_damage':183,'feeding_frenzy_more':30,'crit_chance':crit,'crit_multiplier':crit_mult,'crit_effect':crit_effect,'action_rate':rate,
   'physical_mitigation':{'armour':armour,'armour_ratio':ratio,'damage_taken':taken,'equation':'H * (1-A/(A+10*H)) * 1.06, evaluated separately for ordinary H and crit H*2.15'},
   'elemental_mitigation':{'inferred_multiplier':inferred_elemental_multiplier,'arithmetic_profile_multiplier':elemental_multiplier,
       'equivalent_resistance_percent':1,'status':'inferred from retained total after reconstructing physical; exact upstream resistance source not independently verified in this bounded pass'}},
 'four_way':four_way,
 'attribution':{'S04_delta_with_banner_held_on':values[True,True]-values[False,True],
   'delta_residual':(observed_after-observed_before)-(values[True,True]-values[False,True]),
   'after_reconstruction_residual':observed_after-values[True,True],
   'before_reconstruction_residual':observed_before-values[False,True],
   'banner_damage_delta_with_S04_held_on':values[True,True]-values[True,False],
   'remaining_no_banner_profile_minus_pinned':values[True,False]-mh['TotalDPS'],
   'remaining_status':'3.14837 DPS combines rounding/resistance-profile differences; not asserted as a verified game rounding boundary and not a whole-row known difference'},
 'pinned_S04_proof':{'base_ranges':pinned_base,'physical_hit_range':[mh['PhysicalStoredHitMin'],mh['PhysicalStoredHitMax']],
   'physical_hit_average':mh['PhysicalStoredHitAvg'],'physical_crit_average':mh['PhysicalStoredCritAvg'],
   'cold_hit_average':mh['ColdStoredHitAvg'],'lightning_hit_average':mh['LightningStoredHitAvg'],
   'elemental_resistance':2,'enemy_armour':8063,'dps_reconstructed_from_cached_ranges':pinned_reconstructed,
   'cache_rounding_residual':mh['TotalDPS']-pinned_reconstructed,
   'more_damage_rows':pinned['minionModList']['MORE']['Damage'],
   'code_locations':['vendor/PathOfBuilding-PoE2/src/Modules/ModParser.lua:3302','vendor/PathOfBuilding-PoE2/src/Classes/PassiveTree.lua:481','vendor/PathOfBuilding-PoE2/src/Modules/CalcOffence.lua:820']},
 'banner':{'native_level_18':banner_level,'native_constants':banner_native['constant_stats'],
   'gem_grants_additional_planted_effect':True,'config_bannerPlanted':True,'config_bannerValour':50,
   'effect_sums':banner_sums,'effect_factor':banner_factor,'more_attack_damage_percent':math.floor(25*banner_factor),
   'source_before_and_after':'buff.WarBannerPlayer.base_skill_buff_attack_damage_+%_final_to_apply',
   'recipient_and_activation':'Planted War Banner aura affects you and nearby allies. Explicit bannerPlanted=true supplies the selected activation snapshot; existing aura-area assumption supplies recipient presence. No history/uptime is inferred.',
   'native_text_corroboration':{'url':'https://poe2db.tw/us/War_Banner','checked_date':'2026-10-26','paraphrase':'Maximum Glory permits placing a timed banner; the aura grants the character and nearby allies attack damage, attack speed and accuracy.'},
   'pinned_omission_cause_status':'consistent with recipient-local BannerPlanted condition retained on aura modifiers while config sets it only on player; source inspection plus cached missing modifier, no modified oracle execution',
   'pinned_code_locations':['src/Data/Skills/act_str.lua:21034','src/Modules/ConfigOptions.lua:260','src/Modules/CalcPerform.lua:2320']},
 'separate_confirmed_static_gap':{'stat_id':speed_stat,'value_at_level18':25,'planted_aura':True,
   'proven_by':['native statset positive value','active same planted aura supplies damage/accuracy','no native or vendor extracted speed mapping','uncapped action-rate formula contains only +15% from sceptre'],
   'current_rate_formula':by_stat['Minion:ActionRate'],'pinned_rate':mh['Speed'],
   'minimal_path':'rules/skill_stats.rs::player_buff_modifiers_impl -> existing BuffSpec Aura pass; AttackSpeed INC with BannerPlanted condition; per-stat exporter absent provenance',
   'expected_scaled_increase_percent':math.floor(25*banner_factor),'analytical_rate_with_missing_buff':1.25*(1+.15+math.floor(25*banner_factor)/100),
   'included_in_four_way':False,'owner':'root-authorized S12 follow-up, separate from this pre-S12 receipt'},
 'limits':['No in-game rounding test','Aura-area presence uses existing static assumption','No whole-row known classification','No production changes while producing this evidence','No additional CLI/oracle/cargo/batch calls']
}
(OUT/'evidence.json').write_text(json.dumps(proof,indent=2)+'\n')
print(json.dumps({'four_way':four_way,'attribution':proof['attribution'],'evidence_path':str(OUT/'evidence.json')},indent=2))
