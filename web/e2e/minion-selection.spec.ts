import { expect, test } from '@playwright/test';
import { deflateSync, inflateSync } from 'node:zlib';
import { choose, goTo, importBuild, shareCode } from './skillsAndCalcs';

const source = `<PathOfBuilding2>
  <Build level="90" className="Huntress" mainSocketGroup="1"/>
  <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
  <Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true" mainActiveSkill="1">
    <Gem skillId="SummonBeastPlayer" level="20" quality="0" enabled="true"/>
  </Skill></SkillSet></Skills><Items activeItemSet="1"><ItemSet id="1"/></Items><Config/>
</PathOfBuilding2>`;

test('choose a companion and its skill, inspect minion breakdowns, and preserve the player', async ({ page }) => {
  await importBuild(page, deflateSync(source).toString('base64url'));
  // The minion section of the results panel.
  const minionSection = page.locator('#character-stats .rail-section')
    .filter({ has: page.locator('.rail-title', { hasText: /^Minion/ }) });
  const minionLife = minionSection.locator('.rail-row').filter({ has: page.locator('.rail-label', { hasText: /^Life$/ }) }).locator('.rail-value');
  await goTo(page, 'Skills');
  await page.locator('.skill-group-title').first().click();
  // Minion choices sit with the group's other less common settings.
  await page.locator('.skill-advanced > summary').click();
  const picker = page.locator('.minion-controls');
  await expect(page.locator('.skill-group-title').first()).not.toContainText('{0}');
  await picker.getByRole('searchbox', { name: 'Search minions…', exact: true }).fill('Quadrilla');
  await choose(page, picker.getByRole('combobox', { name: 'Minion type', exact: true }), 'Quadrilla');
  await expect(minionSection).toContainText('Quadrilla');
  await choose(page, picker.getByRole('combobox', { name: 'Minion skill', exact: true }), /^3 ·/);
  const state = () => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state);
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

  await goTo(page, 'Calcs');
  // The metric list shows the values of the chosen actor: the player first, then the minion.
  const life = page.locator('#breakdown-Life strong');
  await expect(life).toBeVisible();
  const playerLife = await life.innerText();
  await choose(page, page.getByRole('combobox', { name: 'Actor', exact: true }), /^Minion: Quadrilla/);
  await expect(life).toHaveText(await minionLife.innerText());
  await page.locator('#breakdown-Life').click();
  await expect(page.locator('#calcs-detail .calcs-eyebrow')).toContainText('Minion: Quadrilla');
  await expect(page.locator('#calcs-detail-heading')).toHaveText('Life');
  await expect(page.locator('#calcs-detail div.breakdown-detail').first()).toBeVisible({ timeout: 90_000 });
  await page.screenshot({ path: '../.cache/minion-selection-calcs.png', fullPage: true });
  await choose(page, page.getByRole('combobox', { name: 'Actor', exact: true }), 'Player');
  await expect(life).toHaveText(playerLife);
  await expect(page.locator('#calcs-detail-heading')).toHaveText('Choose a metric to inspect its calculation.');
  await page.locator('#breakdown-Life').click();
  await expect(page.locator('#calcs-detail .calcs-eyebrow')).toHaveText('Player');
  // A minion's number in the results panel opens the minion's calculation, and carries on to Calcs.
  await minionLife.click();
  await expect(page.locator('.insp-kicker')).toContainText('Quadrilla');
  await expect(page.locator('.insp h2')).toHaveText('Life');
  await page.locator('.insp').getByRole('button', { name: 'Open in Calculations' }).click();
  await expect(page.locator('#calcs-detail .calcs-eyebrow')).toContainText('Minion: Quadrilla');
  await expect(page.locator('#calcs-detail-heading')).toHaveText('Life');
  await expect(life).toHaveText(await minionLife.innerText());

  const code = await shareCode(page);
  const xml = inflateSync(Buffer.from(code, 'base64url')).toString();
  expect(xml).toContain('skillMinion="Metadata/Monsters/Quadrilla/Quadrilla"');
  expect(xml).toContain('skillMinionSkill="3"');
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await expect.poll(async () => (await state()).socketGroups[0].gems[0].minion_skill).toBe(3);
  await page.reload();
  await expect(minionSection).toContainText('Quadrilla', { timeout: 90_000 });
});
