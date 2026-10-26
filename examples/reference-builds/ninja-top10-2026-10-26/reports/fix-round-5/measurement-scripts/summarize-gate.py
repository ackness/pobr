"""Read only suite results and aggregate accuracy; never inspect holdout builds."""
import json
import re
from pathlib import Path

cache = Path(__file__).resolve().parent
log = re.sub(r'\x1b\[[0-9;]*m', '', (cache / 'gate.log').read_text())
steps = re.findall(r'^  ok\s+(\w+)\s*$', log, re.M)
failed = re.findall(r'^  FAIL\s+(\w+)\s*$', log, re.M)
aggregates = []
for corpus, builds, damage, agreed, compared in re.findall(r'^(examples/(?:wasm-benchmark|reference-builds/(?:tuned|holdout))/builds)\s+(\d+)\s+(\d+)\s+\d+%\s+(\d+)/(\d+)\s*[\d.]+%\s*$', log, re.M):
    aggregates.append({'corpus': corpus, 'builds': int(builds), 'damage_agrees': int(damage), 'agreeing_rows': int(agreed), 'compared_rows': int(compared)})
initial_aggregates = list(aggregates)
initial_failed = list(failed)
reruns = []
for prefix in ['gate-rerun', 'gate-final', 'gate-banner']:
    if not (cache / f'{prefix}.log').exists():
        continue
    rerun_log = re.sub(r'\x1b\[[0-9;]*m', '', (cache / f'{prefix}.log').read_text())
    rerun_ok = re.findall(r'^  ok\s+(\w+)\s*$', rerun_log, re.M)
    rerun_failed = re.findall(r'^  FAIL\s+(\w+)\s*$', rerun_log, re.M)
    failed = sorted((set(failed) - set(rerun_ok)) | set(rerun_failed))
    steps = sorted(set(steps) | set(rerun_ok))
    rerun_aggregates = []
    for corpus, builds, damage, agreed, compared in re.findall(r'^(examples/(?:wasm-benchmark|reference-builds/(?:tuned|holdout))/builds)\s+(\d+)\s+(\d+)\s+\d+%\s+(\d+)/(\d+)\s*[\d.]+%\s*$', rerun_log, re.M):
        rerun_aggregates.append({'corpus':corpus,'builds':int(builds),'damage_agrees':int(damage),'agreeing_rows':int(agreed),'compared_rows':int(compared)})
    if rerun_aggregates:
        assert len(rerun_aggregates) == 3
        aggregates = rerun_aggregates
    reruns.append({'log': f'{prefix}.log', 'aggregate_accuracy': rerun_aggregates, 'inputs': json.loads((cache / f'{prefix}-inputs.json').read_text()), 'exit': int((cache / f'{prefix}-exit.txt').read_text()), 'passed_steps': rerun_ok, 'failed_steps': rerun_failed})
rerun = reruns[-1] if reruns else None
summary = {'initial_aggregate_accuracy': initial_aggregates, 'initial_failed_steps': initial_failed, 'targeted_rerun': rerun, 'gate_inputs': json.loads((cache / 'gate-inputs.json').read_text()), 'gate_exit': int((cache / 'gate-exit.txt').read_text()), 'steps_passed': steps, 'failed_steps': failed, 'aggregate_accuracy': aggregates, 'holdout_scope': 'Aggregate accuracy only; no individual holdout inspected.', 'browser_scope': 'Web typecheck/unit tests and Rust WASM/JSON contracts; 508 Web tests passed, 5 skipped (3 opt-in benchmark tests and 2 useBuildSession WASM tests). No actual browser interaction validation claimed.', 'independent_reviews': 'See review-summary.json; focused regression execution belongs to the implementation commits.', 'reference_scope': 'The accuracy gate covers benchmark/tuned/holdout. The separate triage measurement covers the full230 frozen cohort and77 worked builds; numerical disagreement is diagnostic.'}
summary['targeted_reruns'] = reruns
(cache / 'validation-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary, indent=2))
assert len(aggregates) == 3
assert len(steps) == 10 and not failed
assert (summary['gate_exit'] == 0 if rerun is None else rerun['exit'] == 0)
