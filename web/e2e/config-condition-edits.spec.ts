import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { deflateSync, inflateSync } from 'node:zlib';
import type { ConfigInputValue, PassiveNode } from '../src/api/types';
import { booted, control, copyCurrentStage, currentStageName, go, hitDps, idle, importCode, noMotion, openSetting, openStage, row, savedState, setLanguage, shareCode } from './configPage';

test.use(noMotion);

const cases = [
  ['usePowerCharges', 'Fireball', 'Power'],
  ['useFrenzyCharges', 'Fireball', 'Frenzy'],
  ['useEnduranceCharges', 'Fireball', 'Endurance'],
  ['conditionEnemyFireExposure', 'Fireball', null],
  ['conditionEnemyColdExposure', 'Comet', null],
  ['conditionEnemyLightningExposure', 'Spark', null],
] as const;
const raw: Record<string, ConfigInputValue> = {
  enemyIsBoss: 'None', enemyFireResist: '23',
  syntheticUnknownFalse: false, syntheticUnknownZero: 0,
  syntheticUnknownNumber: 7.5, syntheticUnknownString: '0042 <raw> & preserved',
};
const version = readFileSync('../data/CURRENT', 'utf8').trim();
const nodes: PassiveNode[] = JSON.parse(readFileSync(`../data/${version}/base/passive_tree.json`, 'utf8'));
const treeCatalog = JSON.parse(readFileSync(`../data/${version}/overlay/passive_jewels.json`, 'utf8'));
const root = nodes.find(node => node.id === treeCatalog.class_starts.witch)!.skill;
const number = (text: string) => Number(text.replaceAll(',', ''));
const saved = savedState;
const inputs = async (page: Page): Promise<Record<string, ConfigInputValue>> => (await saved(page)).params.config_inputs;

// Synthetic XML is explicitly fixture input. Imported skills, parser, Config,
// calculation, planner candidates and export all use the shipped real WASM/data.
// The per-charge line supplies an existing numerical consumer; it is NOT a claim
// that PoE2 charges inherently grant damage. The native/JSON seam pins charge stacks.
function syntheticCode(key: string, enabled: boolean | undefined, skill = 'Fireball', charge: string | null = null) {
  const config = { ...raw, ...(enabled === undefined ? {} : { [key]: enabled }) };
  const xmlInputs = Object.entries(config).map(([name, value]) => {
    const type = typeof value === 'boolean' ? 'boolean' : typeof value === 'number' ? 'number' : 'string';
    const escaped = String(value).replaceAll('&', '&amp;').replaceAll('"', '&quot;').replaceAll('<', '&lt;');
    return `<Input name="${name}" ${type}="${escaped}"/>`;
  }).join('');
  // A configured exposure only counts for a character that can apply it, which an exposure source on the
  // gear provides; without one the control is not offered and the setting has no effect.
  const exposure = key.endsWith('Exposure')
    ? '\nInflict Elemental Exposure on Hit, lowering total Elemental Resistances by 5%' : '';
  return deflateSync(`<PathOfBuilding2>
    <Build level="85" className="Witch" ascendClassName="None" mainSocketGroup="1"/>
    <Tree activeSpec="1"><Spec nodes="${root}" treeVersion="0_5"/></Tree>
    <Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true">
      <Gem skillId="${skill}Player" gemId="Metadata/Items/Gems/SkillGem${skill}" level="16" quality="0" enabled="true"/>
    </Skill></SkillSet></Skills>
    <Items activeItemSet="1"><Item id="1">Rarity: RARE
Synthetic Condition Ring
Sapphire Ring
Implicits: 0
40% increased Fire Damage
+80 to maximum Life
${charge ? `8% increased Damage per ${charge} Charge` : ''}${exposure}</Item><ItemSet id="1"><Slot name="Ring 1" itemId="1"/></ItemSet></Items>
    <Config>${xmlInputs}</Config>
  </PathOfBuilding2>`).toString('base64url');
}

async function ready(page: Page) {
  await page.goto('/build');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await booted(page);
}

async function edit(page: Page, key: string, enabled: boolean) {
  await openSetting(page, key);
  await control(page, key).setChecked(enabled);
  await expect.poll(async () => (await inputs(page))[key]).toBe(enabled);
  await idle(page);
}

async function reset(page: Page, key: string) {
  await row(page, key).locator('.config-reset').click();
  await expect(row(page, key).locator('.config-reset')).toHaveCount(0);
  await idle(page);
}

async function exportedInputs(page: Page, code: string) {
  return page.evaluate(xml => {
    const document = new DOMParser().parseFromString(xml, 'application/xml');
    return Object.fromEntries([...document.querySelectorAll('Config Input')].map(input => [
      input.getAttribute('name'), input.hasAttribute('boolean') ? input.getAttribute('boolean') === 'true'
        : input.hasAttribute('number') ? Number(input.getAttribute('number')) : input.getAttribute('string'),
    ]));
  }, inflateSync(Buffer.from(code, 'base64url')).toString('utf8'));
}

test.beforeEach(async ({ page }) => {
  // Only unrelated league discovery is intercepted. No engine or data mocks.
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
});

for (const [key, skill, charge] of cases) {
  for (const imported of [false, true]) {
    test(`${key} imported ${imported}: edit/reset/export matches independently imported final state`, async ({ page }) => {
      await ready(page);
      await importCode(page, syntheticCode(key, imported, skill, charge));
      const before = await saved(page);
      const baseline = await hitDps(page).innerText();
      await edit(page, key, !imported);
      const edited = await hitDps(page).innerText();
      expect(imported ? number(edited) < number(baseline) : number(edited) > number(baseline)).toBe(true);
      expect(await inputs(page)).toEqual({ ...before.params.config_inputs, [key]: !imported });
      await reset(page, key);
      if (imported) await expect(control(page, key)).toBeChecked();
      else await expect(control(page, key)).not.toBeChecked();
      await expect(hitDps(page)).toHaveText(baseline);
      expect(await saved(page)).toEqual(before);
      await edit(page, key, !imported);
      const shared = await shareCode(page);
      expect(await exportedInputs(page, shared)).toMatchObject({ ...raw, [key]: !imported });
      await importCode(page, shared, 'currentStage');
      await expect(hitDps(page)).toHaveText(edited);
      expect(await inputs(page)).toMatchObject({ ...before.params.config_inputs, [key]: !imported });
      expect((await saved(page)).params.enemy_tier).toBe(before.params.enemy_tier);
      await importCode(page, syntheticCode(key, !imported, skill, charge), 'currentStage');
      await expect(hitDps(page)).toHaveText(edited);
      expect(await inputs(page)).toMatchObject({ ...before.params.config_inputs, [key]: !imported });
      expect((await saved(page)).params.enemy_tier).toBe(before.params.enemy_tier);
    });
  }
  test(`${key} absent: reset restores absence and the real disabled consumer`, async ({ page }) => {
    await ready(page);
    await importCode(page, syntheticCode(key, undefined, skill, charge));
    const before = await saved(page);
    const baseline = await hitDps(page).innerText();
    expect((await inputs(page))[key]).toBeUndefined();
    await edit(page, key, true);
    await expect.poll(async () => number(await hitDps(page).innerText())).toBeGreaterThan(number(baseline));
    await reset(page, key);
    await expect(control(page, key)).not.toBeChecked();
    await expect(hitDps(page)).toHaveText(baseline);
    expect(await saved(page)).toEqual(before);
    const shared = await shareCode(page);
    const exported = await exportedInputs(page, shared);
    expect(exported).toMatchObject(raw);
    expect(exported[key]).toBeUndefined();
    await importCode(page, shared, 'currentStage');
    await expect(hitDps(page)).toHaveText(baseline);
    expect((await inputs(page))[key]).toBeUndefined();
  });
}

test('opposite charge and exposure stages retain their own edited/reset/imported assumptions', async ({ page }) => {
  await ready(page);
  await importCode(page, syntheticCode('conditionEnemyFireExposure', true));
  const originalStage = await currentStageName(page);
  const original = await saved(page);
  const onDps = await hitDps(page).innerText();
  await copyCurrentStage(page, 'Synthetic condition edit stage');
  const editedStage = await currentStageName(page);
  await edit(page, 'conditionEnemyFireExposure', false);
  await edit(page, 'usePowerCharges', true);
  const offDps = await hitDps(page).innerText();
  const edited = await saved(page);
  expect(number(offDps)).toBeLessThan(number(onDps));
  await openStage(page, originalStage);
  await idle(page);
  await expect(hitDps(page)).toHaveText(onDps);
  expect(await saved(page)).toEqual(original);
  await openStage(page, editedStage);
  await idle(page);
  await expect(hitDps(page)).toHaveText(offDps);
  expect(await saved(page)).toEqual(edited);
  await openSetting(page, 'conditionEnemyFireExposure');
  await reset(page, 'conditionEnemyFireExposure');
  await openSetting(page, 'usePowerCharges');
  await reset(page, 'usePowerCharges');
  await expect(hitDps(page)).toHaveText(onDps);
  expect(await saved(page)).toEqual(original);
});

type Planner = 'support' | 'passive' | 'replacement';
const plannerPage = (planner: Planner) => planner === 'support' ? 'Skills' : planner === 'passive' ? 'Tree' : 'Upgrades';
const results = (page: Page, planner: Planner) => page.locator(planner === 'support' ? '.support-results'
  : planner === 'passive' ? '.tree-planner-body > .tree-planner-results' : '.replacement-content');
const apply = (page: Page, planner: Planner) => planner === 'support'
  ? page.locator('.gem-optimizer').getByRole('button', { name: 'Apply', exact: true })
  : page.getByRole('button', { name: planner === 'passive' ? 'Apply plan' : /^Apply to build/, exact: planner === 'passive' });

async function complete(page: Page, planner: Planner, first: boolean) {
  await go(page, plannerPage(planner));
  if (planner === 'support') {
    if (first) {
      const group = page.locator('.skill-group').first();
      const title = group.locator('.skill-group-title');
      if (await title.getAttribute('aria-expanded') === 'false') await title.click();
      const toggle = group.locator('.gem-optimizer-toggle');
      if (await toggle.getAttribute('aria-expanded') === 'false') await toggle.click();
      // Goal, capacity and candidate settings are folded away until asked for.
      const settings = group.locator('.gem-optimizer-settings');
      if (!(await settings.evaluate(element => (element as HTMLDetailsElement).open))) await settings.locator(':scope > summary').click();
      await page.getByRole('spinbutton', { name: 'Unlocked support sockets', exact: true }).fill('2');
      await page.getByRole('checkbox', { name: 'Include lineage supports', exact: true }).uncheck();
      await page.getByRole('combobox', { name: 'Objective', exact: true }).click();
      await page.getByRole('option', { name: 'Max total DPS', exact: true }).click();
    }
    await page.getByRole('button', { name: 'Calculate support combinations', exact: true }).click();
    await expect(page.locator('.support-plan').first()).toBeVisible({ timeout: 90_000 });
    await expect(page.getByRole('button', { name: 'Calculate support combinations', exact: true })).toBeEnabled();
  } else if (planner === 'passive') {
    if (first) {
      const planToggle = page.getByRole('button', { name: 'Plan passive upgrades', exact: true });
      if (await planToggle.getAttribute('aria-expanded') === 'false') await planToggle.click();
      await page.getByRole('combobox', { name: 'Objective', exact: true }).click();
      await page.getByRole('option', { name: 'Max Life', exact: true }).click();
      await page.getByRole('spinbutton', { name: 'Point budget', exact: true }).fill('3');
      await page.getByRole('combobox', { name: 'New travel attributes', exact: true }).click();
      await page.getByRole('option', { name: 'Strength', exact: true }).click();
    }
    await page.getByRole('button', { name: 'Find upgrade routes', exact: true }).click();
    await expect(results(page, planner).locator(':scope > li').first()).toBeVisible({ timeout: 90_000 });
    await expect(page.getByRole('button', { name: 'Find upgrade routes', exact: true })).toBeEnabled();
  } else {
    if (first) await page.getByRole('button', { name: 'Max total DPS', exact: true }).click();
    await page.getByRole('textbox', { name: 'Complete item text', exact: true })
      .fill('Rarity: RARE\nSynthetic Condition Replacement\nSapphire Ring\nImplicits: 0\n100% increased Fire Damage\n+100 to maximum Life\n8% increased Damage per Power Charge');
    await page.getByRole('button', { name: 'Calculate replacement', exact: true }).click();
    await expect(results(page, planner)).toHaveAttribute('aria-busy', 'false');
  }
  await expect(apply(page, planner).first()).toBeEnabled();
  await idle(page);
}

for (const planner of ['support', 'passive', 'replacement'] as const) {
  for (const [key, charge] of [['usePowerCharges', 'Power'], ['conditionEnemyFireExposure', null]] as const) {
    test(`${planner} ${key}: real edit and reset invalidate receipts without resurrecting Apply`, async ({ page }) => {
      test.setTimeout(300_000);
      await ready(page);
      await importCode(page, syntheticCode(key, true, 'Fireball', charge));
      const original = await saved(page);
      const baseline = await hitDps(page).innerText();
      await complete(page, planner, true);
      const receipt = await results(page, planner).innerText();
      await openSetting(page, key);
      await control(page, key).focus();
      await control(page, key).press('Tab');
      expect(await saved(page)).toEqual(original);
      await go(page, plannerPage(planner));
      await expect(results(page, planner)).toHaveText(receipt, { useInnerText: true });
      await expect(apply(page, planner).first()).toBeEnabled();
      await edit(page, key, false);
      await expect.poll(async () => number(await hitDps(page).innerText())).toBeLessThan(number(baseline));
      await go(page, plannerPage(planner));
      await expect(results(page, planner)).toHaveCount(0);
      await expect(apply(page, planner)).toHaveCount(0);
      await complete(page, planner, false);
      await openSetting(page, key);
      await reset(page, key);
      await expect(hitDps(page)).toHaveText(baseline);
      expect(await saved(page)).toEqual(original);
      await go(page, plannerPage(planner));
      await expect(results(page, planner)).toHaveCount(0);
      await expect(apply(page, planner)).toHaveCount(0);
      await complete(page, planner, false);
      const originalStage = await currentStageName(page);
      await copyCurrentStage(page, `Synthetic ${planner} condition stage`);
      await go(page, plannerPage(planner));
      await expect(results(page, planner)).toHaveCount(0);
      await expect(apply(page, planner)).toHaveCount(0);
      await openStage(page, originalStage);
      await idle(page);
      await expect(results(page, planner)).toHaveCount(0);
      await expect(apply(page, planner)).toHaveCount(0);
      expect(await saved(page)).toEqual(original);
    });
  }
}

for (const width of [390, 1440]) {
  for (const [lang, configName] of [['en-US', 'Config'], ['zh-TW', '配置'], ['zh-CN', '配置']] as const) {
    test(`${lang} ${width}px: keyboard exposure edit/reset preserves typed raw import`, async ({ page }) => {
      await page.setViewportSize({ width, height: 780 });
      await ready(page);
      await importCode(page, syntheticCode('conditionEnemyFireExposure', true));
      const original = await saved(page);
      const baseline = await hitDps(page).innerText();
      await setLanguage(page, lang);
      await openSetting(page, 'conditionEnemyFireExposure', configName);
      const checkbox = control(page, 'conditionEnemyFireExposure');
      const label = await checkbox.getAttribute('aria-label');
      expect(label).not.toBe('conditionEnemyFireExposure');
      await checkbox.focus();
      await checkbox.press('Space');
      await expect.poll(async () => (await inputs(page)).conditionEnemyFireExposure).toBe(false);
      await idle(page);
      await expect.poll(async () => number(await hitDps(page).innerText())).toBeLessThan(number(baseline));
      await expect(checkbox).not.toBeChecked();
      await page.screenshot({ path: test.info().outputPath(`${lang}-${width}-condition-edit.png`), fullPage: true });
      const resetButton = row(page, 'conditionEnemyFireExposure').locator('.config-reset');
      await checkbox.press('Tab');
      await expect(resetButton).toBeFocused();
      await resetButton.press('Enter');
      await idle(page);
      await expect(checkbox).toBeChecked();
      await expect(hitDps(page)).toHaveText(baseline);
      expect(await saved(page)).toEqual(original);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
    });
  }
}
