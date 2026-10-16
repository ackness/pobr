import { expect, test } from '@playwright/test';
import { deflateSync, inflateSync } from 'node:zlib';

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
  await page.goto('/');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await page.getByRole('button', { name: 'Build', exact: true }).click();
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(deflateSync(source).toString('base64url'));
  await page.locator('.import-submit').click();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await page.getByRole('button', { name: 'Skills', exact: true }).click();
  const group = page.locator('.skill-group').first();
  await group.locator('.skill-group-title').click();
  await group.getByRole('button', { name: 'Skill form', exact: true }).click();
  await page.getByRole('option', { name: 'Cold-Infused', exact: true }).click();
  await group.locator('.skill-gem').nth(1).getByRole('checkbox', { name: 'Gem enabled', exact: true }).uncheck();
  await group.getByRole('checkbox', { name: 'Include in Full DPS', exact: true }).check();
  await group.getByRole('spinbutton', { name: 'Group count override', exact: true }).fill('2.5');
  await group.locator('.skill-gem').first().getByRole('spinbutton', { name: 'Gem count', exact: true }).fill('1.5');
  const state = () => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).state);
  await expect.poll(async () => (await state()).socketGroups[0].gems[0].stat_set_index).toBe(2);
  await expect.poll(async () => (await state()).socketGroups[0].gems[1].enabled).toBe(false);
  await expect.poll(async () => (await state()).socketGroups[0].group_count).toBe(2.5);
  const total = page.locator('.stat-row').filter({ has: page.getByText('Full DPS', { exact: true }) });
  await expect(total).toBeVisible();
  await expect(page.locator('.stat-row').filter({ has: page.getByText('Full DoT DPS', { exact: true }) })).toBeVisible();
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
  await page.getByRole('button', { name: 'Calcs', exact: true }).click();
  await expect(page.locator('.full-dps-table tbody tr')).toHaveCount(1);
  await expect(page.locator('.full-dps-table')).toContainText('2.5');
  await page.getByRole('button', { name: 'Skill form', exact: true }).click();
  await page.getByRole('option', { name: 'Cold-Infused', exact: true }).click();
  await page.keyboard.press('Escape');
  await expect(page.locator('[aria-labelledby=skill-metrics-heading]')).toHaveAttribute('aria-busy', 'false');
  await page.screenshot({ path: '../.cache/skill-forms-calcs.png', fullPage: true });

  await page.getByRole('button', { name: 'Build', exact: true }).click();
  await page.getByRole('button', { name: 'Generate share code', exact: true }).click();
  const code = await page.getByRole('textbox', { name: 'Share Code', exact: true }).inputValue();
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
