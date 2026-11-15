import { createHash } from 'node:crypto';
import type { Page } from '@playwright/test';
import type { NativeDisplayTexts } from '../src/lib/nativeDisplayTexts';
import type { DataManifest } from '../src/api/dataPack';

/** Replace native class names, or omit the member and its locale scopes for the legacy path. */
export async function installItemClassFixture(page: Page, member: unknown | undefined,
  records: NativeDisplayTexts['scopes'][string]) {
  const index = await (await page.request.get('/data/manifest.json')).json() as DataManifest;
  if (index.schema !== undefined) throw new Error('Item class fixture requires the ordinary snapshot transport');
  const { version } = index;
  const manifest = await (await page.request.get(`/data/${version}/manifest.json`)).json() as {
    files: Record<string, string>; domains: { overlay: string[] };
  };
  const contents: Record<string, string> = {};
  for (const locale of member === undefined ? ['zh-CN', 'zh-TW'] : ['zh-TW']) {
    const localePath = `i18n/${locale}/display_texts.json`;
    const display = await (await page.request.get(`/data/${version}/${localePath}`)).json() as NativeDisplayTexts;
    if (member === undefined) {
      delete display.scopes['ItemClasses.Name'];
      if (display.origins?.scopes) delete display.origins.scopes['ItemClasses.Name'];
    } else {
      display.scopes['ItemClasses.Name'] = records;
    }
    contents[localePath] = JSON.stringify(display);
  }
  const memberPath = 'overlay/item_class_names.json';
  manifest.domains.overlay = manifest.domains.overlay.filter(domain => domain !== 'item_class_names');
  if (member !== undefined) {
    contents[memberPath] = typeof member === 'string' ? member : JSON.stringify(member);
    manifest.domains.overlay.push('item_class_names');
  } else {
    delete manifest.files[memberPath];
    index.files = index.files.filter(path => path !== memberPath);
    if (index.digests) delete index.digests[memberPath];
  }
  const sha = (body: string) => createHash('sha256').update(body).digest('hex');
  for (const [path, body] of Object.entries(contents)) manifest.files[path] = sha(body);
  contents['manifest.json'] = JSON.stringify(manifest);
  index.files = [...new Set([...index.files, ...Object.keys(contents)])].sort();
  index.digests = { ...index.digests, ...Object.fromEntries(Object.entries(contents).map(([path, body]) => [path, sha(body)])) };
  await page.route('**/data/manifest.json', route => route.fulfill({ json: index }));
  for (const [path, body] of Object.entries(contents)) {
    for (const url of [`**/data/${version}/${path}`, `**/data/h/${sha(body).slice(0, 16)}/${path}`]) {
      await page.route(url, route => route.fulfill({ contentType: 'application/json', body }));
    }
  }
}
