import { expect, test } from '@playwright/test';

test('find a unique by modifier, choose a variant and rolls, then save and equip its preview', async ({ page }, testInfo) => {
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 85, class_name: 'Sorceress' },
    equipments: [{ inventoryId: 'Ring', x: 0, baseType: 'Sapphire Ring', name: 'Current Ring', frameType: 2,
      explicitMods: ['+20 to maximum Life'] }],
    talent_tree: { hashes: [], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: 'Fireball', support: false, properties: [{ type: 5, values: [['16', 0]] }] }],
  } }));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticUniqueFixture');
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 30_000 });
  const life = page.locator('.stat-row').filter({ hasText: /^Life(?=[0-9\s])/ }).locator('dd');
  const before = await life.innerText();
  await page.getByRole('button', { name: 'Unique catalogue', exact: true }).click();
  const catalogue = page.getByRole('region', { name: 'Unique catalogue', exact: true });
  await catalogue.getByRole('searchbox', { name: 'Search unique names and modifiers' }).fill('all attributes');
  await catalogue.getByRole('button', { name: 'Item class', exact: true }).click();
  await catalogue.getByRole('option', { name: 'Amulet', exact: true }).click();
  await catalogue.getByRole('button', { name: 'Equipment slot', exact: true }).click();
  await catalogue.getByRole('option', { name: 'Amulet', exact: true }).click();
  await catalogue.getByRole('button', { name: 'Choose a unique', exact: true }).click();
  await catalogue.getByRole('option', { name: /^Astramentis/ }).click();
  await expect(catalogue).toContainText('Rolls default to the midpoint');
  await catalogue.getByRole('button', { name: 'Variant', exact: true }).click();
  await catalogue.getByRole('option', { name: 'Pre 0.2.0', exact: true }).click();
  await catalogue.getByRole('button', { name: 'All maximum rolls', exact: true }).click();
  await page.setViewportSize({ width: 1280, height: 1100 });
  await catalogue.screenshot({ path: testInfo.outputPath('unique-selection.png') });
  await catalogue.getByRole('button', { name: 'Preview unique', exact: true }).click();
  const text = catalogue.getByRole('textbox', { name: 'Complete item text' });
  await expect(text).toHaveValue(/Selected Variant: 1/);
  await expect(text).toHaveValue(/\+100 to all Attributes/);
  await expect(catalogue.getByRole('button', { name: 'Apply to build · Amulet', exact: true })).toBeVisible();
  await expect(life).toHaveText(before);
  const expectedLife = await catalogue.locator('.upgrade-replacement-result tbody tr').filter({ hasText: /^Life(?=[0-9\s])/ }).locator('td').nth(1).innerText();
  await catalogue.getByRole('button', { name: 'Save to library', exact: true }).click();
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-library')!).items.at(-1));
  expect(saved).toMatchObject({ kind: 'item', slot: 'amulet' });
  expect(saved.text).toContain('Astramentis');
  expect(saved.text).toContain('+100 to all Attributes');
  await expect(life).toHaveText(before);
  await page.screenshot({ path: testInfo.outputPath('unique-preview.png'), fullPage: true });
  await catalogue.getByRole('button', { name: 'Apply to build · Amulet', exact: true }).click();
  await expect(life).not.toHaveText(before);
  expect(Number((await life.innerText()).replaceAll(',', ''))).toBeCloseTo(Number(expectedLife.replaceAll(',', '')), 0);
  await page.setViewportSize({ width: 320, height: 900 });
  await page.addStyleTag({ content: 'html { scrollbar-gutter: stable; }' });
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});
