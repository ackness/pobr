import { expect, test } from '@playwright/test';
import { deflateSync, inflateSync } from 'node:zlib';

const code = deflateSync(`<PathOfBuilding2><Build level="30" className="Witch" mainSocketGroup="1"/>
<Tree activeSpec="1"><Spec title="Mapping" nodes="" treeVersion="0_5"/><Spec title="Boss" nodes="" treeVersion="0_5"/></Tree>
<Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true"><Gem skillId="SparkPlayer" level="5" enabled="true"/></Skill></SkillSet></Skills>
<Config activeConfigSet="7"><ConfigSet id="7" title="Mapping"><Input name="ActiveBrands" number="0"/><Input name="futureInput" string="keep"/></ConfigSet><ConfigSet id="3" title="Boss"><Input name="ActiveBrands" number="2"/></ConfigSet></Config></PathOfBuilding2>`).toString('base64url');

test('config sets save edits independently and survive copy, export, reload and reimport', async ({ page }, testInfo) => {
  await page.goto('/');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill(code);
  await page.locator('.import-submit').click();
  await expect(page.getByRole('heading', { name: 'Items', exact: true })).toBeVisible({ timeout: 60_000 });
  await page.getByRole('button', { name: 'Config', exact: true }).click();
  const sets = page.getByRole('region', { name: 'Config set', exact: true });
  const selector = sets.getByRole('combobox');
  await expect(selector).toHaveValue('7');
  await expect(selector).toBeEnabled();
  await sets.getByRole('button', { name: 'Copy set', exact: true }).click();
  await sets.getByRole('textbox', { name: 'Set name' }).fill('Copy');
  await sets.getByRole('button', { name: 'Save name' }).click();
  await expect(selector).toHaveValue('8');
  await page.getByLabel('Search config options…').fill('ActiveBrands');
  const brands = page.locator('[data-config-var="ActiveBrands"] input');
  await expect(brands).toBeEnabled();
  await brands.fill('5');
  await brands.blur();
  await expect(selector).toBeEnabled();
  await selector.selectOption('7');
  await expect(brands).toHaveValue('0');
  await expect(selector).toBeEnabled();
  await selector.selectOption('8');
  await expect(brands).toHaveValue('5');
  await expect(selector).toBeEnabled();
  await sets.scrollIntoViewIfNeeded();
  await page.screenshot({ path: testInfo.outputPath('config-sets.png'), fullPage: true });
  await page.reload();
  await expect(selector).toHaveValue('8', { timeout: 90_000 });
  await page.getByRole('button', { name: 'Build', exact: true }).click();
  await page.getByRole('button', { name: 'Generate share code', exact: true }).click();
  const exported = await page.getByRole('textbox', { name: 'Share Code', exact: true }).inputValue();
  const xml = inflateSync(Buffer.from(exported, 'base64url')).toString('utf8');
  expect(xml.match(/<ConfigSet /g)).toHaveLength(3);
  expect(xml).toContain('activeConfigSet="8"');
  expect(xml.match(/name="futureInput" string="keep"/g)).toHaveLength(2);
  await page.getByRole('textbox', { name: 'Build code' }).fill(exported);
  await page.locator('.import-submit').click();
  await expect(page.getByRole('heading', { name: 'Items', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Config', exact: true }).click();
  await expect(selector).toHaveValue('8');
  await page.getByLabel('Search config options…').fill('ActiveBrands');
  await expect(brands).toHaveValue('5');
  await page.getByRole('button', { name: 'Build', exact: true }).click();
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('30');
  await page.locator('.workspace-loadouts summary').click();
  const loadout = page.getByRole('combobox', { name: 'PoB loadout', exact: true });
  await expect(loadout).toBeEnabled();
  await loadout.selectOption('1');
  await page.getByRole('button', { name: 'Config', exact: true }).click();
  await expect(selector).toHaveValue('3');
  await page.getByLabel('Search config options…').fill('ActiveBrands');
  await expect(brands).toHaveValue('2');
});
