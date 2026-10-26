"""Finalize the bounded completion statement from saved measurements only."""
import json
from pathlib import Path
root = Path(__file__).resolve().parents[2]
cache = Path(__file__).resolve().parent
out = root / 'examples/reference-builds/ninja-top10-2026-10-26/reports/fix-round-5'
read = lambda p: json.loads(p.read_text())
s = read(out / 'summary.json'); d = read(out / 'default-calculation.json'); v = read(cache / 'validation-summary.json')
assert d['calculated_and_checked'] == 230 and not d['failures']
assert len(v['steps_passed']) == 10 and not v['failed_steps']
assert v['targeted_rerun']['inputs']['git_commit'] == s['source_revision']
assert v['targeted_rerun']['exit'] == 0
s['completion_status'] = 'Twelve demonstrated static causes and their review defects repaired; all 230 frozen imports and selected calculations checked in both modes. Remaining diagnostic differences are not claimed resolved.'
s['remaining_limits'] += [
 'Tactician06 whole MinionTotalDPS row remains unclassified: S04 is supported in both engines, and old agreement was cancellation with Banner differences. The fixed-rate pre-S12 counterfactual retains an unverified 3.14837044 DPS residual.',
 'Equipment-count eligibility for Transcendent limbs and maximum-resistance-only modifiers remains unverified.',
 'Brutality physical-reduction bypass chance and Second Wind stored-use capability remain diagnostic; no current eligible selected-output defect was established for those separate effects.'
]
(out/'summary.json').write_text(json.dumps(s,ensure_ascii=False,indent=2)+'\n')
completion = {'source_revision':s['source_revision'],'scope':'All 230 frozen Forbidden Rites imports; twelve demonstrated static calculation causes and review corrections, not all possible game mechanics.','implemented_cause_ids':[f'S{i:02}' for i in range(1,13)],'open_proven_defects_in_this_batch':[],'validation_status':'Completed; full gate and affected reruns retained, latest source measured in both modes.','reference_builds_checked':230,'default_builds_checked':230,'non_holdout_cross_checks':307,'remaining_differing_rows':s['after']['compared_rows']-s['after']['agreed_rows'],'remaining_classification':s['after']['classification'],'current_diagnostic_inventory':'triage.json','historical_scope_inventory':'../fix-round-4/completion-scope-inventory.json (as of its recorded source, not current counts)','deferred_or_unverified':s['remaining_limits'],'retention_qualification':'The prior independent native audit reconciled this corpus\'s selected imports, gems, occupied equipment and socket records. Empty rejected-item lists and calculation success alone are not a proof of lossless import; malformed, historical, disabled and multiple-set cases are limited to existing focused regressions.','holdout':'Aggregate gate measurement only; no individual holdout inspected.'}
(out/'completion-audit.json').write_text(json.dumps(completion,ensure_ascii=False,indent=2)+'\n')
c=s['after']['classification']; b=s['before']; a=s['after']
text=f'''Frozen Forbidden Rites cohort — fifth calculation batch, 2026-10-26

Measured calculation revision: {s['source_revision']}.
Game data: 0.5.5.4.2, snapshot 0757-20261026-16736. The original 230 codes
and decoded XML hashes are unchanged. Pinned PoB2 remains clean at
bb52d6b368307457eb9c54bb13f1829993d390b1 and is a cross-check, not game truth.

All 230 target builds imported and calculated in reference mode with no reported
rejected items or negative typed hit pools. Default production mode separately
checked 230/230 with zero failures; {d['builds_differing_from_reference_options']} outputs differ from reference options.
Default cache receipts bind the measured binary, input, options and output hashes.
The full cross-check covers 307 non-holdout builds. No FullDPS aggregate or actual
browser interaction is claimed.

Compared with source 45a3c00a:
  agreeing rows:              {b['agreed_rows']} -> {a['agreed_rows']} / {a['compared_rows']}
  selected damage agreement:  {b['damage_agrees']} -> {a['builds_with_agreeing_damage']} / 230
  unsupported occurrences:    {b['unsupported_line_occurrences']} -> {a['unsupported_line_occurrences']}
  newly within tolerance:     {s['rows_newly_within_tolerance']}
  newly outside tolerance:    {s['rows_newly_outside_tolerance']}

The remaining differing rows classify as {c['engine']} engine, {c['both']} both,
{c['reference']} reference and {c['known']} known. These are diagnostic categories,
not counts of proven bugs. No new known rows or weakened tolerances were added.

Twelve established static causes were repaired: Eldritch Will resource names,
movement increases shared into ES recharge, Lightning hit luck, ally weapon-base
sharing, distinct companion counts, equipment resistance counts, Loreweave source
exclusivity, Crimson Assault duration, Shattering Spite ailment magnitude,
Oisin typed leech, Brutality damage/ailment restrictions, and planted War Banner
attack speed. Existing configuration and aura paths are reused. No kill history,
Glory generation, banner uptime, Headhunter state or sustained flask uptime is
invented. Cause commits, focused checks and independent review responses are
recorded in cause-evidence.json and review-summary.json.

Tactician06 is the only newly outside row. Pinned PoB2 already implements the
weapon share; old agreement was cancellation with its omitted Banner damage.
The exact S04 delta reconstructs to within 6e-10 DPS under the held-rate profile.
A separate native Banner speed omission was then fixed in S12. The reference
mode omits only this exporter-absent speed contribution; native mode includes it.
The fixed-rate counterfactual still has an unverified 3.14837044 DPS remainder,
so the whole row remains unclassified. Pre-S12 evidence is explicitly separated
from final measurements in evidence/tactician-06-s04 and evidence/pre-banner.

All ten gate steps pass using the full run plus source-bound affected reruns;
all run logs and receipts are retained. Benchmark/tuned/holdout aggregates stay
661/715, 5794/6119, 1912/2034, respectively. Holdout is aggregate-only. Web unit
checks retain 508 passes and 5 disclosed skips; no browser or game-client testing
is implied. Remaining uncertainty and import-retention limits are explicit in
completion-audit.json; numerical agreement does not certify every mechanic.
'''
(out/'summary.txt').write_text(text)
print('Finalized summary and bounded completion audit.')
