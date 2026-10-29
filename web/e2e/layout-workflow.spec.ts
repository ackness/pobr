import { expect, test, type Page } from '@playwright/test';
import { statRow, statValue } from './shell';
import { choose, goTo, importAndOpen } from './skillsAndCalcs';
import { readFileSync } from 'node:fs';
import { deflateSync } from 'node:zlib';

const monk = readFileSync('../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt', 'utf8');
// The skill picker at the top of the results panel.
const inspected = (page: Page) => page.getByRole('combobox', { name: 'Inspected skill', exact: true });
const saved = (page: Page) => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state);
/** Import a build and open Items. Below the wide breakpoint the page links sit in the menu sheet. */
async function start(page: Page, code: string) {
  await importAndOpen(page, code);
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
    const trigger = page.getByRole('combobox', { name: 'Equipment position', exact: true });
    const positions = ['Main Hand', 'Off Hand', 'Helmet', 'Body Armour', 'Gloves', 'Boots', 'Amulet', 'Ring 1', 'Ring 2', 'Belt', 'Flask 1', 'Flask 2', 'Charm 1', 'Charm 2', 'Charm 3', 'Arm 1', 'Arm 2', 'Leg 1', 'Leg 2'];
    for (const position of positions) {
      await trigger.click();
      await page.getByRole('listbox').getByRole('option', { name: position, exact: true }).click();
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
  await goTo(page, 'Config');
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
  // The view switch above the list has a button of the same name; this one is in the empty-search notice.
  await page.locator('.search-empty').getByRole('button', { name: 'Show all options', exact: true }).click();
  await expect(search).toHaveValue('');
  await expect(page.getByRole('checkbox', { name: /Configured only/ })).not.toBeChecked();
  expect((await saved(page)).params.config_inputs.buffOnslaught).toBe(false);
});

test('editing and analysis shortcuts reach their target without changing the current build', async ({ page }) => {
  await start(page, monk);
  const before = await saved(page);
  // Skills opens on the main skill's group, ready to edit.
  await goTo(page, 'Skills');
  await expect(page.locator('.skill-group.is-main .skill-group-title')).toHaveAttribute('aria-expanded', 'true');
  await goTo(page, 'Tree');
  // The planning side panel starts open, so the button first folds it away; opening it again moves focus to its tools.
  const planning = page.getByRole('button', { name: 'Attributes & planning', exact: true });
  await expect(planning).toHaveAttribute('aria-expanded', 'true');
  await planning.click();
  await expect(planning).toHaveAttribute('aria-expanded', 'false');
  await planning.click();
  await expect(page.locator('.tree-edit-tools')).toBeFocused();
  // The results panel's "Skill metrics" link opens the skill overview on the Calcs page.
  await page.locator('#character-stats .rail-link').click();
  await expect(page).toHaveURL(/\/calcs$/);
  await expect(page.locator('#skill-metrics-heading')).toBeFocused();
  expect(await saved(page)).toEqual(before);
});

test('a build without a main selection defaults to its highest-DPS skill and can select an exact secondary effect', async ({ page }) => {
  const code = deflateSync(`<PathOfBuilding2>
    <Build level="80" className="Witch"/>
    <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
    <Skills activeSkillSet="1"><SkillSet id="1">
      <Skill enabled="true" set1="false" set2="true"><Gem skillId="FireballPlayer" level="15" quality="0" enabled="true"/><Gem skillId="SparkPlayer" level="15" quality="0" enabled="true"/></Skill>
      <Skill enabled="true"><Gem skillId="ElementalWeaknessPlayer" level="1" quality="0" enabled="true"/><Gem skillId="ViciousHexSupportPlayer" level="1" quality="0" enabled="true"/></Skill>
      <Skill enabled="true"><Gem skillId="UnknownOverviewSkillPlayer" level="1" quality="0" enabled="true"/></Skill>
    </SkillSet></Skills><Items/><Config/>
  </PathOfBuilding2>`).toString('base64url');
  await start(page, code);
  // The first group deals the most damage; it is bound to the second weapon set.
  // Doom Blast runs at the level of the curse it is granted on, here 1.
  await expect(inspected(page)).toHaveText(/^1\. (Fireball|Spark)$/);
  expect((await saved(page)).params.main_socket_group).toBe(0);
  expect((await saved(page)).weaponSwap.active).toBe(2);
  await expect(page.locator('#character-stats').getByRole('heading', { name: 'Offence', exact: true })).toHaveCount(1);
  await page.locator('#character-stats .rail-link').click();
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
  await expect(rows.filter({ hasText: 'Highest DPS' })).toHaveCount(1);
  expect((await saved(page)).params.main_socket_group).toBe(0);
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await spark.getByRole('button').focus();
  await page.keyboard.press('Enter');
  await expect.poll(async () => (await saved(page)).socketGroups[0].main_active_skill).toBe(2);
  expect((await saved(page)).params.main_socket_group).toBe(0);
  expect((await saved(page)).weaponSwap.active).toBe(2);
  await expect(inspected(page)).toContainText('Spark');
  await expect(spark.getByRole('button')).toHaveAttribute('aria-disabled', 'false');
  await expect(spark.getByRole('button')).toBeFocused();
  // An unresolvable inspected skill must not present the previous skill's offence as
  // current: skill-specific values are hidden, character resources and defence stay.
  await unavailable.getByRole('button').click();
  const stats = page.locator('#character-stats');
  await expect(stats).toContainText('No valid inspected skill');
  await expect(stats.getByRole('heading', { name: 'Offence', exact: true })).toHaveCount(0);
  await expect(statRow(page, 'Total DPS')).toHaveCount(0);
  await expect(stats.getByRole('heading', { name: 'Resources', exact: true })).toHaveCount(1);
  await expect(statValue(page, /^Life$/).first()).not.toHaveText('—');
  await choose(page, inspected(page), '1. Spark');
  await expect(inspected(page)).toHaveText('1. Spark');
  await goTo(page, 'Upgrades');
  await expect(page.getByRole('combobox', { name: 'Skill used for scoring', exact: true })).toContainText('1. Spark');
  await page.reload();
  await expect(inspected(page)).toContainText('Spark', { timeout: 90_000 });
  expect((await saved(page)).socketGroups[0].main_active_skill).toBe(2);
  expect((await saved(page)).weaponSwap.active).toBe(2);
});

test('opening support planning explicitly selects an imported group without a primary', async ({ page }) => {
  const code = deflateSync(`<PathOfBuilding2><Build level="80" className="Witch"/>
    <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
    <Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="15" quality="0" enabled="true"/></Skill></Skills>
    <Items/><Config/></PathOfBuilding2>`).toString('base64url');
  await start(page, code);
  await expect(inspected(page)).toHaveText('1. Fireball');
  expect((await saved(page)).params.main_socket_group).toBe(0);
  await goTo(page, 'Skills');
  await page.locator('.skill-group-title').first().click();
  await page.locator('.gem-optimizer-toggle').click();
  await expect.poll(async () => (await saved(page)).params.main_socket_group).toBe(0);
  await expect(inspected(page)).toContainText('Fireball');
  await expect(page.getByRole('button', { name: 'Calculate support combinations', exact: true })).toBeEnabled();
});
