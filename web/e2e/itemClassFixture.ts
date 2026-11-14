import { createHash } from 'node:crypto';
import type { Page } from '@playwright/test';
import type { NativeDisplayTexts } from '../src/lib/nativeDisplayTexts';
import type { DataManifest } from '../src/api/dataPack';

/** Extend the existing verified snapshot for one display fixture; keep every other locale scope. */
export async function installItemClassFixture(page: Page, member: unknown | undefined,
  records: NativeDisplayTexts['scopes'][string]) {
  const index = await (await page.request.get('/data/manifest.json')).json() as DataManifest;
  if (index.schema !== undefined) throw new Error('Item class fixture requires the ordinary snapshot transport');
  const { version } = index;
  const manifest = await (await page.request.get(`/data/${version}/manifest.json`)).json() as {
    files: Record<string, string>; domains: { overlay: string[] };
  };
  const localePath = 'i18n/zh-TW/display_texts.json';
  const display = await (await page.request.get(`/data/${version}/${localePath}`)).json() as NativeDisplayTexts;
  display.scopes['ItemClasses.Name'] = records;
  const contents: Record<string, string> = { [localePath]: JSON.stringify(display) };
  if (member !== undefined) {
    contents['overlay/item_class_names.json'] = typeof member === 'string' ? member : JSON.stringify(member);
    manifest.domains.overlay.push('item_class_names');
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
