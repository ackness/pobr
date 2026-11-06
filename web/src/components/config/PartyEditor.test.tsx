import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, expect, test, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import { PartyEditor } from './PartyEditor';

// Replay the editor's code and importing states without a browser DOM.
const harness = vi.hoisted(() => ({ input: undefined as ComponentProps<'textarea'>['onChange'],
  importClick: undefined as ComponentProps<'button'>['onClick'],
  importMember: vi.fn(),
}));
const hooks = await vi.hoisted(async () => {
  const { createHookHarness } = await import('../../testing/hookHarness');
  return createHookHarness({ lazyState: false, functionalUpdates: false });
});
vi.mock('react', async original => hooks.mockReact(await original<typeof import('react')>()));
vi.mock('../../api/backend', () => ({ getBackend: async () => ({ importPartyMember: harness.importMember }) }));
vi.mock('../shared/controls', () => ({
  AppButton: (props: ComponentProps<'button'>) => {
    if (props['aria-describedby'] || String(props.children) === 'Import member') harness.importClick = props.onClick;
    return <button {...props} />;
  },
  AppInput: (props: ComponentProps<'input'>) => <input {...props} />,
  AppTextarea: (props: ComponentProps<'textarea'>) => { harness.input = props.onChange; return <textarea {...props} />; },
}));

afterEach(() => { hooks.reset(); vi.clearAllMocks(); });

test('party import explains the disabled state, enables for a code and disables again after import', async () => {
  const updateParams = vi.fn();
  const session = { calcParams: {}, stateVersion: 1, busy: false, updateParams } as unknown as BuildSession;
  const render = () => { hooks.beginRender(); return renderToStaticMarkup(<PartyEditor session={session} lang="en-US" />); };
  const empty = render();
  expect(empty).toContain('id="party-import-hint">Paste your teammate’s build code first');
  expect(empty).toContain('disabled="" aria-describedby="party-import-hint"');
  harness.input!({ target: { value: '  party-code  ' } } as never);
  const ready = render();
  expect(ready).not.toContain('party-import-hint');
  expect(ready).toContain('<button>Import member</button>');
  harness.importMember.mockResolvedValue({ name: 'Teammate', enabled: true, blocks: { Aura: '20% increased Damage' }, warnings: [] });
  await harness.importClick!({} as never);
  expect(harness.importMember).toHaveBeenCalledWith('party-code');
  expect(updateParams).toHaveBeenCalledWith({ party: { exports: {}, members: [expect.objectContaining({ name: 'Teammate', enabled: true })] } });
  expect(render()).toContain('disabled="" aria-describedby="party-import-hint"');
});
