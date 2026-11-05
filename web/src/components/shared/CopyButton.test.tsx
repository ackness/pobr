import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, expect, it, vi } from 'vitest';
import { CopyButton } from './CopyButton';

// Capture the real copy handler and replay its state changes in the SSR test environment.
const state = vi.hoisted(() => ({ status: 'idle', click: undefined as ComponentProps<'button'>['onClick'] }));
vi.mock('react', async importOriginal => {
  const react = await importOriginal<typeof import('react')>();
  return { ...react, useState: () => [state.status, (status: string) => { state.status = status; }] };
});
vi.mock('./controls', () => ({
  AppButton: ({ children, onClick, ...props }: ComponentProps<'button'>) => {
    state.click = onClick;
    return <button {...props}>{children}</button>;
  },
  AppTextarea: (props: ComponentProps<'textarea'>) => <textarea {...props} />,
}));

afterEach(() => {
  state.status = 'idle';
  state.click = undefined;
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

it.each([
  ['en-US', 'Fireball', 'Copied'],
  ['zh-CN', '火球术', '已复制'],
  ['zh-TW', '火球術', '已複製'],
] as const)('announces a successful compact copy in %s and clears it after reset', async (lang, text, copied) => {
  vi.useFakeTimers();
  const writeText = vi.fn().mockResolvedValue(undefined);
  vi.stubGlobal('navigator', { clipboard: { writeText } });
  const render = () => renderToStaticMarkup(<CopyButton compact text={text} lang={lang} />);
  const emptyRegion = '<span class="sr-only" aria-live="polite"></span>';
  expect(render()).toContain(emptyRegion);

  await state.click!({ stopPropagation: vi.fn() } as unknown as Parameters<NonNullable<typeof state.click>>[0]);
  expect(writeText).toHaveBeenCalledWith(text);
  expect(render()).toContain(`<span class="sr-only" aria-live="polite">${copied}</span>`);

  vi.advanceTimersByTime(1200);
  expect(render()).toContain(emptyRegion);
});
