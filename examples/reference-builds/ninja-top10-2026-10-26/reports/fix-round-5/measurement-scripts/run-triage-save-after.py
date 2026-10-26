"""Run the repository triage once, preserving the frozen cohort's full outputs."""
import importlib.util
import json
import sys
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('triage_saved', root / 'devs/scripts/triage.py')
triage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(triage)
engine_report = triage.engine_report
out = Path(__file__).resolve().parent / 'after-builds'
out.mkdir(exist_ok=True)
cohort = root / 'examples/reference-builds/ninja-top10-2026-10-26/builds'


def capture(binary, code_file):
    extra = 'Int,Str,SpiritUnreserved,EsRechargeRate,EsRechargePerSecond,EffectiveLootRarityMod,BleedDuration,BleedDamage,ActiveTotemLimit,ManaLeechGainPerHit,ManaLeechGainRate,ESCost,ESPerSecondCost,LifeLeechGainRate,EnergyShieldLeechGainRate,EsLeechRate,ManaRegenRecovery,RageRegenRecovery,CullingDPS'
    if code_file.parent.name == 'tactician-06' and code_file.parent.parent.resolve() == cohort:
        extra += ',Minion:TotalDPS,Minion:ActionRate'
    result = subprocess.run([str(binary), 'calculate-build', '--file', str(code_file),
        '--breakdown', 'all,SkillSpiritReservationBase,' + extra,
        '--json', '--reference-basis', '--usage'], cwd=root, capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(f'{code_file}: {result.stderr[-2000:]}')
    report = json.loads(result.stdout)
    if code_file.parent.parent.resolve() == cohort:
        (out / (code_file.parent.name + '.json')).write_text(json.dumps(report) + '\n')
    return report


triage.engine_report = capture
if __name__ == '__main__':
    sys.exit(triage.main())
