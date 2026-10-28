import { renderToStaticMarkup } from 'react-dom/server';
import { expect, test } from 'vitest';
import type { BuildSession } from '../../../hooks/useBuildSession';
import { CalcsPanel } from '../../../components/calcs/CalcsPanel';
import { UiProvider } from '../../../components/shared/UiContext';
import { TooltipProvider } from '../../../components/ui/tooltip';
import { LedgerView } from './LedgerView';
import { mockLineUsage } from './mockLineUsage';
import { summarise, STATUS_ORDER } from './ledger';

const session = { calc: { stats: [{ id: 'Life', category: 'Resource', value: 100 }], breakdowns: {}, breakdown_descriptors: {}, minions: [] },
  socketGroups: [], calcParams: {}, stateVersion: 1, busy: false } as unknown as BuildSession;

const render = (node: React.ReactNode) => renderToStaticMarkup(<UiProvider v2 lang="en-US"><TooltipProvider>{node}</TooltipProvider></UiProvider>);

test('the ledger opens on one call to action and does not run the slow check by itself', () => {
  const html = render(<LedgerView session={session} lang="en-US" />);
  expect(html).toContain('id="ledger-heading"');
  expect(html).toContain('Check lines');
  expect(html).toContain('Check the lines to see what became of each one.');
  expect(html).not.toContain('ledger-list');
  expect(html).not.toContain('role="alert"');
});

test.each([
  ['zh-CN', '检查词条', '词条账本'],
  ['zh-TW', '檢查詞綴', '詞綴帳本'],
] as const)('the ledger speaks %s', (lang, action, title) => {
  const html = renderToStaticMarkup(<UiProvider v2 lang={lang}><TooltipProvider><LedgerView session={session} lang={lang} /></TooltipProvider></UiProvider>);
  expect(html).toContain(action);
  expect(html).toContain(title);
});

test('the check button is disabled until there is a calculation to read', () => {
  const html = render(<LedgerView session={{ ...session, calc: null } as unknown as BuildSession} lang="en-US" />);
  expect(html).toMatch(/<button[^>]*disabled[^>]*>Check lines<\/button>/);
});

test('Calcs adds a Ledger tab on the v2 page only', () => {
  const v2 = render(<CalcsPanel session={session} lang="en-US" />);
  expect(v2).toContain('role="tab"');
  expect(v2).toContain('>Ledger<');
  const legacy = renderToStaticMarkup(<UiProvider lang="en-US"><TooltipProvider><CalcsPanel session={session} lang="en-US" /></TooltipProvider></UiProvider>);
  expect(legacy).not.toContain('Ledger');
  expect(legacy).not.toContain('role="tab"');
});

test('the development mock is a realistic ledger: every status appears and the counts agree with the lines', () => {
  const { usage } = mockLineUsage();
  const summary = summarise(usage);
  for (const status of STATUS_ORDER) expect(summary.counts[status], status).toBeGreaterThan(0);
  expect(usage.summary.lines).toBe(usage.lines.length);
  expect(usage.summary.lines_applied).toBe(summary.counts.applied);
  expect(usage.summary.lines_unparsed).toBe(summary.counts.unparsed);
  expect(usage.unparsed.length).toBe(summary.counts.unparsed);
});
