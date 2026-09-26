import { expect, test } from '@playwright/test';
import { deflateSync } from 'node:zlib';
import { installTradeCatalogFixture } from './tradeCatalogFixture';

const code = deflateSync(`<PathOfBuilding2>
  <Build level="70" className="Witch"/>
  <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
  <Skills/>
  <Items activeItemSet="1">
    <Item id="1">Rarity: RARE
Reference Ring
Sapphire Ring
Implicits: 0
+50 to maximum Life</Item>
    <ItemSet id="1"><Slot name="Ring 1" itemId="1"/></ItemSet>
  </Items>
</PathOfBuilding2>`).toString('base64url');

test('completed trade analysis survives market changes and resets for a new goal', async ({ page }) => {
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await installTradeCatalogFixture(page, catalog => {
    catalog.mods = catalog.mods.filter((mod: { lines: string[] }) =>
      mod.lines.some(line => /to maximum Life|to Fire Resistance/.test(line))).slice(0, 10);
  });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible();
  await page.getByRole('button', { name: 'Upgrades', exact: true }).click();
  await page.getByRole('button', { name: 'Analyze all positions', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Reanalyze all positions', exact: true })).toBeEnabled({ timeout: 90_000 });
  await expect(page.locator('.trade-priorities .ui-badge')).toContainText('1 / 1');

  await page.getByRole('spinbutton', { name: 'Max price' }).fill('50');
  await expect(page.getByRole('button', { name: 'Reanalyze all positions', exact: true })).toBeEnabled();
  await expect(page.locator('.trade-priorities .ui-badge')).toContainText('1 / 1');

  await page.getByRole('button', { name: 'Max Life', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Analyze all positions', exact: true })).toBeEnabled();
  await expect(page.locator('.trade-priorities .ui-badge')).toHaveCount(0);
});
