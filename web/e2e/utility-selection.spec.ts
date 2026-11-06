import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { deflateSync, inflateSync } from 'node:zlib';
import { importAndOpen, shareCode } from './skillsAndCalcs';
import { busy } from './shell';

const xml = readFileSync(new URL('../../apps/pobr-wasm/tests/fixtures/utility-selection.xml', import.meta.url), 'utf8');
const code = deflateSync(xml).toString('base64url');
const utilities = (page: Page) => page.evaluate(() =>
  (JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.flasks as { slot: string; text: string; active: boolean }[]));
const selected = (page: Page) => page.getByRole('checkbox', { name: 'Selected for use', exact: true });
const openSlot = async (page: Page, slot: string) => {
  // Flasks and charms sit in their own group beside the figure (`.paper-doll` is only the body).
  await page.locator('.doll').getByRole('button', { name: slot, exact: true }).click();
  await expect(selected(page)).toBeEnabled();
};

test('unselected utilities survive import, text editing, undo, reload and share-code export', async ({ page }) => {
  await importAndOpen(page, code);
  await openSlot(page, 'Flask 1');
  await expect(selected(page)).not.toBeChecked();
  await expect(page.locator('#utility-selection-hint')).toContainText('unselected flask');
  const original = (await utilities(page)).find(item => item.slot === 'Flask 1')!;
  expect(original.active).toBe(false);

  // Keyboard operation changes selection; undo restores the equipped item and its flag.
  await selected(page).focus();
  await page.keyboard.press('Space');
  await expect.poll(async () => (await utilities(page)).find(item => item.slot === 'Flask 1')?.active).toBe(true);
  await expect(busy(page)).toHaveCount(0);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(selected(page)).not.toBeChecked();
  await expect(busy(page)).toHaveCount(0);

  await page.locator('.item-detail-header').getByRole('button', { name: 'Edit text (advanced)', exact: true }).click();
  const text = page.getByRole('textbox', { name: 'Flask 1 item text', exact: true });
  await text.fill(original.text.replace('+40%', '+12%'));
  await page.getByRole('button', { name: 'Apply changes', exact: true }).click();
  await expect.poll(async () => (await utilities(page)).find(item => item.slot === 'Flask 1'))
    .toEqual({ ...original, text: original.text.replace('+40%', '+12%') });
  await expect(busy(page)).toHaveCount(0);
  await openSlot(page, 'Charm 1');
  await expect(selected(page)).not.toBeChecked();
  await page.reload();
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 90_000 });
  await expect(busy(page)).toHaveCount(0, { timeout: 90_000 });
  await openSlot(page, 'Flask 1');
  await expect(selected(page)).not.toBeChecked();

  const exported = await shareCode(page);
  const savedXml = inflateSync(Buffer.from(exported, 'base64url')).toString('utf8');
  expect(savedXml).toMatch(/<Slot name="Flask 1" itemId="\d+" active="false"\/>/);
  expect(savedXml).toMatch(/<Slot name="Charm 1" itemId="\d+" active="false"\/>/);
  expect(savedXml).toContain('+12% to Fire Resistance during effect');
  await importAndOpen(page, exported);
  await openSlot(page, 'Flask 1');
  await expect(selected(page)).not.toBeChecked();
  await page.locator('.item-detail-header').getByRole('button', { name: 'Remove', exact: true }).click();
  await expect.poll(async () => (await utilities(page)).some(item => item.slot === 'Flask 1')).toBe(false);
  await expect(busy(page)).toHaveCount(0);
  await page.reload();
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 90_000 });
  await expect(busy(page)).toHaveCount(0, { timeout: 90_000 });
  expect((await utilities(page)).some(item => item.slot === 'Flask 1')).toBe(false);
  const clearedXml = inflateSync(Buffer.from(await shareCode(page), 'base64url')).toString('utf8');
  expect(clearedXml).not.toContain('<Slot name="Flask 1"');
  expect(clearedXml).toMatch(/<Slot name="Charm 1" itemId="\d+" active="false"\/>/);
});
