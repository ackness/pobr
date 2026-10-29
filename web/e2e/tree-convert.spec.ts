import { expect, test, type Page } from '@playwright/test';
import { busy, openItemsAfterImport, pageLink } from './shell';
import { choose, generateShareCode } from './treePage';
import { readFileSync } from 'node:fs';
import { deflateSync, inflateSync } from 'node:zlib';
import type { SavedSession } from '../src/hooks/useBuildSession';

// Synthetic, connected 0_4 Ranger allocation. Construction and local topology
// cross-check are recorded in the shared fixture's XML comment; no player code.
const synthetic = readFileSync(new URL('../../crates/pobr-build/tests/fixtures/synthetic-tree-0_4.xml', import.meta.url), 'utf8');
const code = deflateSync(synthetic).toString('base64url');
const xml = (code: string) => inflateSync(Buffer.from(code, 'base64url')).toString();
const nav = (page: Page, name: string) => pageLink(page, name);
const saved = (page: Page): Promise<SavedSession['state']> => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state);
async function idle(page: Page) {
  await expect(busy(page)).toHaveCount(0);
  await expect(page.locator('.shell-save-state')).toContainText('Saved locally');
  await expect(page.locator('.shell-boot[role=alert], .shell-alert')).toHaveCount(0);
}
async function importCode(page: Page, value: string) {
  await page.locator('.shell-builds-button').click();
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(value);
  await page.locator('.import-submit').click();
  await openItemsAfterImport(page);
  await expect(page.getByRole('heading', { name: 'Items', exact: true })).toBeVisible();
  await idle(page);
}

test('synthetic historical conversion previews, confirms, undoes and exports each or all specs', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1100 });
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.goto('/build');
  await expect(page.getByRole('textbox', { name: 'Build code', exact: true })).toBeVisible({ timeout: 90_000 });
  await importCode(page, code);
  const before = await saved(page);
  expect(before.treeVersion).toBe('0_4');
  await nav(page, 'Tree').click();
  const convert = page.getByRole('button', { name: 'Convert to current tree', exact: true });
  const confirm = page.getByRole('button', { name: 'Confirm conversion', exact: true });
  const undo = page.getByRole('button', { name: 'Undo tree edit', exact: true });
  const preview = page.getByRole('region', { name: 'Conversion preview', exact: true });
  await convert.click();
  await expect(preview).toContainText('Kept 18 · Dropped 1 · Disconnected 1');
  await expect(preview).toContainText('Stun Threshold if no recent Stun');
  await expect(preview).toContainText("25% increased Stun Threshold if you haven't been Stunned Recently");
  await expect(preview).toContainText('Damage against Enemies on Low Life · #56118');
  await expect(preview).toContainText('Weapon set 2');
  expect(await saved(page)).toEqual(before);
  await page.screenshot({ path: test.info().outputPath('tree-conversion-preview.png') });
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(preview).toHaveCount(0);
  expect(await saved(page)).toEqual(before);
  await convert.click();
  await confirm.click();
  await idle(page);
  let current = await saved(page);
  expect(current.treeVersion).toBe('0_5');
  expect(current.allocatedNodes).not.toContain(8194);
  expect(current.weaponSwap?.exclusive_nodes).toEqual([[], [56118]]);
  expect(current.jewels.map(j => j.socket_node)).toEqual([60735]);
  expect(current.jewels[0].text.trim()).toBe(before.jewels[0].text.trim());
  expect(xml(current.pobCode!).match(/treeVersion="0_4"/g)).toHaveLength(1);
  await undo.click();
  await idle(page);
  expect(await saved(page)).toEqual(before);
  await page.getByRole('checkbox', { name: 'All passive specs', exact: true }).check();
  await convert.click();
  await expect(preview.locator('article')).toHaveCount(2);
  await confirm.click();
  await idle(page);
  current = await saved(page);
  expect(xml(current.pobCode!)).not.toContain('treeVersion="0_4"');
  await undo.click();
  await idle(page);
  expect(await saved(page)).toEqual(before);
  await page.getByRole('checkbox', { name: 'All passive specs', exact: true }).check();
  await convert.click();
  await confirm.click();
  await idle(page);
  const exported = await generateShareCode(page);
  expect(xml(exported)).not.toContain('treeVersion="0_4"');
  await importCode(page, exported);
  current = await saved(page);
  expect(current.treeVersion).toBe('0_5');
  expect(current.weaponSwap?.exclusive_nodes).toEqual([[], [56118]]);
  expect(current.jewels[0].text.trim()).toBe(before.jewels[0].text.trim());
  await page.reload();
  await idle(page);
  expect((await saved(page)).treeVersion).toBe('0_5');
  await nav(page, 'Tree').click();
  await convert.click();
  await expect(confirm).toBeEnabled();
  // Switching the active weapon set changes the request while its preview is open. The canvas is rebuilt
  // for the other set, but the preview stays: a stale confirmation cannot overwrite the changed session.
  await choose(page, page.getByRole('combobox', { name: 'Allocate to', exact: true }), 'Weapon set 2');
  await idle(page);
  await expect(confirm).toBeDisabled();
  await expect(preview).toContainText('The build changed. Create a new preview.');
  expect((await saved(page)).weaponSwap?.active).toBe(2);
  expect((await saved(page)).allocatedNodes).toContain(56118);
  expect((await saved(page)).treeVersion).toBe('0_5');
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(preview).toHaveCount(0);
  // Explicitly refund the disconnected leaf through the normal tree editor; it stays allocated
  // until this action, and ordinary tree Undo restores it.
  await page.locator('circle[data-skill-id="56118"]').dispatchEvent('click', { clientX: 600, clientY: 400 });
  await expect.poll(async () => (await saved(page)).weaponSwap?.exclusive_nodes[1]).toEqual([]);
  await idle(page);
  await undo.click();
  await idle(page);
  expect((await saved(page)).weaponSwap?.exclusive_nodes[1]).toEqual([56118]);
});
