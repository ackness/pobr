import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import { getBackend } from '../../src/api/backend';
import type { CalculateBuildRequest, ConfigInputValue, ConfigOption, Breakdown } from '../../src/api/types';
import { useBuildSession } from '../../src/hooks/useBuildSession';
import { CalcsPanel } from '../../src/components/calcs/CalcsPanel';
import { ConfigPanel } from '../../src/components/config/ConfigPanel';
import { UiProvider } from '../../src/components/shared/UiContext';
import { TooltipProvider } from '../../src/components/ui/tooltip';
import { AppButton } from '../../src/components/shared/controls';
import type { Lang } from '../../src/lib/i18n';
import './calcs-config-v2.css';

// Exercise the real session hook and components; deterministic backend results
// allow explicit control over late detail responses without rebuilding WASM.
const probe = {
  requests: [] as CalculateBuildRequest[], metrics: 0, attribution: 0,
  holdNextDetail: false, release: () => {}, inputs: {} as Record<string, ConfigInputValue>,
  edit: (_key: string, _value: ConfigInputValue | null) => {}, focus: (_id: string) => {},
  language: (_lang: Lang) => {}, busy: false,
};
Object.assign(window, { calcsConfigProbe: probe });
const backend = await getBackend();
const calculate = backend.calculateBuild;
const decode = backend.decodeBuild;
const metrics = backend.skillMetrics;
const attribution = backend.attribution;
backend.decodeBuild = async code => ({ ...await decode(code), config_inputs: {
  TestNumber: 0, TestSaved: false, rawText: '0', rawZero: 0, rawFalse: false,
} });
backend.loadConfigOptions = async () => [
  { var: 'TestNumber', label: 'Test number', input_type: 'count', section: 'General', default: { state_number: 5 }, tooltip: 'Numeric values commit on blur.' },
  { var: 'TestSaved', label: 'Saved inapplicable', input_type: 'check', section: 'General', visibility: { if_skill: ['Absent'] } },
  { var: 'TestUnknown', label: 'Unknown applicability', input_type: 'count', section: 'General', visibility: { if_stat: ['Unknown'] } },
  { var: 'TestHidden', label: 'Unsaved inapplicable', input_type: 'check', section: 'General', visibility: { if_skill: ['Absent'] } },
  { var: 'TestList', label: 'Test choice', input_type: 'list', section: 'General', default: { index: 1 }, list_options: [{ value: 'default', label: 'Default' }, { value: 'alternate', label: 'Alternate' }] },
  { var: 'conditionEnemyShocked', label: 'Is the enemy Shocked?', input_type: 'check', section: 'For Effective DPS' },
] as ConfigOption[];
backend.calculateBuild = async request => {
  probe.requests.push(structuredClone(request));
  const result = structuredClone(await calculate(request));
  const life = Number(request.config_inputs?.probeLife ?? 1234);
  const detail = (value: number): Breakdown => ({ base_total: value, inc_total: 0, more_total: 1, mods: [], kind: 'steps',
    formula: { steps: [{ op: 'base', operand: value, result: value }], value } });
  result.stats = result.stats.map(row => row.id === 'Life' ? { ...row, value: life } : row);
  if (request.breakdowns?.includes('Life') && request.breakdown_actor?.kind !== 'minion') result.breakdowns.Life = detail(life);
  result.minions = [{ index: 0, minion_id: 'test-minion', name: 'Test minion', summoned_by: 'fixture', skill_id: 'test',
    level: 20, dps: 300, life: 9876, armour: 0, energy_shield: 0, evasion: 0,
    stats: { Life: 9876, FireResistMax: 75 }, breakdown_descriptors: { Life: 'steps', FireResistMax: 'none' },
    breakdowns: request.breakdown_actor?.kind === 'minion' && request.breakdowns?.includes('Life') ? { Life: { ...detail(9876), actor: { kind: 'minion', index: 0 } } } : {} }];
  result.main_minion_index = 0;
  result.full_dps = { dps: 300, dot_dps: 0, skills: [{ group_index: 0, active_skill_index: 1, skill_id: 'Fireball', count: 1, hit_dps: 300, dot_dps: 0, dps: 300, unsupported_modifiers: [], error: null }] };
  if (request.config_context) result.config_context = { main_skill: 'Fireball', skill_types: [], predicates: { if_skill: { Absent: false } } };
  if (probe.holdNextDetail && request.breakdowns?.length) {
    probe.holdNextDetail = false;
    await new Promise<void>(resolve => { probe.release = resolve; });
  }
  return result;
};
backend.skillMetrics = async request => { probe.metrics++; return metrics(request); };
backend.attribution = async request => { probe.attribution++; return attribution(request); };

function Harness() {
  const [page, setPage] = useState('calcs');
  const [lang, setLang] = useState<Lang>('en-US');
  const [focus, setFocus] = useState<{ id: string } | null>(null);
  const session = useBuildSession(page === 'config');
  probe.edit = session.setConfigInput;
  probe.inputs = session.calcParams.config_inputs;
  probe.focus = id => { setPage('calcs'); setFocus({ id }); };
  probe.language = setLang;
  probe.busy = session.busy;
  return <UiProvider v2 lang={lang}><TooltipProvider><main className="v2-root dark">
    <nav aria-label="Test harness">
      <AppButton onClick={() => setPage('calcs')}>Open calcs</AppButton>
      <AppButton onClick={() => setPage('config')}>Open config</AppButton>
      <AppButton disabled={session.busy || !!session.bootMessage} onClick={() => void session.importCode('fixture')}>Import baseline</AppButton>
    </nav>
    {page === 'calcs' ? <CalcsPanel session={session} lang={lang} focus={focus} onFocusConsumed={() => setFocus(null)} />
      : <ConfigPanel session={session} lang={lang} />}
  </main></TooltipProvider></UiProvider>;
}
createRoot(document.getElementById('root')!).render(<Harness />);
