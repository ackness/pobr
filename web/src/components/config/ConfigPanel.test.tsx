import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, test, vi } from 'vitest';
import type { BuildJson, ConfigInputValue } from '../../api/types';
import { applyNamedConfig, backfillFromDecoded, parseSaved, toRequest, type BuildSession, type SavedSession } from '../../hooks/useBuildSession';
import { copySection } from '../../lib/compare';
import { ConfigPanel, OptionRow } from './ConfigPanel';
import { UiProvider } from '../shared/UiContext';
import { TooltipProvider } from '../ui/tooltip';
import { RawConfigValue } from './RawConfigValue';

vi.mock('../../api/backend', () => ({ getBackend: vi.fn() }));

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

function input(html: string, key: string) {
  return html.match(new RegExp(`<input[^>]*aria-label="${key}"[^>]*>`))?.[0];
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
    expect(input(html, 'retainedFalse')).toContain('value="false"');
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
    expect(input(html, 'conditionEnemyShocked')).toContain('value="false"');
    expect(input(html, 'nativeZero')).toContain('value="0"');
    expect(input(html, 'absentInSelectedSet')).toBeUndefined();
    expect(html).not.toContain('class="config-reset"');
  });
});

test('V2 keeps a saved inapplicable option, typed value and collapsed help', () => {
  const html = renderToStaticMarkup(<UiProvider v2 lang="en-US"><TooltipProvider>
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

test.each([undefined, false, 0, '0', ''] as const)('V2 raw editor retains the type of %j', value => {
  const html = renderToStaticMarkup(<UiProvider v2 lang="en-US">
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
