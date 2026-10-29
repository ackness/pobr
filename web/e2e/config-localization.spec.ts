import { expect, test, type Page } from '@playwright/test';
import { deflateSync } from 'node:zlib';
import { control, go, idle, nativeSelect, noMotion } from './configPage';
import { statValue } from './shell';

test.use(noMotion);

const reward = 'questInterlude 2Khari CrossingMolten Shrine';
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
  await page.goto('/build');
  await expect(page.getByLabel('等级', { exact: true })).toBeEnabled({ timeout: 90_000 });
}

async function importBuild(page: Page) {
  await page.locator('.import-code').first().fill(code);
  await page.locator('.import-submit').click();
  // An import lands on the Overview, with the reminder to review the configuration above the page.
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  await expect(page.locator('.shell-notice[role=note]')).toBeVisible();
  await idle(page);
}

/** Searching opens every group, so a control is on the page whichever group it belongs to (quest rewards start folded). */
async function find(page: Page, key: string) {
  await page.locator('.config-search').fill(key);
  await expect(control(page, key)).toBeVisible();
}

test('catalog defaults are checked, translated, recalculable and resettable', async ({ page }) => {
  await ready(page);
  await go(page, '配置');
  const life = statValue(page, /^生命$/);
  await find(page, reward);
  await expect(control(page, reward)).toBeChecked();
  await find(page, 'companionInPresence');
  await expect(control(page, 'companionInPresence')).toBeChecked();
  const baseline = await life.textContent();
  await find(page, reward);
  await control(page, reward).uncheck();
  await expect(life).not.toHaveText(baseline!);
  await page.locator('.config-item').filter({ has: control(page, reward) }).locator('.config-reset').click();
  await expect(control(page, reward)).toBeChecked();
  await expect(life).toHaveText(baseline!);
  // A list control is a picker over a hidden native select that carries the value and the full list of choices.
  const medallion = control(page, 'questAct 2Valley of the TitansMedallion');
  await find(page, 'questAct 2Valley of the TitansMedallion');
  await expect.poll(async () => nativeSelect(medallion).inputValue()).toBe('None');
  await expect.poll(() => nativeSelect(medallion).textContent()).not.toContain('increased Charm');
  const search = page.getByRole('searchbox', { name: '搜索配置项…' });
  // Labels are translated whether or not the option applies to this build, so look at the whole catalog.
  await page.locator('.config-view button').last().click();
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
  const reminder = page.locator('.shell-notice[role=note]');
  await expect(reminder).toHaveAttribute('role', 'note');
  await expect(page.locator('#ov-title')).toBeVisible();
  await expect(reminder).toContainText('任务奖励');
  await reminder.getByRole('button', { name: '检查配置' }).click();
  await expect(reminder).toHaveCount(0);
  await expect(page).toHaveURL(/\/config$/);
  await find(page, reward);
  await expect(control(page, reward)).not.toBeChecked();
  await find(page, 'companionInPresence');
  await expect(control(page, 'companionInPresence')).not.toBeChecked();
  await page.reload();
  await find(page, reward);
  await expect(control(page, reward)).not.toBeChecked({ timeout: 90_000 });
  expect(dialogs).toBe(0);
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.params.config_inputs);
  expect(saved[reward]).toBe(false);
  expect(saved.companionInPresence).toBe(false);
});

test('localized triggered skill uses a readable popup and keyboard selection', async ({ page }) => {
  await ready(page);
  await importBuild(page);
  // The Overview names the skill the calculation resolved to, in the interface language.
  await expect(page.locator('.ov-figure').first().locator('small')).toHaveText('风舞者');
  // The skill whose numbers the results panel shows is picked in a popup at the head of the panel.
  const selector = page.locator('.shell-rail').getByRole('combobox', { name: '查看技能', exact: true });
  await expect(selector).toHaveText(/2\. 风舞者/);
  await go(page, '技能');
  await expect(page.locator('.skill-group.is-main .skill-group-name')).toHaveText('风舞者');
  await expect(page.locator('.skill-set-chip')).toContainText('锐眼');
  await selector.click();
  const popup = page.getByRole('listbox');
  await expect(popup).toBeVisible();
  await expect(popup.getByRole('option', { selected: true })).toContainText('2. 风舞者');
  const colors = await popup.getByRole('option').first().evaluate(element => ({
    text: getComputedStyle(element).color, background: getComputedStyle(element.parentElement!).backgroundColor,
  }));
  expect(colors.text).not.toBe(colors.background);
  expect(colors.background).not.toBe('rgb(255, 255, 255)');
  // The popup keeps keyboard focus on its items: Home goes to the first one, Enter picks it.
  await page.keyboard.press('Home');
  await page.keyboard.press('Enter');
  await expect(popup).toHaveCount(0);
  await expect(selector).toHaveText(/1\. 火球/);
  await expect(page.locator('.skill-group.is-main .skill-group-name')).toHaveText('火球');
  await selector.click();
  await popup.getByRole('option', { name: /2\. 风舞者/ }).click();
  await expect(selector).toHaveText(/2\. 风舞者/);
});
