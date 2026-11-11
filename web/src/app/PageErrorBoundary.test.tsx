import { act, lazy, Suspense, type ComponentProps } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { HostDocument, type HostNode } from '../testing/hostDom';
import { t } from '../lib/i18n';
import { PageErrorBoundary } from './PageErrorBoundary';

const ui = vi.hoisted(() => ({ buttons: [] as ComponentProps<'button'>[] }));
vi.mock('../components/ui/button', () => ({ Button: (props: ComponentProps<'button'>) => {
  ui.buttons.push(props); return <button {...props} />;
} }));

let root: Root, host: HostNode;
const textOf = (node: HostNode): string => node.textContent + node.childNodes.map(textOf).join('');
beforeEach(() => {
  const document = new HostDocument();
  vi.stubGlobal('document', document);
  vi.stubGlobal('window', document.defaultView);
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  host = document.createElement('div');
  root = createRoot(host as unknown as HTMLElement);
  ui.buttons = [];
  vi.spyOn(console, 'error').mockImplementation(() => {});
});
afterEach(async () => {
  await act(async () => root.unmount());
  vi.restoreAllMocks(); vi.unstubAllGlobals(); vi.useRealTimers();
});

function ThrowingChild(): never { throw new Error('Panel rendering failed'); }

test.each(['en-US', 'zh-CN', 'zh-TW'] as const)('a failed page keeps its sibling and offers a current backup in %s', async lang => {
  const exportWorkspace = vi.fn(() => '{"workspace":"current"}');
  await act(async () => root.render(<>
    <PageErrorBoundary lang={lang} exportWorkspace={exportWorkspace}><ThrowingChild /></PageErrorBoundary>
    <PageErrorBoundary lang={lang} exportWorkspace={exportWorkspace}><p>Other page</p></PageErrorBoundary>
  </>));
  expect(textOf(host)).toContain(t(lang, 'shell.pageError'));
  expect(textOf(host)).toContain('Panel rendering failed');
  expect(textOf(host)).toContain('Other page');
  expect(exportWorkspace).not.toHaveBeenCalled();
  const button = ui.buttons.find(button => Array.isArray(button.children) && button.children.includes(t(lang, 'shell.downloadBackup')))!;
  expect(button).toBeDefined();
  const link = { href: '', download: '', click: vi.fn() };
  const createUrl = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:backup');
  vi.spyOn(document, 'createElement').mockReturnValue(link as unknown as HTMLElement);
  vi.useFakeTimers();
  button.onClick!({} as never);
  expect(exportWorkspace).toHaveBeenCalledOnce();
  expect(await (createUrl.mock.calls[0][0] as Blob).text()).toBe('{"workspace":"current"}');
  expect(link.download).toBe('pobr-backup.json');
  expect(link.click).toHaveBeenCalledOnce();
});

test('a rejected lazy chunk reaches the page boundary through Suspense', async () => {
  let reject!: (error: Error) => void;
  const Page = lazy(() => new Promise<{ default: () => null }>((_resolve, fail) => { reject = fail; }));
  await act(async () => root.render(<PageErrorBoundary lang="en-US" exportWorkspace={() => '{}'}>
    <Suspense fallback={<p>Loading page</p>}><Page /></Suspense>
  </PageErrorBoundary>));
  expect(textOf(host)).toContain('Loading page');
  await act(async () => reject(new Error('Failed to fetch dynamically imported module')));
  expect(textOf(host)).toContain(t('en-US', 'shell.pageError'));
  expect(textOf(host)).toContain('Failed to fetch dynamically imported module');
  expect(textOf(host)).not.toContain('Loading page');
});
