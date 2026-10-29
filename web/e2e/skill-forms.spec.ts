import { expect, test } from '@playwright/test';
import { deflateSync, inflateSync } from 'node:zlib';
import { statRow } from './shell';
import { choose, goTo, importBuild, shareCode } from './skillsAndCalcs';

const source = `<PathOfBuilding2>
  <Build level="90" className="Sorceress" mainSocketGroup="1"/>
  <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
  <Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true" mainActiveSkill="1">
    <Gem skillId="IceNovaPlayer" level="20" quality="0" enabled="true"/>
    <Gem skillId="SupportControlledDestructionPlayer" level="1" quality="0" enabled="true"/>
  </Skill><Skill enabled="true"><Gem skillId="FlameblastPlayer" level="20" quality="0" enabled="true"/></Skill></SkillSet></Skills>
  <Items activeItemSet="1"><ItemSet id="1"/></Items><Config/>
</PathOfBuilding2>`;

test('forms, stages, disabled supports and Full DPS can be edited and shared', async ({ page }) => {
  await importBuild(page, deflateSync(source).toString('base64url'));
  await goTo(page, 'Skills');
  const group = page.locator('.skill-group').first();
  await group.locator('.skill-group-title').click();
  await choose(page, group.getByRole('combobox', { name: 'Skill form', exact: true }), 'Cold-Infused');
  await group.locator('.skill-gem').nth(1).getByRole('checkbox', { name: 'Gem enabled', exact: true }).uncheck();
  // Full DPS inclusion and the group count are among the group's advanced settings.
  await group.locator('.skill-advanced > summary').click();
  await group.getByRole('checkbox', { name: 'Include in Full DPS', exact: true }).check();
  await group.getByRole('spinbutton', { name: 'Group count override', exact: true }).fill('2.5');
  await group.locator('.skill-gem').first().getByRole('spinbutton', { name: 'Gem count', exact: true }).fill('1.5');
  const state = () => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state);
  await expect.poll(async () => (await state()).socketGroups[0].gems[0].stat_set_index).toBe(2);
  await expect.poll(async () => (await state()).socketGroups[0].gems[1].enabled).toBe(false);
  await expect.poll(async () => (await state()).socketGroups[0].group_count).toBe(2.5);
  const total = statRow(page, /^Full DPS$/);
  await expect(total).toBeVisible();
  await expect(statRow(page, /^Full DoT DPS$/)).toBeVisible();
  await page.screenshot({ path: '../.cache/skill-forms-skills.png', fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await group.locator('.skill-gem').first().scrollIntoViewIfNeeded();
  expect(await group.locator('.skill-gem input').evaluateAll(inputs => inputs.every(input => input.getBoundingClientRect().right <= window.innerWidth))).toBe(true);
  await page.screenshot({ path: '../.cache/skill-forms-mobile.png', fullPage: true });
  await page.setViewportSize({ width: 1280, height: 720 });

  const flame = page.locator('.skill-group').nth(1);
  await flame.locator('.skill-group-title').click();
  await flame.getByRole('spinbutton', { name: 'Stages', exact: true }).fill('4');
  await expect.poll(async () => (await state()).params.config_inputs.skillFlameblastStages).toBe(4);
  await goTo(page, 'Calcs');
  await page.getByRole('tab', { name: 'Full DPS', exact: true }).click();
  await expect(page.locator('.full-dps-table tbody tr')).toHaveCount(1);
  await expect(page.locator('.full-dps-table')).toContainText('2.5');
  // The skill overview offers the form of the inspected skill as well.
  await page.getByRole('tab', { name: 'Skill metrics', exact: true }).click();
  await choose(page, page.getByRole('combobox', { name: 'Skill form', exact: true }), 'Cold-Infused');
  await page.keyboard.press('Escape');
  await expect(page.locator('[aria-labelledby=skill-metrics-heading]')).toHaveAttribute('aria-busy', 'false');
  await page.screenshot({ path: '../.cache/skill-forms-calcs.png', fullPage: true });

  const code = await shareCode(page);
  const xml = inflateSync(Buffer.from(code, 'base64url')).toString();
  expect(xml).toContain('includeInFullDPS="true"');
  expect(xml).toContain('groupCount="2.5"');
  expect(xml).toMatch(/<Gem\b[^>]*statSetIndex="2"[^>]*count="1.5"/);
  expect(xml).toMatch(/<Gem\b[^>]*skillId="SupportControlledDestructionPlayer"[^>]*enabled="false"/);
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await expect.poll(async () => (await state()).socketGroups[0].gems[0].stat_set_index).toBe(2);
  await expect.poll(async () => (await state()).socketGroups[0].gems[1].enabled).toBe(false);
  await page.reload();
  await expect(total).toBeVisible({ timeout: 90_000 });
  await expect.poll(async () => (await state()).socketGroups[0].group_count).toBe(2.5);
});
