"""Retain receipts and evidence after all frozen measurements complete."""
import hashlib
import json
import shutil
from pathlib import Path

root = Path(__file__).resolve().parents[2]
cache = Path(__file__).resolve().parent
out = root / 'examples/reference-builds/ninja-top10-2026-10-26/reports/fix-round-5'
read = lambda p: json.loads(p.read_text())
summary = read(out / 'summary.json')
assert summary['after']['failed_imports_or_calculations'] == 0
assert read(out / 'default-calculation.json')['calculated_and_checked'] == 230
assert not read(out / 'default-calculation.json')['failures']
for name in ['cause-evidence.json', 'scope.json', 'review-summary.json', 'validation-summary.json', 'known-after-validation.json', 'residual-static-audit.json']:
    shutil.copyfile(cache / name, out / name)
shutil.copytree(cache / 'evidence', out / 'evidence', dirs_exist_ok=True)
(out / 'evidence/README.txt').write_text('Implementation handoffs retain their as-of-commit validation and parent_remaining/not_run fields as historical records of the child scope. Current parent completion and corpus validation are in ../completion-audit.json, ../validation-summary.json and banner-speed-s12/corpus-validation.json. No historical child record claims to have run the later parent batch.\n')
pre = out / 'evidence/pre-banner'
pre.mkdir(exist_ok=True)
for name in ['measurement-inputs.json', 'summary.json', 'native-summary.json', 'default-calculation.json']:
    shutil.copyfile(cache / 'pre-banner' / name, pre / name)
(pre / 'README.txt').write_text('Historical intermediate measurement at23e01377 before S12. Any pending wording in these frozen snapshots describes that earlier stage, not the current completion status. Current authoritative results are in the report root. The new-row attribution subsequently established the missing native Banner speed stat. Raw intermediate outputs remain at .cache/accuracy-round5-2026-10-26/pre-banner/.\n')
receipts = []
for kind in ['after-builds', 'default-builds']:
    files = sorted((cache / kind).glob('*.json'))
    assert len(files) == 230, (kind, len(files))
    for file in files:
        receipts.append({'kind': kind, 'build': file.stem, 'path': str(file.relative_to(root)), 'sha256': hashlib.sha256(file.read_bytes()).hexdigest()})
(out / 'measurement-artifacts.json').write_text(json.dumps({'source_revision': summary['source_revision'], 'outputs': receipts, 'default_cache_receipts': [read(p) for p in sorted((cache / 'default-receipts').glob('*.json'))]}, indent=2) + '\n')
repro = out / 'measurement-scripts'
repro.mkdir(exist_ok=True)
for name in ['run-triage-save-after.py', 'summarize-native.py', 'check-default-calculation.py', 'summarize-gate.py', 'reclassify-known.py', 'build-report.py', 'write-summary.py', 'finish-archive.py']:
    shutil.copyfile(cache / name, repro / name)
(out / 'measurement-scripts/README.txt').write_text('These are the exact temporary scripts used for this batch. Their relative root is intentional: to rerun, copy them back to .cache/accuracy-round5-2026-10-26/ in the checkout at measurement-inputs.json git_commit. Preserve the recorded input receipt and game snapshot. Commands and limitations are in inputs.json, measurement-inputs.json and validation-summary.json. They do not inspect individual holdout inputs.\n')
normalized = []
for file in out.rglob('*'):
    if file.is_file() and file.suffix in ['.txt', '.log']:
        raw = file.read_text()
        clean = '\n'.join(line.rstrip() for line in raw.splitlines()).rstrip() + '\n'
        if clean != raw:
            file.write_text(clean)
            normalized.append(str(file.relative_to(out)))
files = sorted(p for p in out.rglob('*') if p.is_file() and p.name != 'final-artifact-check.json')
(out / 'final-artifact-check.json').write_text(json.dumps({'measurement_revision': summary['source_revision'], 'files': [{'path':str(p.relative_to(out)), 'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in files], 'limits':'Hash consistency is not independent gameplay validation. Raw native output paths are retained in measurement-artifacts.json; archive summaries retain all differing rows and scoped measured examples.'}, indent=2) + '\n')
manifest = read(out / 'final-artifact-check.json')
manifest['text_archive_normalization'] = {'files': normalized, 'change': 'Trailing whitespace and extra final blank lines only; original raw logs remain in the cache, JSON evidence and raw calculation outputs are unchanged.'}
(out / 'final-artifact-check.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(f'Archived {len(files)} files and {len(receipts)} output receipts.')
