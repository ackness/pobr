import { expect, test } from '@playwright/test';
import { deflateSync, inflateSync } from 'node:zlib';

const source = `<PathOfBuilding2>
  <Build level="90" className="Huntress" mainSocketGroup="1"/>
  <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
  <Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true" mainActiveSkill="1">
    <Gem skillId="SummonBeastPlayer" level="20" quality="0" enabled="true"/>
  </Skill></SkillSet></Skills><Items activeItemSet="1"><ItemSet id="1"/></Items><Config/>
</PathOfBuilding2>`;

test('choose a companion and its skill, inspect minion breakdowns, and preserve the player', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await page.getByRole('button', { name: 'Build', exact: true }).click();
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(deflateSync(source).toString('base64url'));
  await page.locator('.import-submit').click();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await page.getByRole('button', { name: 'Skills', exact: true }).click();
  await page.locator('.skill-group-title').first().click();
  const picker = page.locator('.minion-controls');
  await expect(page.locator('.skill-group-title').first()).not.toContainText('{0}');
  await picker.getByRole('searchbox', { name: 'Search minions…', exact: true }).fill('Quadrilla');
  await picker.getByRole('button', { name: 'Minion type', exact: true }).click();
  await page.getByRole('option', { name: 'Quadrilla', exact: true }).click();
  await expect(page.locator('.minion-section')).toContainText('Quadrilla');
  await picker.getByRole('button', { name: 'Minion skill', exact: true }).click();
  await page.getByRole('option', { name: /^3 ·/ }).click();
  const state = () => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).state);
  await expect.poll(async () => (await state()).socketGroups[0].gems[0].minion_skill).toBe(3);
  await expect.poll(async () => (await state()).socketGroups[0].gems[0].minion).toBe('Metadata/Monsters/Quadrilla/Quadrilla');
  await page.keyboard.press('Escape');
  await page.screenshot({ path: '../.cache/minion-selection-skills.png', fullPage: true });
  const desktopSize = page.viewportSize()!;
  await page.setViewportSize({ width: 390, height: 844 });
  await picker.scrollIntoViewIfNeeded();
  await page.screenshot({ path: '../.cache/minion-selection-mobile.png', fullPage: true });
  for (const control of await picker.locator('input, button').all()) {
    const bounds = await control.boundingBox();
    expect(bounds && bounds.x >= 0 && bounds.x + bounds.width <= 390).toBeTruthy();
  }
  await page.setViewportSize(desktopSize);

  await page.getByRole('button', { name: 'Calcs', exact: true }).click();
  const life = page.locator('#breakdown-Life .breakdown-value');
  await expect(life).toBeVisible();
  const playerLife = await life.innerText();
  await page.getByRole('button', { name: 'Actor', exact: true }).click();
  await page.getByRole('option', { name: /^Minion: Quadrilla/ }).click();
  await expect(page.locator('#breakdown-Life .breakdown-name')).toHaveText('Minion: Life');
  const minionLife = page.locator('.minion-section .stat-row').filter({ has: page.getByText('Life', { exact: true }) }).locator('dd');
  await expect(life).toHaveText(await minionLife.innerText());
  await page.locator('#breakdown-Life .breakdown-toggle').click();
  await expect(page.locator('#breakdown-Life .breakdown-detail')).toBeVisible();
  await page.screenshot({ path: '../.cache/minion-selection-calcs.png', fullPage: true });
  await page.getByRole('button', { name: 'Actor', exact: true }).click();
  await page.getByRole('option', { name: 'Player', exact: true }).click();
  await expect(life).toHaveText(playerLife);
  await expect(page.locator('#breakdown-Life .breakdown-name')).toHaveText('Life');
  await page.locator('.minion-section .stat-row').filter({ has: page.getByText('Life', { exact: true }) }).click();
  await expect(page.locator('#breakdown-Life .breakdown-name')).toHaveText('Minion: Life');

  await page.getByRole('button', { name: 'Build', exact: true }).click();
  await page.getByRole('button', { name: 'Generate share code', exact: true }).click();
  const code = await page.getByRole('textbox', { name: 'Share Code', exact: true }).inputValue();
  const xml = inflateSync(Buffer.from(code, 'base64url')).toString();
  expect(xml).toContain('skillMinion="Metadata/Monsters/Quadrilla/Quadrilla"');
  expect(xml).toContain('skillMinionSkill="3"');
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await expect.poll(async () => (await state()).socketGroups[0].gems[0].minion_skill).toBe(3);
  await page.reload();
  await expect(page.locator('.minion-section')).toContainText('Quadrilla', { timeout: 90_000 });
});
