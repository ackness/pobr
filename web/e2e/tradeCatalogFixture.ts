import type { Page } from '@playwright/test';
import { createHash } from 'node:crypto';
import type { TradeCatalog } from '../src/lib/tradeOptimizer';

/** Keep the bounded market pool in the same verified snapshot as the real WASM data. */
export async function installTradeCatalogFixture(page: Page, bound: (catalog: TradeCatalog) => void): Promise<void> {
  const indexResponse = await page.request.get('/data/manifest.json');
  if (!indexResponse.ok()) throw new Error(`Data index request failed: ${indexResponse.status()}`);
  const { version } = await indexResponse.json() as { version: string };
  const [catalogResponse, manifestResponse] = await Promise.all([
    page.request.get(`/data/${version}/overlay/trade_catalog.json`),
    page.request.get(`/data/${version}/manifest.json`),
  ]);
  if (!catalogResponse.ok()) throw new Error(`Trade catalog request failed: ${catalogResponse.status()}`);
  if (!manifestResponse.ok()) throw new Error(`Snapshot manifest request failed: ${manifestResponse.status()}`);

  const catalog = await catalogResponse.json() as TradeCatalog;
  const manifest = await manifestResponse.json() as { files: Record<string, string> };
  if (!manifest.files?.['overlay/trade_catalog.json']) throw new Error('Snapshot manifest lacks the trade catalog hash');
  bound(catalog);
  const catalogBody = JSON.stringify(catalog);
  manifest.files['overlay/trade_catalog.json'] = createHash('sha256').update(catalogBody).digest('hex');

  await page.route(`**/data/${version}/overlay/trade_catalog.json`, route =>
    route.fulfill({ contentType: 'application/json', body: catalogBody }));
  await page.route(`**/data/${version}/manifest.json`, route =>
    route.fulfill({ contentType: 'application/json', body: JSON.stringify(manifest) }));
}
