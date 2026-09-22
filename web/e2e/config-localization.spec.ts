import { expect, test, type Page } from '@playwright/test';
import { deflateSync } from 'node:zlib';

const reward = 'questInterlude 2Khari CrossingMolten Shrine';
const control = (page: Page, key: string) => page.locator(`[id=${JSON.stringify(`config-${key}`)}]`);
const code = deflateSync(`<PathOfBuilding2>
  <Build level="90" className="Ranger" ascendClassName="Deadeye" mainSocketGroup="2"/>
  <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
  <Skills activeSkillSet="1"><SkillSet id="1">
    <Skill enabled="true"><Gem skillId="FireballPlayer" gemId="Metadata/Items/Gems/SkillGemFireball" level="20" quality="0" enabled="true"/></Skill>
    <Skill enabled="true"><Gem skillId="WindDancerPlayer" gemId="Metadata/Items/Gems/SkillGemWindDancer" level="20" quality="0" enabled="true"/></Skill>
  </SkillSet></Skills>
  <Config><Input name="${reward}" boolean="false"/><Input name="companionInPresence" boolean="false"/></Config>
</PathOfBuilding2>`).toString('base64url');

async function ready(page: Page) {
  await page.addInitScript(() => localStorage.setItem('pobr-lang', 'zh-CN'));
  await page.goto('/');
  await expect(page.getByLabel('等级', { exact: true })).toBeEnabled({ timeout: 90_000 });
}

async function importBuild(page: Page) {
  await page.locator('.import-code').first().fill(code);
  await page.locator('.import-submit').click();
  await expect(page.locator('.import-config-review')).toBeVisible();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
}

test('catalog defaults are checked, translated, recalculable and resettable', async ({ page }) => {
  await ready(page);
  await page.getByRole('navigation').getByRole('button', { name: '配置', exact: true }).click();
  const life = page.locator('.stat-row').filter({ has: page.locator('dt', { hasText: /^生命$/ }) }).locator('dd').first();
  await expect(control(page, reward)).toBeChecked();
  await expect(control(page, 'companionInPresence')).toBeChecked();
  const baseline = await life.textContent();
  await control(page, reward).uncheck();
  await expect(life).not.toHaveText(baseline!);
  await page.locator('.config-item').filter({ has: control(page, reward) }).getByRole('button').click();
  await expect(control(page, reward)).toBeChecked();
  await expect(life).toHaveText(baseline!);
  await expect(control(page, 'questAct 2Valley of the TitansMedallion')).toHaveValue('None');
  await expect.poll(() => control(page, 'questAct 2Valley of the TitansMedallion').textContent()).not.toContain('increased Charm');
  const search = page.getByRole('searchbox', { name: '搜索配置项…' });
  for (const [key, translated] of [
    ['conditionLowRunicWard', '处于低符文结界状态？'],
    ['conditionMissingRunicWard', '符文结界未满？'],
    ['conditionNoRunicWard', '符文结界已耗尽？'],
    ['configBossFaceBroken', '毁面首领数量：'],
    ['onLeyline', '位于地脉上？'],
    ['TotalCompanionLife', '同伴总生命覆盖值：'],
  ]) {
    await search.fill(key);
    await expect(control(page, key)).toHaveAttribute('aria-label', translated);
  }
  await search.fill('multiplierCurrentManaPercentage');
  await expect(control(page, 'multiplierCurrentManaPercentage')).toHaveAttribute('placeholder', '100');
  await search.fill('conditionEnemyShocked');
  await expect(control(page, 'conditionEnemyShocked')).not.toBeChecked();
});

test('import reminder is inline and imported false values survive review and reload', async ({ page }) => {
  let dialogs = 0;
  page.on('dialog', async dialog => { dialogs++; await dialog.dismiss(); });
  await ready(page);
  await importBuild(page);
  const reminder = page.locator('.import-config-review');
  await expect(reminder).toHaveAttribute('role', 'note');
  await expect(page.locator('.items-page')).toBeVisible();
  await expect(reminder).toContainText('任务奖励');
  await reminder.getByRole('button', { name: '检查配置' }).click();
  await expect(reminder).toHaveCount(0);
  await expect(control(page, reward)).not.toBeChecked();
  await expect(control(page, 'companionInPresence')).not.toBeChecked();
  await page.reload();
  await expect(control(page, reward)).not.toBeChecked({ timeout: 90_000 });
  expect(dialogs).toBe(0);
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).state.params.config_inputs);
  expect(saved[reward]).toBe(false);
  expect(saved.companionInPresence).toBe(false);
});

test('localized triggered skill uses a readable popup and keyboard selection', async ({ page }) => {
  await ready(page);
  await importBuild(page);
  const selector = page.locator('.main-skill-select').getByRole('button', { name: '主技能', exact: true });
  await expect(selector).toHaveText(/风舞者（触发）/);
  await page.getByRole('navigation').getByRole('button', { name: '技能', exact: true }).click();
  await expect(page.locator('.skills-toolbar')).toContainText('风舞者（触发）');
  await expect(page.locator('.skill-set-chip')).toContainText('锐眼');
  await selector.click();
  const popup = page.getByRole('listbox', { name: '主技能' });
  await expect(popup).toBeVisible();
  await expect(popup.getByRole('option', { selected: true })).toContainText('风舞者（触发）');
  const colors = await popup.getByRole('option').first().evaluate(element => ({
    text: getComputedStyle(element).color, background: getComputedStyle(element.parentElement!).backgroundColor,
  }));
  expect(colors.text).not.toBe(colors.background);
  expect(colors.background).not.toBe('rgb(255, 255, 255)');
  await popup.press('Home');
  await popup.press('Enter');
  await expect(popup).toHaveCount(0);
  await expect(selector).toHaveText(/1\. 火球/);
  await expect(page.locator('.skill-group.is-main .skill-group-name')).toHaveText('火球');
  await selector.click();
  await popup.getByRole('option', { name: /2\. 风舞者/ }).click();
  await expect(selector).toHaveText(/风舞者（触发）/);
});
