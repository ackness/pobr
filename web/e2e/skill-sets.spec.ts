import { expect, test } from '@playwright/test';
import { deflateSync, inflateSync } from 'node:zlib';
import { bdLibraryButton } from './shell';
import { answerConfirm } from './confirmDialog';
import { choose, goTo, importBuild, shareCode } from './skillsAndCalcs';

const code = deflateSync(`<PathOfBuilding2><Build level="30" className="Witch" mainSocketGroup="2"/>
<Tree activeSpec="1"><Spec title="Mapping" nodes="" treeVersion="0_5"/><Spec title="Boss" nodes="" treeVersion="0_5"/></Tree>
<Skills activeSkillSet="7"><SkillSet id="7" title="Mapping"><Skill enabled="true" set1="true" set2="true" label="Keep label"><Gem skillId="SparkPlayer" level="5" quality="7" enabled="true" enableGlobal1="true" enableGlobal2="false"/></Skill><Skill enabled="true" set1="true" set2="false"><Gem skillId="FireballPlayer" level="8" quality="0" enabled="true"/></Skill></SkillSet><SkillSet id="3" title="Boss" mainSocketGroup="1"><Skill enabled="true"><Gem skillId="IceNovaPlayer" level="10" quality="0" enabled="true"/></Skill></SkillSet></Skills>
<Config><Input name="futureConfig" string="keep"/></Config></PathOfBuilding2>`).toString('base64url');

test('skill sets keep their main group and weapon bindings across copy, edits, switching and export', async ({ page }, testInfo) => {
  await importBuild(page, code);
  await goTo(page, 'Skills');
  const sets = page.getByRole('region', { name: 'Skill set', exact: true });
  // The set picker is a drop-down: its trigger shows the set's name, a hidden native select behind it holds the id.
  const selector = sets.getByRole('combobox', { name: 'Skill set', exact: true });
  const selectorId = sets.locator('select');
  const groups = page.locator('.skill-group');
  await expect(selectorId).toHaveValue('7');
  await expect(selector).toContainText('Mapping');
  await expect(groups.nth(1)).toHaveClass(/is-main/);
  await expect(selector).toBeEnabled();
  await sets.getByRole('button', { name: 'Copy set', exact: true }).click();
  await sets.getByRole('textbox', { name: 'Set name' }).fill('Copy');
  await sets.getByRole('button', { name: 'Save name' }).click();
  await expect(selectorId).toHaveValue('8');
  await expect(selector).toContainText('Copy');
  await expect(groups.first().locator('.skill-main-toggle')).toBeEnabled();
  await groups.first().locator('.skill-main-toggle').click();
  await expect(groups.first()).toHaveClass(/is-main/);
  if (await groups.first().locator('.skill-group-title').getAttribute('aria-expanded') !== 'true') {
    await groups.first().locator('.skill-group-title').click();
  }
  const level = groups.first().getByRole('spinbutton', { name: 'Level', exact: true });
  await expect(level).toBeEnabled();
  await level.fill('12');
  await expect(selector).toBeEnabled();
  await choose(page, selector, 'Mapping');
  await expect(selectorId).toHaveValue('7');
  await expect(groups.nth(1)).toHaveClass(/is-main/);
  await expect(selector).toBeEnabled();
  await choose(page, selector, 'Copy');
  await expect(selectorId).toHaveValue('8');
  await expect(groups.first()).toHaveClass(/is-main/);
  if (await groups.first().locator('.skill-group-title').getAttribute('aria-expanded') !== 'true') {
    await groups.first().locator('.skill-group-title').click();
  }
  await expect(level).toHaveValue('12');
  await expect(groups.nth(1).locator('.ui-badge')).toContainText('1');
  await sets.scrollIntoViewIfNeeded();
  await page.screenshot({ path: testInfo.outputPath('skill-sets.png'), fullPage: true });
  await page.reload();
  await expect(selectorId).toHaveValue('8', { timeout: 90_000 });
  await expect(groups.first()).toHaveClass(/is-main/);
  const exported = await shareCode(page);
  const xml = inflateSync(Buffer.from(exported, 'base64url')).toString('utf8');
  expect(xml.match(/<SkillSet /g)).toHaveLength(3);
  expect(xml).toContain('activeSkillSet="8"');
  expect(xml).toContain('enableGlobal2="false"');
  expect(xml).toContain('set2="false"');
  expect(xml).toContain('name="futureConfig" string="keep"');
  expect(xml.match(/label="Keep label"/g)).toHaveLength(2);
  await page.getByRole('textbox', { name: 'Build code' }).fill(exported);
  await page.locator('.import-submit').click();
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  await goTo(page, 'Skills');
  await expect(selectorId).toHaveValue('8');
  await expect(groups.first()).toHaveClass(/is-main/);
  if (await groups.first().locator('.skill-group-title').getAttribute('aria-expanded') !== 'true') {
    await groups.first().locator('.skill-group-title').click();
  }
  await expect(level).toHaveValue('12');
  await expect(selector).toBeEnabled();
  await choose(page, selector, 'Mapping');
  await expect(selectorId).toHaveValue('7');
  await expect(groups.nth(1)).toHaveClass(/is-main/);
  if (await groups.first().locator('.skill-group-title').getAttribute('aria-expanded') !== 'true') {
    await groups.first().locator('.skill-group-title').click();
  }
  await expect(level).toHaveValue('5');
  await expect(selector).toBeEnabled();
  await bdLibraryButton(page).click();
  await page.locator('.bd-details > summary').click();
  // Switching the PoB loadout replaces the stage's local edits, so the page asks first.
  await page.getByRole('combobox', { name: 'PoB loadout', exact: true }).click();
  await page.getByRole('option').nth(1).click();
  await answerConfirm(page, true);
  await goTo(page, 'Skills');
  await expect(selectorId).toHaveValue('3');
  await expect(groups).toHaveCount(1);
});
