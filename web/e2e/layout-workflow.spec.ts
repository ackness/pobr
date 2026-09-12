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
  await page.getByRole('button', { name: /^Edit main skill/ }).click();
  await expect(page.locator('.skill-group.is-main .skill-group-title')).toHaveAttribute('aria-expanded', 'true');
  await nav(page, 'Tree').click();
  await page.getByRole('button', { name: 'Attributes & planning', exact: true }).click();
  await expect(page.locator('.tree-edit-tools')).toBeFocused();
  await nav(page, 'Calcs').click();
  await page.getByRole('button', { name: 'Skill DPS ↓', exact: true }).click();
  await expect(page.locator('#fulldps-heading')).toBeFocused();
  await nav(page, 'Build references').click();
  await page.locator('.guidance-result').first().getByRole('button', { name: 'Compare details', exact: true }).click();
  await expect(page.locator('.guidance-detail')).toBeFocused();
  await page.locator('.guidance-detail .page-header button').click();
  await expect(page.locator('.guidance-section-heading')).toBeFocused();
  expect(await saved(page)).toEqual(before);
});
