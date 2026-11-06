import type { ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, test, vi } from 'vitest';
import type { BuildJson, ConfigInputValue } from '../../api/types';
import { applyNamedConfig, backfillFromDecoded, parseSaved, toRequest, type BuildSession, type SavedSession } from '../../hooks/useBuildSession';
import { copySection } from '../../lib/compare';
import { configValueDiffers } from '../../lib/configDefaults';
import { ConfigPanel, OptionRow } from './ConfigPanel';
import { UiProvider } from '../shared/UiContext';
import { TooltipProvider } from '../ui/tooltip';
import { RawConfigValue } from './RawConfigValue';
import { ConfigScopeControl, type ConfigScope } from './ConfigScope';

vi.mock('../../api/backend', () => ({ getBackend: vi.fn() }));
// Server rendering mounts only the active tab; these tests read the Advanced tab too.
vi.mock('../ui/tabs', async importOriginal => ({
  ...await importOriginal<typeof import('../ui/tabs')>(),
  TabsContent: ({ children }: { children?: ReactNode }) => children,
}));

const state = (config_inputs: Record<string, ConfigInputValue>): SavedSession['state'] => ({
  pobCode: 'source-code', sourceBackfillComplete: true,
  character: { class_name: 'Witch', ascendancy_name: '', level: 85 },
  allocatedNodes: [], attributeChoices: {}, socketGroups: [], items: [], flasks: [], limbs: [],
  jewels: [], annotations: {}, params: { config_inputs },
});

// SSR renders the advanced inputs before the asynchronous catalog arrives.
// Exercise the real panel and request together; the browser spec covers loaded controls.
function render(current: SavedSession['state'], imported: Record<string, ConfigInputValue>) {
  const session = { build: { config_inputs: imported }, calcParams: current.params,
    calc: null, busy: false, stateVersion: 1 } as BuildSession;
  return renderToStaticMarkup(<ConfigPanel session={session} lang="en-US" />);
}

// The raw editor shows a boolean as a select trigger labelled with its value, anything else as an input.
function input(html: string, key: string) {
  return html.match(new RegExp(`<(?:input|button)[^>]*aria-label="${key}"[^>]*>(?:<span[^>]*>[^<]*</span>)?`))?.[0];
}

describe('materialized configuration display', () => {
  test.each([true, false, 0, 20, '20', ''] as const)('a copied absence replaces imported %j in both the request and display', value => {
    const original = state({ conditionEnemyShocked: value, retainedFalse: false, retainedZero: 0 });
    const next = copySection(original, state({}), { kind: 'config', key: 'conditionEnemyShocked' });
    const html = render(next, original.params.config_inputs);
    expect(toRequest(next).config_inputs).toEqual({ retainedFalse: false, retainedZero: 0 });
    expect(input(html, 'conditionEnemyShocked')).toContain('value=""');
    // Clearing a source value is an edit, even when its display is blank.
    expect(html).toContain('aria-label="Restore imported/default value: conditionEnemyShocked"');
    expect(input(html, 'retainedFalse')).toContain('>False</span>');
    expect(input(html, 'retainedZero')).toContain('value="0"');
    expect(html).not.toContain('aria-label="Restore imported/default value: retainedFalse"');
  });

  test('reloading a copied absence keeps the source only as its reset baseline', () => {
    const imported = { conditionEnemyShocked: true };
    const copied = copySection(state(imported), state({}), { kind: 'config', key: 'conditionEnemyShocked' });
    const restored = parseSaved(JSON.stringify({ version: 1, notes: '', state: copied }))!;
    const current = backfillFromDecoded(restored.state, { config_inputs: imported } as unknown as BuildJson);
    expect(toRequest(current).config_inputs).not.toHaveProperty('conditionEnemyShocked');
    expect(input(render(current, imported), 'conditionEnemyShocked')).toContain('value=""');
    expect(render(current, imported)).toContain('aria-label="Restore imported/default value: conditionEnemyShocked"');
  });

  test('a selected named config establishes its own values and reset baseline', () => {
    const original = state({ conditionEnemyShocked: true, absentInSelectedSet: 20 });
    const decoded = { code: 'selected-code', config_inputs: { conditionEnemyShocked: false, nativeZero: 0 } } as unknown as BuildJson & { code: string };
    const selected = applyNamedConfig(original, decoded);
    const html = render(selected, decoded.config_inputs);
    expect(toRequest(selected).config_inputs).toEqual(decoded.config_inputs);
    expect(input(html, 'conditionEnemyShocked')).toContain('>False</span>');
    expect(input(html, 'nativeZero')).toContain('value="0"');
    expect(input(html, 'absentInSelectedSet')).toBeUndefined();
    expect(html).not.toContain('class="config-reset"');
  });
});

test('configuration exclusions use the shared modifier list with one original-text disclosure', () => {
  const lines = ['Config: First unknown effect', 'Config: Second unknown effect'];
  const session = { calcParams: { config_inputs: {} }, items: [], busy: false, stateVersion: 1,
    calc: { stats: [], main_skill: null, item_errors: [], breakdowns: {}, unsupported_modifiers: [...lines, 'Tree:123: Unrelated effect'] },
  } as unknown as BuildSession;
  const html = renderToStaticMarkup(<UiProvider lang="en-US"><TooltipProvider>
    <ConfigPanel session={session} lang="en-US" />
  </TooltipProvider></UiProvider>);
  expect(html).toContain('Modifiers not included');
  expect(html).toContain('class="unincluded-modifiers"');
  expect(html.match(/class="unincluded-raw"/g)).toHaveLength(1);
  expect(html).toContain('<pre>First unknown effect\nSecond unknown effect</pre>');
  expect(html).not.toContain('Config: ');
  expect(html).not.toContain('class="modifier-raw"');
  expect(html).not.toContain('Unrelated effect');
});

test('keeps a saved inapplicable option, typed value and collapsed help', () => {
  const html = renderToStaticMarkup(<UiProvider lang="en-US"><TooltipProvider>
    <OptionRow option={{ var: 'testOption', label: 'Test option', input_type: 'count', tooltip: 'Catalog help' }}
      value="0" overridden={false} busy={false} lang="en-US" listLabels={{}} applicability="not_applicable"
      onChange={vi.fn()} onReset={vi.fn()} />
  </TooltipProvider></UiProvider>);
  expect(html).toContain('Saved value kept visible');
  expect(html).toContain('<details class="config-option-help">');
  expect(html).toContain('Text: <code>&quot;0&quot;</code>');
  expect(html).toContain('value="0"');
  expect(html).not.toContain(' title=');
});

test('an empty enemy field identifies its inherited tier without showing a catalog example as the value', () => {
  const html = renderToStaticMarkup(<UiProvider lang="en-US"><TooltipProvider>
    <OptionRow option={{ var: 'enemyFireResist', label: 'Enemy fire resistance', input_type: 'count', default: { placeholder_number: 75 } }}
      value={undefined} inheritedLabel="Inherited from Pinnacle Boss" overridden={false} busy={false}
      lang="en-US" listLabels={{}} applicability="applicable" onChange={vi.fn()} onReset={vi.fn()} />
  </TooltipProvider></UiProvider>);
  expect(html).toContain('placeholder="Inherited from Pinnacle Boss"');
  expect(html).not.toContain('placeholder="75"');
  expect(html).toContain('value=""');
  expect(html).toContain('<summary>Help</summary>');
  expect(html).not.toContain('<details class="config-option-help" open');
});

test.each([
  ['en-US', 'Inherited from Pinnacle Boss', '50 (Inherited from Pinnacle Boss)'],
  ['zh-CN', '继承自巅峰 Boss', '50（继承自巅峰 Boss）'],
  ['zh-TW', '繼承自巔峰頭目', '50（繼承自巔峰頭目）'],
] as const)('an empty resistance field shows the actual inherited number and source in %s', (lang, source, placeholder) => {
  const html = renderToStaticMarkup(<UiProvider lang={lang}><TooltipProvider>
    <OptionRow option={{ var: 'enemyFireResist', label: 'Enemy fire resistance', input_type: 'count', default: { placeholder_number: 75 } }}
      value={undefined} inheritedValue={50} inheritedLabel={source} overridden={false} busy={false}
      lang={lang} listLabels={{}} applicability="applicable" onChange={vi.fn()} onReset={vi.fn()} />
  </TooltipProvider></UiProvider>);
  expect(html).toContain(`placeholder="${placeholder}"`);
  expect(html).toContain('value=""');
  expect(html).not.toContain('placeholder="75"');
});

test.each([undefined, false, 0, '0', ''] as const)('raw editor retains the type of %j', value => {
  const html = renderToStaticMarkup(<UiProvider lang="en-US">
    <RawConfigValue name="raw" value={value} lang="en-US" onChange={vi.fn()} />
  </UiProvider>);
  expect(html).toContain('aria-label="Value type: raw"');
  if (typeof value === 'boolean') expect(html.match(/role="combobox"/g)).toHaveLength(2);
  else {
    expect(html).toContain(`type="${typeof value === 'number' ? 'number' : 'text'}"`);
    expect(html).toContain(`value="${value ?? ''}"`);
    if (value === undefined) expect(html).toContain('disabled=""');
  }
});


test.each([true, false])('catalog row removes reset after returning to default %s', baseline => {
  const option = { var: 'flag', label: 'Flag', input_type: 'check' as const, default: { state_bool: baseline } };
  for (const current of [undefined, !baseline, baseline]) {
    const html = renderToStaticMarkup(<UiProvider lang="en-US"><TooltipProvider>
      <OptionRow option={option} value={current} overridden={configValueDiffers(option, current, undefined)}
        busy={false} lang="en-US" listLabels={{}} applicability="applicable" onChange={vi.fn()} onReset={vi.fn()} />
    </TooltipProvider></UiProvider>);
    expect(html.includes('aria-label="Restore imported/default value: Flag"')).toBe(current === !baseline);
    expect(html.includes('is-overridden')).toBe(current === !baseline);
  }
});

test.each(['relevant', 'modified', 'all'] as ConfigScope[])('configuration scope %s has exactly one checked radio', scope => {
  const html = renderToStaticMarkup(<ConfigScopeControl scope={scope} count={2} lang="en-US" onChange={vi.fn()} />);
  const radios = (html.match(/<button[^>]*>/g) ?? []).filter(button => button.includes('role="radio"'));
  expect(radios).toHaveLength(3);
  expect(radios.filter(button => button.includes('aria-checked="true"'))).toHaveLength(1);
  expect(html).toContain('role="radiogroup"');
  expect(html).toContain('Relevant to this build');
  expect(html).toContain('Modified (2)');
});

test('configuration exclusions retain player text in one shared original-text disclosure', () => {
  const session = { calcParams: { config_inputs: {} }, calc: {
    unsupported_modifiers: ['Config: First unknown effect', 'Config: Second unknown effect', 'Tree:12: Other effect'],
  }, busy: false, stateVersion: 1 } as unknown as BuildSession;
  const html = renderToStaticMarkup(<ConfigPanel session={session} lang="zh-CN" />);
  expect(html).toContain('class="unincluded-modifiers"');
  expect(html.match(/<summary>词缀原文<\/summary>/g)).toHaveLength(1);
  expect(html).toContain('<pre>First unknown effect\nSecond unknown effect</pre>');
  expect(html).not.toContain('Config: ');
  expect(html).not.toContain('Other effect');
  expect(html).not.toContain('class="modifier-raw"');
});
