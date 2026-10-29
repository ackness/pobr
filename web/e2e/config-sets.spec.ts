import { expect, test } from '@playwright/test';
import { choose, go, importCode, pickerValue, shareCode } from './configPage';
import { deflateSync, inflateSync } from 'node:zlib';

const code = deflateSync(`<PathOfBuilding2><Build level="30" className="Witch" mainSocketGroup="1"/>
<Tree activeSpec="1"><Spec title="Mapping" nodes="" treeVersion="0_5"/><Spec title="Boss" nodes="" treeVersion="0_5"/></Tree>
<Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true"><Gem skillId="SparkPlayer" level="5" enabled="true"/></Skill></SkillSet></Skills>
<Config activeConfigSet="7"><ConfigSet id="7" title="Mapping"><Input name="multiplierKilledRecently" number="0"/><Input name="futureInput" string="keep"/></ConfigSet><ConfigSet id="3" title="Boss"><Input name="multiplierKilledRecently" number="2"/></ConfigSet></Config></PathOfBuilding2>`).toString('base64url');

test('config sets save edits independently and survive copy, export, reload and reimport', async ({ page }, testInfo) => {
  await page.goto('/build');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await importCode(page, code);
  await expect(page.getByRole('heading', { name: 'Equipment', exact: true })).toBeVisible({ timeout: 60_000 });
  await go(page, 'Config');
  const sets = page.getByRole('region', { name: 'Config set', exact: true });
  const selector = sets.getByRole('combobox');
  // The picker shows set titles; the set numbers live in the hidden native select behind it.
  const selected = (id: string) => expect.poll(() => pickerValue(selector), { timeout: 90_000 }).toBe(id);
  await selected('7');
  await expect(selector).toBeEnabled();
  await sets.getByRole('button', { name: 'Config set actions', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Copy set', exact: true }).click();
  await sets.getByRole('textbox', { name: 'Set name' }).fill('Copy');
  await sets.getByRole('button', { name: 'Save name' }).click();
  await selected('8');
  await page.getByLabel('Search config options…').fill('multiplierKilledRecently');
  const killed = page.locator('[data-config-var="multiplierKilledRecently"] input');
  await expect(killed).toBeEnabled();
  await killed.fill('5');
  await killed.blur();
  await expect(selector).toBeEnabled();
  await choose(page, selector, 'Mapping');
  await selected('7');
  await expect(killed).toHaveValue('0');
  await expect(selector).toBeEnabled();
  await choose(page, selector, 'Copy');
  await selected('8');
  await expect(killed).toHaveValue('5');
  await expect(selector).toBeEnabled();
  await sets.scrollIntoViewIfNeeded();
  await page.screenshot({ path: testInfo.outputPath('config-sets.png'), fullPage: true });
  await page.reload();
  await selected('8');
  const exported = await shareCode(page);
  const xml = inflateSync(Buffer.from(exported, 'base64url')).toString('utf8');
  expect(xml.match(/<ConfigSet /g)).toHaveLength(3);
  expect(xml).toContain('activeConfigSet="8"');
  expect(xml.match(/name="futureInput" string="keep"/g)).toHaveLength(2);
  await importCode(page, exported);
  await expect(page.getByRole('heading', { name: 'Equipment', exact: true })).toBeVisible();
  await go(page, 'Config');
  await selected('8');
  await page.getByLabel('Search config options…').fill('multiplierKilledRecently');
  await expect(killed).toHaveValue('5');
  await page.locator('.shell-builds-button').click();
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('30');
  await page.locator('.bd-details summary').click();
  const loadout = page.getByRole('combobox', { name: 'PoB loadout', exact: true });
  await expect(loadout).toBeEnabled();
  await choose(page, loadout, 'Boss');
  await go(page, 'Config');
  await selected('3');
  await page.getByLabel('Search config options…').fill('multiplierKilledRecently');
  await expect(killed).toHaveValue('2');
});
