import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { deflateSync } from 'node:zlib';

const monk = readFileSync('../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt', 'utf8');
const nav = (page: Page, name: string) => page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name, exact: true });
const saved = (page: Page) => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).state);
async function start(page: Page, code: string) {
  await page.goto('/');
  await expect(page.getByRole('textbox', { name: 'Build code' })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill(code);
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible();
}

for (const width of [390, 1440]) {
  test(`equipment selection at ${width}px reveals the editor and every position remains reachable`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await start(page, monk);
    const before = await saved(page);
    await page.getByRole('button', { name: 'Body Armour', exact: true }).click();
    const detail = page.locator('.item-detail');
    await expect(detail).toBeFocused();
    const trigger = page.getByRole('button', { name: 'Equipment position', exact: true });
    const positions = ['Main Hand', 'Off Hand', 'Helmet', 'Body Armour', 'Gloves', 'Boots', 'Amulet', 'Ring 1', 'Ring 2', 'Belt', 'Flask 1', 'Flask 2', 'Charm 1', 'Charm 2', 'Charm 3'];
    for (const position of positions) {
      await trigger.click();
      await page.getByRole('listbox', { name: 'Equipment position', exact: true }).getByRole('option', { name: position, exact: true }).click();
      await expect(trigger).toContainText(position);
      await expect.poll(async () => detail.evaluate(element => {
        const main = element.closest('main')!.getBoundingClientRect();
        const header = element.querySelector('header')!.getBoundingClientRect();
        return header.top >= main.top && header.bottom <= main.bottom;
      })).toBe(true);
    }
    expect(await saved(page)).toEqual(before);
  });
}

test('configured-only filtering includes imported false values, composes with search and follows reset', async ({ page }) => {
  const code = deflateSync('<PathOfBuilding2><Build level="40" className="Witch"/><Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree><Skills/><Items/><Config><Input name="buffOnslaught" boolean="false"/><Input name="enemyLevel" number="82"/></Config></PathOfBuilding2>').toString('base64url');
  await start(page, code);
  await nav(page, 'Config').click();
  await page.getByRole('checkbox', { name: /Configured only/ }).check();
  await expect(page.locator('#config-buffOnslaught')).toBeVisible();
  await expect(page.locator('#config-buffOnslaught')).not.toBeChecked();
  await expect(page.locator('#config-enemyLevel')).toHaveValue('82');
  const search = page.getByRole('searchbox', { name: 'Search config options…' });
  await search.fill('buffOnslaught');
  await expect(page.locator('.config-item')).toHaveCount(1);
  await page.locator('#config-buffOnslaught').check();
  await page.getByRole('button', { name: /Restore imported\/default value/ }).click();
  await expect(page.locator('#config-buffOnslaught')).not.toBeChecked();
  await expect(page.locator('#config-buffOnslaught')).toBeVisible();
  await search.fill('no-such-condition');
  await page.getByRole('button', { name: 'Show all options', exact: true }).click();
  await expect(search).toHaveValue('');
  await expect(page.getByRole('checkbox', { name: /Configured only/ })).not.toBeChecked();
  expect((await saved(page)).params.config_inputs.buffOnslaught).toBe(false);
});

test('editing and analysis shortcuts reach their target without changing the current build', async ({ page }) => {
  await start(page, monk);
  const before = await saved(page);
  await nav(page, 'Skills').click();
  await page.getByRole('button', { name: /^Edit inspected skill/ }).click();
  await expect(page.locator('.skill-group.is-main .skill-group-title')).toHaveAttribute('aria-expanded', 'true');
  await nav(page, 'Tree').click();
  await page.getByRole('button', { name: 'Attributes & planning', exact: true }).click();
  await expect(page.locator('.tree-edit-tools')).toBeFocused();
  await nav(page, 'Calcs').click();
  await page.getByRole('button', { name: 'Skill metrics ↓', exact: true }).click();
  await expect(page.locator('#skill-metrics-heading')).toBeFocused();
  expect(await saved(page)).toEqual(before);
});

test('skill overview works without a main selection and selects an exact secondary effect', async ({ page }) => {
  const code = deflateSync(`<PathOfBuilding2>
    <Build level="80" className="Witch"/>
    <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
    <Skills activeSkillSet="1"><SkillSet id="1">
      <Skill enabled="true" set1="false" set2="true"><Gem skillId="FireballPlayer" level="15" quality="0" enabled="true"/><Gem skillId="SparkPlayer" level="15" quality="0" enabled="true"/></Skill>
      <Skill enabled="true"><Gem skillId="ElementalWeaknessPlayer" level="15" quality="0" enabled="true"/><Gem skillId="ViciousHexSupportPlayer" level="1" quality="0" enabled="true"/></Skill>
      <Skill enabled="true"><Gem skillId="UnknownOverviewSkillPlayer" level="1" quality="0" enabled="true"/></Skill>
    </SkillSet></Skills><Items/><Config/>
  </PathOfBuilding2>`).toString('base64url');
  await start(page, code);
  expect((await saved(page)).params.main_socket_group).toBeUndefined();
  expect((await saved(page)).weaponSwap?.active ?? 1).toBe(1);
  await expect(page.locator('.main-skill-select .app-select-value')).toHaveText('Not selected');
  await page.getByRole('button', { name: 'Skill metrics →', exact: true }).click();
  await expect(page.locator('#skill-metrics-heading')).toBeFocused();
  const rows = page.locator('.skill-metrics-row');
  await expect(rows).toHaveCount(5);
  await expect(rows.filter({ hasText: 'Fireball' })).toBeVisible();
  const spark = rows.filter({ hasText: 'Spark' });
  await expect(spark).toBeVisible();
  const curse = rows.filter({ hasText: 'Elemental Weakness' });
  await expect(curse.locator('.mod-value').first()).toHaveText('0');
  await expect(curse).not.toContainText('Unavailable');
  await expect(rows.filter({ hasText: /Doom Blast|DoomBlast/ })).toBeVisible();
  const unavailable = rows.filter({ hasText: /Unknown Overview Skill|UnknownOverviewSkill/ });
  await expect(unavailable).toContainText('Unavailable');
  await expect(unavailable.locator('.mod-value').first()).toHaveText('—');
  await expect(unavailable.locator('.calc-error')).not.toHaveText('Unavailable:');
  expect((await saved(page)).params.main_socket_group).toBeUndefined();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await spark.getByRole('button').focus();
  await page.keyboard.press('Enter');
  await expect.poll(async () => (await saved(page)).socketGroups[0].main_active_skill).toBe(2);
  expect((await saved(page)).params.main_socket_group).toBe(0);
  expect((await saved(page)).weaponSwap.active).toBe(2);
  await expect(page.locator('.main-skill-select .app-select-value')).toContainText('Spark');
  await expect(spark.getByRole('button')).toHaveAttribute('aria-disabled', 'false');
  await expect(spark.getByRole('button')).toBeFocused();
  // A failed inspection must not present the previous skill's baseline as current.
  await unavailable.getByRole('button').click();
  await expect(page.locator('.app-main > .calc-error')).toBeVisible();
  await expect(page.locator('.calcs-breakdowns .breakdown-item')).toHaveCount(0);
  await expect.poll(async () => {
    const values = await page.locator('.stat-sidebar .stat-row dd').allTextContents();
    return values.length > 0 && values.every(value => value.trim() === '—');
  }).toBe(true);
  await page.getByRole('button', { name: 'Inspected skill', exact: true }).click();
  await page.getByRole('option', { name: '1. Spark', exact: true }).click();
  await expect(page.locator('.main-skill-select .app-select-value')).toHaveText('1. Spark');
  await nav(page, 'Upgrades').click();
  await expect(page.getByRole('button', { name: 'Skill used for scoring', exact: true })).toContainText('1. Spark');
  await page.reload();
  await expect(page.locator('.main-skill-select .app-select-value')).toContainText('Spark', { timeout: 90_000 });
  expect((await saved(page)).socketGroups[0].main_active_skill).toBe(2);
  expect((await saved(page)).weaponSwap.active).toBe(2);
});

test('opening support planning explicitly selects an imported group without a primary', async ({ page }) => {
  const code = deflateSync(`<PathOfBuilding2><Build level="80" className="Witch"/>
    <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
    <Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="15" quality="0" enabled="true"/></Skill></Skills>
    <Items/><Config/></PathOfBuilding2>`).toString('base64url');
  await start(page, code);
  expect((await saved(page)).params.main_socket_group).toBeUndefined();
  await nav(page, 'Skills').click();
  await page.locator('.skill-group-title').first().click();
  await page.locator('.gem-optimizer-toggle').click();
  await expect.poll(async () => (await saved(page)).params.main_socket_group).toBe(0);
  await expect(page.locator('.main-skill-select .app-select-value')).toContainText('Fireball');
  await expect(page.getByRole('button', { name: 'Calculate support combinations', exact: true })).toBeEnabled();
});
