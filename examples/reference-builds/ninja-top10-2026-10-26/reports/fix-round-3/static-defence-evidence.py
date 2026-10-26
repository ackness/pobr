"""Bounded arithmetic audit of frozen worked-build caches; invokes no engine/oracle."""
import hashlib
import json
import math
from pathlib import Path
import subprocess
import xml.etree.ElementTree as ET

ROOT = Path('/root/codes/pobr')
CACHE = ROOT / '.cache/accuracy-round2-2026-10-26'
OUT = Path(__file__).with_name('static-defence-attribution.json')
report = json.loads((CACHE / 'final-classified.json').read_text())
rows = {b['short']: b['rows'] for b in report['results']}
tree = json.loads(Path('data/0.5.5.4.2/base/passive_tree.json').read_text())
checks = []


def check(label, actual, expected, tolerance=0.00001):
    assert math.isclose(actual, expected, rel_tol=0, abs_tol=tolerance), (label, actual, expected)
    checks.append({'label': label, 'actual': actual, 'expected': expected, 'absolute_tolerance': tolerance})


def load(slug):
    path = CACHE / 'final-builds' / (slug + '.json')
    code = ROOT / 'examples/reference-builds/ninja-top10-2026-10-26/builds' / slug / 'code.txt'
    digest = hashlib.sha256(''.join(code.read_text().split()).encode()).hexdigest()[:16]
    matches = sorted((ROOT / '.cache/pob2-oracle').glob(digest + '-*.json'))
    assert len(matches) == 1, (slug, matches)
    oracle_path = matches[0]
    native = json.loads(path.read_text())
    oracle = json.loads(oracle_path.read_text())
    xml = ET.parse(oracle_path.with_suffix('.xml')).getroot()
    return native, oracle, xml, {
        'native_path': str(path), 'native_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
        'oracle_path': str(oracle_path), 'oracle_sha256': hashlib.sha256(oracle_path.read_bytes()).hexdigest(),
        'xml_path': str(oracle_path.with_suffix('.xml')), 'code_path': str(code),
    }


def selected_rows(slug, stats):
    return [r for r in rows[slug] if r['stat'] in stats]


def lines(native, predicate):
    return [l for l in native['usage']['lines'] if predicate(l)]


evidence = {
    'scope': ['infernalist-03', 'witchhunter-10', 'warbringer-02', 'warbringer-03', 'warbringer-08', 'warbringer-10',
              'pathfinder-01', 'pathfinder-02', 'pathfinder-04', 'pathfinder-08', 'pathfinder-09'],
    'outcome': 'No calculation, parser, rule, documentation, or registry edits. Bounded source/arithmetic assessment only.',
    'merged_parent': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
    'inputs': {'report': str(CACHE / 'final-classified.json'), 'frozen_native_revision': 'ef00 (parent-provided label)',
               'source_version': '0.5.5.4.2', 'vendor_revision': 'bb52d6b3',
               'basis': 'Frozen reference-basis outputs; native usage retains game-source evidence. No new calculations or oracle executions.'},
    'authority': 'Frozen game text and retained game data; cached PoB2 only cross-checks arithmetic.',
    'game_nodes': [n for n in tree if n['skill'] in [51446, 53607]],
    'validation': 'Standalone arithmetic assertions over cached inputs only; no Rust changes, focused tests, Clippy, or fmt needed for this no-edit outcome.',
}

n, o, xml, source = load('infernalist-03')
life = 1528 * 1.05 * .55
check('infernalist-03 frozen Life', life, n['output']['life'])
check('infernalist-03 frozen unreserved Life', life * .34, 300.0228)
oracle_more = math.floor((1 - .15) * (1 - .30) * 100 + .5) / 100
oracle_life = math.floor(1528 * 1.05 * oracle_more + .5)
check('infernalist-03 oracle Life from split rune MORE', oracle_life, o['calcsOutput']['Life'])
check('infernalist-03 oracle unreserved Life', oracle_life - math.ceil(oracle_life * .66), o['calcsOutput']['LifeUnreserved'])
evidence['infernalist_03'] = {
    'status': 'Explained frozen-text versus oracle reconstructed-rune behavior; preserve frozen 45% line.',
    'sources': source,
    'rows': selected_rows('infernalist-03', ['Life', 'LifeUnreserved', 'LifeRecoverable']),
    'native_life_formula': next(b for b in n['breakdowns'] if b['stat'] == 'Life'),
    'frozen_weapon': [item.text for item in xml.findall('.//Items/Item') if item.get('id') == '18'],
    'native_rune_line': lines(n, lambda l: l.get('text') == '45% less maximum Life'),
    'oracle_life_more_mods': o['extraModList']['MORE']['Life'],
    'arithmetic': {'native': '1528 * 1.05 * (1 - .45) = 882.42; unreserved/recoverable = .34 * 882.42 = 300.0228',
                   'oracle': 'round2(.85*.70) = round2(.595) = .60; round(1528*1.05*.60) = 963; 963-ceil(963*.66)=327'},
    'crosscheck': 'Pinned ModDB:MoreInternal rounds its MORE product to 2 decimal places; CalcDefence doActorLifeManaSpirit rounds the final pool.',
    'limits': 'No new in-game rounding measurement. Do not change PoBR precision or split the frozen combined rune line. Reference-class EHP/max-hit rows were not reclassified by this assessment.'
}

n, o, xml, source = load('witchhunter-10')
co = o['calcsOutput']
wh = {}
for name, extra, armour, evasion, life, mana, expected_ehp in [
    ('native', 1323, 2750, 10185, 2109.45, 1186.5, 43182.673520364),
    ('oracle', 0, 2393, 7613, 2109, 1187, 29844.51499),
]:
    base_evasion = 3296 + extra
    calculated_armour = math.floor((2004 + base_evasion * .36) * 1.50 * .50 + .5)
    calculated_evasion = math.floor((base_evasion * 1.62 + 1703 * .59) * 1.20 + .5)
    check('witchhunter-10 ' + name + ' Armour', calculated_armour, armour)
    check('witchhunter-10 ' + name + ' Evasion', calculated_evasion, evasion)
    aegis = math.ceil((armour + evasion) * .30)
    life_available = life * .34 if name == 'native' else life - math.ceil(life * .66)
    pool = life_available + mana + aegis
    deflection = evasion * 1.29
    enemy_accuracy = math.floor(2114 * (1 - .20 * 1.20) + .5)
    deflect_chance = 100 - math.floor(enemy_accuracy / (enemy_accuracy + .12 * deflection) * 150 - 50 + .5)
    evade = 100 - math.floor((1 - .95 * evasion / (evasion + 4 * enemy_accuracy)) * 100 + .5)
    damage_chance = 1 - deflect_chance / 100 * .40
    if name == 'native':
        ehp = n['output']['ehp']
        total_taken = sum(t['taken_hit'] for t in ehp['by_type'].values())
        expected_hits = ehp['damaging_hits']
        incoming_physical = ehp['by_type']['Physical']['enemy_damage']
    else:
        total_taken = sum(co[t + 'TakenHit'] for t in ['Physical', 'Fire', 'Cold', 'Lightning', 'Chaos'])
        expected_hits = co['NumberOfDamagingHits']
        incoming_physical = 965 * co['EnemyCritEffect']
    physical_dr = math.floor(100 * armour / (armour + 10 * incoming_physical) + .5)
    check('witchhunter-10 ' + name + ' PhysicalDamageReduction', physical_dr, 22 if name == 'native' else 20)
    damaging_hits = pool / total_taken
    check('witchhunter-10 ' + name + ' damaging hits', damaging_hits, expected_hits)
    total_ehp = damaging_hits / damage_chance / (1 - evade / 100) * 4246
    check('witchhunter-10 ' + name + ' TotalEHP', total_ehp, expected_ehp)
    for stat, value in [('DeflectionRating', deflection), ('DeflectChance', deflect_chance),
                        *[(s, evade) for s in ['EvadeChance', 'MeleeEvadeChance', 'ProjectileEvadeChance', 'SpellEvadeChance', 'SpellProjectileEvadeChance']]]:
        expected = next(r for r in rows['witchhunter-10'] if r['stat'] == stat)['pobr' if name == 'native' else 'pob2']
        check('witchhunter-10 ' + name + ' ' + stat, value, expected)
    max_hits = {}
    for dtype, resistance_mult, armour_fraction in [('Physical', 1, 1), ('Fire', .28, .15), ('Cold', .28, .15), ('Lightning', .28, .15), ('Chaos', 1, 0)]:
        h = pool / resistance_mult
        value = math.floor((h + math.sqrt(h*h + .4*h*armour*armour_fraction))/2) if armour_fraction else math.floor(h+.5)
        expected = next(r for r in rows['witchhunter-10'] if r['stat'] == dtype + 'MaxHit')['pobr' if name == 'native' else 'pob2']
        check('witchhunter-10 ' + name + ' ' + dtype + 'MaxHit', value, expected)
        max_hits[dtype] = value
    wh[name] = {'base_evasion': base_evasion, 'Armour': armour, 'Evasion': evasion, 'aegis': aegis,
                'life_available': life_available, 'mana': mana, 'total_hit_pool': pool,
                'enemy_accuracy_after_blind': enemy_accuracy, 'DeflectionRating': deflection, 'DeflectChance': deflect_chance, 'EvadeChance': evade,
                'damage_chance': damage_chance, 'total_taken_hit': total_taken, 'damaging_hits': damaging_hits,
                'TotalEHP': total_ehp, 'max_hits': max_hits}
evidence['witchhunter_10'] = {
    'status': 'Exact arithmetic attribution for all 16 reported defence rows; preserve game-granted passive.',
    'sources': source, 'rows': rows['witchhunter-10'], 'arithmetic': wh,
    'frozen_megalomaniac': [item.text for item in xml.findall('.//Items/Item') if item.get('id') == '4'],
    'native_source_lines': lines(n, lambda l: l['source_id'] in ['node.51446', 'node.13823', 'node.28267', 'skill.FrostWallPlayer.blind_effect'] or any(s in (l.get('text') or '') for s in ['aegis_unique_shield_max_value_from_%_armour_evasion', '15% of Armour also applies', 'Gain 36% of Evasion'])),
    'native_ehp_terms': n['output']['ehp'],
    'oracle_defence_terms': {k: v for k, v in co.items() if k in ['ArmourOnGloves', 'sharedAegis', 'EnemyCritChance', 'EnemyCritEffect', 'PhysicalDamageReduction', 'ConfiguredDamageChance', 'ConfiguredNotHitChance'] or k.endswith('TakenHit') or k.endswith('TotalHitPool')},
    'oracle_evasion_base_mods': o['extraModList']['BASE'].get('Evasion', []),
    'formulas': {
        'Armour': 'round((681+1323 + .36*(7+620+966+1703+LeatherBound1323))*1.50*.50)',
        'Evasion': 'round(((7+620+966+1703+LeatherBound1323)*1.62 +1703*.59)*1.20)',
        'enemy_accuracy': 'round(2114*(1-.20*1.20))=1607 on both sides; one strongest skill blind (FrostWallPlayer.blind_effect).',
        'EvadeChance': '100-round((1-.95*Evasion/(Evasion+4*1607))*100) =58 native,52 oracle; all five attack/spell variants agree.',
        'DeflectChance': '100-round(1607/(1607+.12*DeflectionRating)*150-50) =74 native,63 oracle.',
        'SorceryWard': 'ceil(.30*(Armour+Evasion)); Ceremonial Ablution already selected, so shared aegis protects all types',
        'MaxHit': 'Physical: floor((pool+sqrt(pool^2+.4*pool*Armour))/2). Elements: substitute h=pool/.28 and .15*Armour. Chaos: round(pool). Incoming damage multiplier is 1 for this max-hit view.',
        'TotalEHP': 'pool/sum(cached taken hits) / (1-.40*DeflectChance/100) / (1-EvadeChance/100) *4246',
        'PhysicalDR': 'round(100*Armour/(Armour+10*physical enemy hit)) =22 native,20 oracle',
        'incoming_crit': 'Enemy crit chance=5%*(1-.25)*(1-evade/100); bonus=30%*(1-.25). Crit effect=1+chance/100*.225, giving1.00354375 native and1.00405 oracle. Enemy physical hit965*effect=968.41971875 native and968.90825 oracle. Controlling Magic node13823 reduces chance; Desensitisation node28267 reduces bonus.'
    },
    'limits': 'No in-game validation of existing evasion/deflection/armour rounding or combat timeline. No need to remove the passive, alter jewel behavior, or change precision for parity.'
}

evidence['warbringer'] = []
for slug, base, native_inc, oracle_inc, native_count, oracle_count in [
    ('warbringer-02', 7, 15, 25, 0, 1), ('warbringer-03', 7, 45, 55, 0, 1),
    ('warbringer-08', 187, 55, 65, 1, 2), ('warbringer-10', 7, 0, 10, 0, 1),
]:
    n, o, xml, source = load(slug)
    check(slug + ' native Evasion', math.floor(base*(1+native_inc/100)+.5), n['output']['evasion'])
    check(slug + ' oracle Evasion', math.floor(base*(1+oracle_inc/100)+.5), o['calcsOutput']['Evasion'])
    for r in selected_rows(slug, ['DeflectionRating']):
        check(slug + ' native DeflectionRating', n['output']['evasion'] * .12, r['pobr'])
        check(slug + ' oracle DeflectionRating', o['calcsOutput']['Evasion'] * .12, r['pob2'])
    evidence['warbringer'].append({
        'build': slug, 'status': 'Explained selected-count difference, not a rounding bug.', 'sources': source,
        'rows': selected_rows(slug, ['Evasion', 'DeflectionRating']),
        'base_evasion': base, 'native_INC': native_inc, 'oracle_INC': oracle_inc,
        'native_summoned_count': native_count, 'oracle_summoned_count': oracle_count,
        'source_lines': lines(n, lambda l: l['source_id'] == 'node.53607'),
        'explicit_totem_inputs': [x.attrib for x in xml.findall('.//Input') if 'totem' in x.get('name','').lower()],
        'selected_skill': n['usage']['main_skill'],
        'oracle_evasion_increases': o['extraModList']['INC'].get('Evasion', []),
        'count_authority': 'Existing selected-skill snapshot: none inferred for non-totem selection; one for selected totem-hosted skill. Capacity does not prove a summoned count. See fix-round-2/totem-evidence.json count_summary.',
        'deflection': 'Where reported (02/10), DeflectionRating = .12 * rounded Evasion on both sides.',
        'limits': 'Offence and cost rows were not assessed or reclassified. No inferred additional summoned totem or Presence history.'
    })

evidence['pathfinder_flasks'] = []
for slug in ['pathfinder-01', 'pathfinder-02', 'pathfinder-04', 'pathfinder-08', 'pathfinder-09']:
    n, o, xml, source = load(slug)
    u = n['output']['utility_stats']
    scoped = selected_rows(slug, ['LifeRegenRecovery', 'EnergyShieldRegenRecovery'])
    for row in scoped:
        key = 'Flask1RecoveryPerSecond' if row['stat'] == 'LifeRegenRecovery' else 'Flask1EnergyShieldRecoveryPerSecond'
        check(slug + ' ' + row['stat'] + ' missing amount already in utility', row['pob2'] - row['pobr'], u[key])
    evidence['pathfinder_flasks'].append({
        'build': slug, 'status': 'Static recovery reader exists; aggregate sustained-regeneration treatment deferred.',
        'sources': source, 'rows': scoped,
        'per_use_output': {k: v for k, v in u.items() if k in ['Flask1Duration', 'Flask1Recovery', 'Flask1InstantRecovery', 'Flask1RecoveryPerSecond', 'Flask1EnergyShieldRecovery', 'Flask1EnergyShieldInstantRecovery', 'Flask1EnergyShieldRecoveryPerSecond']},
        'equipped_flasks': [x.attrib for x in xml.findall('.//Slot') if 'Flask' in x.get('name','')],
        'source_lines': lines(n, lambda l: any(m['name'] in ['LifeFlaskAppliesToEnergyShield', 'LifeFlaskEffectNotRemoved'] for m in l['modifiers'])),
        'reader': 'crates/pobr-core/src/calc/utility.rs: RecoveryPerSecond = gradual recovery / duration. LifeFlaskAppliesToEnergyShield copies per-use recovery with destination EnergyShieldRecoveryRate and CannotGainEnergyShield gate.',
        'aggregate_boundary': 'crates/pobr-core/src/calc/recovery.rs fill_regeneration does not fold finite flask recovery into sustained regeneration. Current guide documents per-use fields without use frequency/uptime inference.',
        'limits': 'Slots are enabled in frozen XML, but this assessment does not infer continuous flask drinking, queueing, charge supply, event history, or aggregate regen uptime. No new reader or model is justified within the requested scope.'
    })

evidence['explicit_unresolved'] = [
    'Runic Tempering remains inactive without proved plates; do not infer plate use.',
    'Blood-mage-08 residual and intrinsic 15% Life-to-ES conversion semantics remain as previously reported; not revisited here.',
    'No new in-game proof of rounding boundaries; precision remains unchanged.',
    'Pathfinder sustained-regeneration display/model treatment is deferred; finite flask recovery is already computed.',
    'This bounded pass does not reclassify unrelated rows or assert all remaining full307 defence discrepancies are resolved.',
    'Root owns documentation/known registry and report archival. This file supplies evidence only; no row was marked known here.'
]
evidence['arithmetic_checks'] = checks
OUT.write_text(json.dumps(evidence, indent=2, ensure_ascii=False) + '\n')
print(json.dumps({'evidence': str(OUT.resolve()), 'arithmetic_checks_passed': len(checks), 'builds': len(evidence['scope'])}))
