"""Archive measured results and preserve every remaining differing row."""
import collections
import base64
import csv
import json
import shutil
import subprocess
import xml.etree.ElementTree as ET
import zlib
from pathlib import Path

root = Path(__file__).resolve().parents[2]
cache = Path(__file__).resolve().parent
previous = root / '.cache/accuracy-round4-2026-10-26'
out = root / 'examples/reference-builds/ninja-top10-2026-10-26/reports/fix-round-5'
load = lambda p: json.loads(p.read_text())
cohort = 'examples/reference-builds/ninja-top10-2026-10-26/builds'
before = {row['build']: row for row in load(previous / 'after-classified.json')['results'] if row['set'] == cohort}
after = {row['build']: row for row in load(cache / 'after-classified.json')['results'] if row['set'] == cohort}
assert len(before) == len(after) == 230
native = load(out / 'native-summary.json')
classes = lambda rows: dict(collections.Counter(row['class'] for build in rows.values() for row in build['rows']))
transitions, resolved, added = [], [], []
for name in sorted(before):
    old_rows = {row['stat']: row for row in before[name]['rows']}
    new_rows = {row['stat']: row for row in after[name]['rows']}
    for stat in sorted(old_rows.keys() | new_rows.keys()):
        old, new = old_rows.get(stat), new_rows.get(stat)
        transitions.append({'build': name, 'stat': stat, 'before_class': old['class'] if old else 'agrees', 'after_class': new['class'] if new else 'agrees', 'before_value': old['pobr'] if old else None, 'after_value': new['pobr'] if new else None, 'saved_reference': (new or old)['code'], 'pinned_pob2': (new or old)['pob2'], 'reason': new['reason'] if new else None})
        if old and not new:
            resolved.append([name, stat])
        if new and not old:
            added.append([name, stat])
with (out / 'row-transitions.csv').open('w', encoding='utf-8-sig', newline='') as stream:
    writer = csv.DictWriter(stream, fieldnames=list(transitions[0]), lineterminator="\n")
    writer.writeheader()
    writer.writerows(transitions)

for source, target in [('after-classified.json', 'triage.json'), ('after-classified.log', 'triage.txt'), ('gate.log', 'gate.log'), ('gate-inputs.json', 'gate-inputs.json'), ('after-measurement-inputs.json', 'measurement-inputs.json'), ('added-known-entries.json', 'added-known-entries.json')]:
    shutil.copyfile(cache / source, out / target)
for prefix in ['gate', 'gate-rerun', 'gate-final', 'gate-banner']:
    if (cache / f'{prefix}.log').exists():
        for suffix in ['.log', '-inputs.json', '-exit.txt']:
            shutil.copyfile(cache / f'{prefix}{suffix}', out / f'{prefix}{suffix}')

def value(report, stat, name):
    for row in report['breakdowns']:
        if row['stat'] == stat:
            if stat == 'Minion:ActionRate' and row['value'] is None:
                assert row['breakdown']['actor'] == {'kind': 'minion', 'index': 0}
                return row['breakdown']['formula']['value']
            return row['value']
    # Default reports also contain a separately calculated reference comparison.
    # Its values must never substitute for the actual default output.
    if report.get('reference_basis', False):
        for row in report['reference']['differing']:
            if row['stat'] == stat:
                return row['pobr']
    if stat.startswith('Minion'):
        minions = [m for m in report['output'].get('minions', []) if m['summoned_by'] == report['main_skill']['skill_id']]
        if len(minions) > 1:
            code = (root / cohort / name / 'code.txt').read_text().strip()
            xml = ET.fromstring(zlib.decompress(base64.urlsafe_b64decode(code + '=' * (-len(code) % 4))))
            skills = xml.find('Skills')
            skill_set = next((row for row in skills.findall('SkillSet') if row.get('id') == skills.get('activeSkillSet')), skills)
            index = int(xml.find('Build').get('mainSocketGroup')) - 1
            assert index == report['main_skill']['group_index'], name
            group = skill_set.findall('Skill')[index]
            gems = [gem for gem in group.findall('Gem') if gem.get('skillId') == report['main_skill']['skill_id']]
            assert len(gems) == 1 and gems[0].get('skillMinion'), name
            minions = [minion for minion in minions if minion['minion_id'] == gems[0].get('skillMinion')]
            assert len(minions) == 1, name
        if len(minions) == 1:
            return minions[0].get({'MinionTotalDPS': 'dps', 'MinionLife': 'life', 'MinionEnergyShield': 'energy_shield'}.get(stat, stat))
    if stat == 'CritChance':
        return report['output']['crit_chance'] * 100
    names = {'TotalDPS': 'dps', 'TotalHitAvg': 'total_hit_avg', 'CritMultiplier': 'crit_multiplier', 'IgniteDPS': 'ignite_dps', 'BleedDPS': 'bleed_dps', 'Life': 'life', 'Mana': 'mana', 'EnergyShield': 'energy_shield', 'FireResist': 'fire_resistance', 'ColdResist': 'cold_resistance'}
    return report['output'].get(names.get(stat, stat), report['output']['utility_stats'].get(stat))

examples = []
cases = {
 'acolyte-of-chayula-08': ['Life', 'Mana', 'EnergyShield'],
 'oracle-06': ['Life', 'Mana', 'EsRechargePerSecond'],
 'oracle-04': ['EsRechargePerSecond'],
 'shaman-07': ['EsRechargePerSecond', 'TotalDPS'],
 'blood-mage-07': ['TotalHitAvg', 'TotalDPS'],
 'blood-mage-09': ['TotalHitAvg', 'TotalDPS'],
 'invoker-03': ['TotalHitAvg', 'TotalDPS'],
 'tactician-06': ['MinionTotalDPS', 'Minion:ActionRate'],
 'spirit-walker-04': ['FireResist', 'ColdResist'],
 'spirit-walker-10': ['FireResist', 'ColdResist'],
 'warbringer-07': ['EffectiveLootRarityMod'],
 'smith-of-kitava-06': ['BleedDuration', 'BleedDamage'],
 'blood-mage-01': ['BleedDPS', 'IgniteDPS'],
 'pathfinder-06': ['ManaLeechGainPerHit', 'ManaLeechGainRate'],
 'warbringer-01': ['TotalDPS', 'IgniteDPS'],
 'disciple-of-varashta-02': ['Mana', 'MinionTotalDPS'],
}
for name, stats in cases.items():
    old, new = load(previous / 'after-builds' / f'{name}.json'), load(cache / 'after-builds' / f'{name}.json')
    old_default = load(previous / 'default-builds' / f'{name}.json')
    new_default = load(cache / 'default-builds' / f'{name}.json')
    for stat in stats:
        examples.append({'build': name, 'stat': stat, 'before_reference_basis': value(old, stat, name), 'before_value_note': 'not retained in old standard panels' if value(old, stat, name) is None else None, 'after_reference_basis': value(new, stat, name), 'remaining_reference_difference': next((row for row in new['reference']['differing'] if row['stat'] == stat), None)})
        examples[-1].update(before_default=value(old_default, stat, name), after_default=value(new_default, stat, name), before_default_note='not retained in old panels' if value(old_default, stat, name) is None else None)

summary = {'source_revision': native['git_commit'], 'data_version': native['data_version'], 'corpus_size': 230, 'non_holdout_builds_cross_checked': 307, 'before': {'source_revision': '45a3c00a265a60ad69cf2c152395f443089abf13', 'agreed_rows': 19190, 'compared_rows': 20427, 'damage_agrees': 169, 'unsupported_line_occurrences': 2109, 'classification': classes(before)}, 'after': {**{key: native[key] for key in ['agreed_rows', 'compared_rows', 'builds_with_agreeing_damage', 'unsupported_line_occurrences', 'builds_with_all_compared_rows_agreeing', 'failed_imports_or_calculations', 'missing_items_reported', 'negative_typed_hit_pools']}, 'classification': classes(after)}, 'rows_newly_within_tolerance': len(resolved), 'rows_newly_outside_tolerance': len(added), 'resolved_rows': resolved, 'newly_different_rows': added, 'measured_examples': examples, 'commits': subprocess.check_output(['git', 'log', '--no-merges', '--reverse', '--format=%H %s', '45a3c00a..' + native['git_commit']], cwd=root, text=True).splitlines(), 'classification_caveat': 'engine/both are diagnostic candidates, not a count of proven bugs. Saved references and pinned PoB2 are cross-checks, not game truth. Known rows retain narrow source evidence; no whole build is declared correct from numerical agreement.', 'measurement_limits': native['limits'], 'validation': load(cache / 'validation-summary.json'), 'default_mode_validation': 'default-calculation.json', 'independent_review': 'review-summary.json', 'completion_status': 'Current batch addresses twelve proven static causes; final review and measurements determine closure.', 'remaining_limits': ['Unverified intrinsic minion Life-to-ES deduction, Runic Plate snapshots and several residual damage/level/reservation rows remain explicit in triage.', 'No inferred kill history, Offering consumption, Headhunter buffs, minion Rage or sustained flask uptime is added.', 'Spirit Walker09 Rage III recipient-state question remains unresolved.', 'Unique Herald/Blink mappings are supported by game attachment tables; the separate Oracle04 five-point Spirit pool difference remains unresolved.', 'Finite flask recovery is available separately; it is not automatically aggregated into permanent regeneration.', 'Game rounding is not changed solely for exporter agreement.', 'Retained enableGlobal1/2 flags contain no explicit false in this frozen corpus; legacy false-state interpretation is not validated and existing ordinary-buff alternate-set behavior is preserved.', 'Passing imports and calculation checks do not establish complete support for every modifier or all malformed/historical/multi-set inputs.']}
actor_build = 'disciple-of-varashta-02'
old_actors = load(previous / 'default-builds' / f'{actor_build}.json')['output']['minions']
new_actors = load(cache / 'default-builds' / f'{actor_build}.json')['output']['minions']
summary['measured_secondary_minion_examples'] = []
for index, expected_id in [(2, 'RaisedSkeletonWarriors'), (6, 'RaisedSkeletonReaver')]:
    old_actor, new_actor = old_actors[index], new_actors[index]
    assert old_actor['minion_id'] == new_actor['minion_id'] == expected_id
    assert (old_actor['skill_id'], old_actor['summoned_by']) == (new_actor['skill_id'], new_actor['summoned_by'])
    summary['measured_secondary_minion_examples'].append({'build': actor_build, 'output_actor_index': index, 'minion_id': expected_id, 'skill_id': new_actor['skill_id'], 'summoned_by': new_actor['summoned_by'], 'before_default_dps': old_actor['dps'], 'after_default_dps': new_actor['dps'], 'scope': 'Individual retained secondary minion output; not a FullDPS aggregate or selected-main-minion comparison.'})
(out / 'summary.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2) + '\n')
print(json.dumps({key: value for key, value in summary.items() if key in ['source_revision', 'before', 'after', 'rows_newly_within_tolerance', 'rows_newly_outside_tolerance', 'newly_different_rows']}, ensure_ascii=False, indent=2))
