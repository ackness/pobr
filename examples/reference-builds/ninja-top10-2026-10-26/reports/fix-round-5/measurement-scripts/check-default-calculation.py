"""Validate the product's default calculation separately from reference mode."""
import concurrent.futures
import hashlib
import json
import math
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parents[2]
cache = Path(__file__).resolve().parent
cohort = root / 'examples/reference-builds/ninja-top10-2026-10-26'
destination = cache / 'default-builds'
destination.mkdir(exist_ok=True)
receipts = cache / 'default-receipts'
receipts.mkdir(exist_ok=True)
inputs = json.loads((cache / 'after-measurement-inputs.json').read_text())
measurement_hash = hashlib.sha256((cache / 'after-measurement-inputs.json').read_bytes()).hexdigest()
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip() == inputs['git_commit']
binary = root / 'target/debug/pobr'
assert hashlib.sha256(binary.read_bytes()).hexdigest() == inputs['cli_sha256']
expected_codes = {row['name']: row['code_sha256'] for row in json.loads((cohort / 'reports/fix-round-5/inputs.json').read_text())['builds']}


def run(code_path):
    name = code_path.parent.name
    code_hash = hashlib.sha256(code_path.read_text().strip().encode()).hexdigest()
    assert code_hash == expected_codes[name], name
    saved = destination / f'{name}.json'
    receipt_path = receipts / f'{name}.json'
    command = [str(binary), 'calculate-build', '--file', str(code_path), '--json', '--breakdown', 'TotalDPS,TotalHitAvg,FireResist,ColdResist,Int,Str,ManaLeechGainPerHit,ManaLeechGainRate,ESCost,ESPerSecondCost,EnergyShieldLeechGainRate,EsLeechRate,ManaRegenRecovery,RageRegenRecovery,CullingDPS,ActiveTotemLimit,SpiritUnreserved,EsRechargeRate,EsRechargePerSecond,EffectiveLootRarityMod,BleedDuration,BleedDamage']
    if name == 'tactician-06':
        command[-1] += ',Minion:TotalDPS,Minion:ActionRate'
    expected_receipt = {'measurement_sha256': measurement_hash, 'cli_sha256': inputs['cli_sha256'], 'code_sha256': code_hash, 'command': command}
    reusable = False
    if saved.exists() and receipt_path.exists():
        receipt = json.loads(receipt_path.read_text())
        reusable = all(receipt.get(key) == value for key, value in expected_receipt.items()) and receipt.get('result_sha256') == hashlib.sha256(saved.read_bytes()).hexdigest()
    if not reusable:
        process = subprocess.run(command, cwd=root, text=True, capture_output=True)
        if process.returncode:
            raise RuntimeError(f'{name}: {process.stderr}')
        saved.write_text(process.stdout)
        receipt_path.write_text(json.dumps({**expected_receipt, 'result_sha256': hashlib.sha256(saved.read_bytes()).hexdigest()}, indent=2) + '\n')
    report = json.loads(saved.read_text())
    # The JSON contract omits this field when false.
    assert report.get('reference_basis', False) is False, name
    assert not report['missing_items'], name
    assert report['tree_version']['unknown_node_count'] == 0, name
    output = report['output']
    for key in ['life', 'mana', 'energy_shield', 'armour', 'evasion', 'dps', 'total_hit_avg', 'action_rate', 'mana_cost', 'bleed_dps', 'ignite_dps', 'poison_dps']:
        assert isinstance(output[key], (int, float)) and math.isfinite(output[key]) and output[key] >= 0, (name, key, output[key])
    for kind, terms in (output.get('ehp') or {}).get('by_type', {}).items():
        assert math.isfinite(terms['total_hit_pool']) and terms['total_hit_pool'] >= 0, (name, kind, terms)
    for minion in output.get('minions', []):
        for key in ['life', 'energy_shield', 'dps']:
            if key in minion:
                assert isinstance(minion[key], (int, float)) and math.isfinite(minion[key]) and minion[key] >= 0, (name, minion['minion_id'], key)
    scoped = {}
    scoped_sources = {}
    for row in report['breakdowns']:
        value = row['value']
        if row['stat'] == 'Minion:ActionRate' and value is None:
            # This generic actor stat has a formula, but no scalar catalog key.
            assert row['breakdown']['actor'] == {'kind': 'minion', 'index': 0}
            value = row['breakdown']['formula']['value']
            scoped_sources[row['stat']] = 'breakdown.formula.value; generic actor stat'
        scoped[row['stat']] = value
    for stat, value in scoped.items():
        assert isinstance(value, (int, float)) and math.isfinite(value), (name, stat, value)
        if stat not in ['ManaRegenRecovery', 'RageRegenRecovery', 'SpiritUnreserved', 'FireResist', 'ColdResist']:
            assert value >= 0, (name, stat, value)
    reference = json.loads((cache / 'after-builds' / f'{name}.json').read_text())
    assert report['provenance'] == reference['provenance'], name
    differences = {key: {'default': output[key], 'reference_basis': reference['output'].get(key)} for key in output if output[key] != reference['output'].get(key)}
    return {'build': name, 'code_sha256': code_hash, 'differing_output_fields': sorted(differences), 'selected_outputs': {key: output[key] for key in ['life', 'mana', 'energy_shield', 'dps', 'ignite_dps', 'action_rate']}, 'unsupported_modifier_lines': len(report['unsupported_modifiers']), 'scoped_outputs': scoped, 'scoped_output_sources': scoped_sources}


paths = sorted((cohort / 'builds').glob('*/code.txt'))
assert len(paths) == 230
rows = []
failures = []
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
    futures = {pool.submit(run, path): path.parent.name for path in paths}
    for future in concurrent.futures.as_completed(futures):
        try:
            rows.append(future.result())
        except Exception as error:
            failures.append({'build': futures[future], 'error': str(error)})
        if (len(rows) + len(failures)) % 25 == 0:
            print(f'{len(rows) + len(failures)}/230 checked; {len(failures)} failures', flush=True)
summary = {'source_revision': inputs['git_commit'], 'command': 'python3 .cache/accuracy-round5-2026-10-26/check-default-calculation.py', 'expected_builds': 230, 'calculated_and_checked': len(rows), 'failures': failures, 'builds_differing_from_reference_options': sum(bool(row['differing_output_fields']) for row in rows), 'validation': ['default production options', 'no reported rejected items or unresolved tree nodes', 'finite nonnegative selected player/minion resources and damage', 'finite nonnegative typed hit pools', 'finite scoped cost/recovery/culling/capacity/reservation/ailment/rarity/resistance views; nonnegative costs/leech/culling/capacity/ailment durations' , 'same game-data provenance as reference-mode measurement'], 'limits': ['This is a calculation/invariant check, not proof that all game mechanics are implemented.', 'No FullDPS aggregate or browser interaction is exercised.', 'Conditional models use only the imported snapshot and existing defaults; no new states are invented.'], 'builds': sorted(rows, key=lambda row: row['build'])}
(cohort / 'reports/fix-round-5/default-calculation.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2) + '\n')
print(json.dumps({key: value for key, value in summary.items() if key != 'builds'}, ensure_ascii=False, indent=2))
assert not failures, failures
