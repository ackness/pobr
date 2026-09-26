import { renderToStaticMarkup } from 'react-dom/server';
import { readFileSync } from 'node:fs';
import { describe, expect, it, vi } from 'vitest';
import { TopBar } from './TopBar';
import type { Lang } from '../../lib/i18n';

describe('TopBar versions', () => {
  it('uses the Cargo workspace version', () => {
    const cargo = readFileSync(new URL('../../../../Cargo.toml', import.meta.url), 'utf8');
    const workspaceSection = cargo.match(/\[workspace\.package\]([\s\S]*?)(?=\n\[|$)/)?.[1];
    const version = workspaceSection?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
    expect(__POBR_APP_VERSION__).toBe(version);
  });

  it.each([
    ['en-US', 'Data'],
    ['zh-TW', '資料'],
    ['zh-CN', '数据'],
  ] as const)('shows the app and loaded data versions in %s', (lang: Lang, dataLabel) => {
    const html = renderToStaticMarkup(
      <TopBar
        tab="build"
        onTab={vi.fn()}
        lang={lang}
        onLang={vi.fn()}
        character={null}
        classNames={{ classes: {}, ascendancies: {} }}
        busy={false}
        dataVersion="4.5.5.2"
      />,
    );
    expect(html).toContain(`v${__POBR_APP_VERSION__}`);
    expect(html).toContain(`${dataLabel} 4.5.5.2`);
  });
});
