import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { deflateSync, inflateSync } from 'node:zlib';
import type { ConfigInputValue, PassiveNode } from '../src/api/types';
import type { SavedSession } from '../src/hooks/useBuildSession';
import { booted, openBuilds, control, copyCurrentStage, currentStageName, go, hitDps, idle, importCode, noMotion, openSetting, openStage, row, savedState, setLanguage, shareCode } from './configPage';

test.use(noMotion);

const charges = ['Power', 'Frenzy', 'Endurance'] as const;
type Charge = typeof charges[number];
const countKey = (charge: Charge) => `override${charge}Charges`;
const useKey = (charge: Charge) => `use${charge}Charges`;
const raw: Record<string, ConfigInputValue> = {
  enemyIsBoss: 'None', enemyFireResist: '23',
  syntheticUnknownFalse: false, syntheticUnknownZero: 0,
  syntheticUnknownNumber: 7.5, syntheticUnknownString: '0042 <raw> & preserved',
};
const version = readFileSync('../data/CURRENT', 'utf8').trim();
const nodes: PassiveNode[] = JSON.parse(readFileSync(`../data/${version}/base/passive_tree.json`, 'utf8'));
const treeCatalog = JSON.parse(readFileSync(`../data/${version}/overlay/passive_jewels.json`, 'utf8'));
const root = nodes.find(node => node.id === treeCatalog.class_starts.witch)!.skill;
const numeric = (text: string) => Number(text.replaceAll(',', ''));
const saved = (page: Page): Promise<SavedSession['state']> => savedState(page);
const inputs = async (page: Page) => (await saved(page)).params.config_inputs;

// Synthetic XML and named synthetic items are fixtures, not backend substitutes.
// An existing per-charge item modifier supplies the numerical consumer. PoE2
// charges do not acquire any invented inherent damage bonus in these tests.
function syntheticCode(config: Record<string, ConfigInputValue>, perCharge: readonly Charge[] = [], flatDamage = 0) {
  const xmlInputs = Object.entries({ ...raw, ...config }).map(([name, value]) => {
    const type = typeof value === 'boolean' ? 'boolean' : typeof value === 'number' ? 'number' : 'string';
    const escaped = String(value).replaceAll('&', '&amp;').replaceAll('"', '&quot;').replaceAll('<', '&lt;');
    return `<Input name="${name}" ${type}="${escaped}"/>`;
  }).join('');
  const consumer = perCharge.map(charge => `8% increased Damage per ${charge} Charge`).join('\n');
  return deflateSync(`<PathOfBuilding2>
    <Build level="85" className="Witch" ascendClassName="None" mainSocketGroup="1"/>
    <Tree activeSpec="1"><Spec nodes="${root}" treeVersion="0_5"/></Tree>
    <Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true">
      <Gem skillId="FireballPlayer" gemId="Metadata/Items/Gems/SkillGemFireball" level="16" quality="0" enabled="true"/>
    </Skill></SkillSet></Skills>
    <Items activeItemSet="1"><Item id="1">Rarity: RARE
Synthetic Charge Count Ring
Sapphire Ring
Implicits: 0
40% increased Fire Damage
+80 to maximum Life
${consumer}
${flatDamage ? `${flatDamage}% increased Damage` : ''}</Item><ItemSet id="1"><Slot name="Ring 1" itemId="1"/></ItemSet></Items>
    <Config>${xmlInputs}</Config>
  </PathOfBuilding2>`).toString('base64url');
}

async function ready(page: Page) {
  await booted(page);
  await openBuilds(page);
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled();
}

async function editCount(page: Page, charge: Charge, count: number, configName = 'Config') {
  const key = countKey(charge);
  await openSetting(page, key, configName);
  await control(page, key).fill(String(count));
  await control(page, key).press('Enter');
  await expect.poll(async () => (await inputs(page))[key]).toBe(count);
  await idle(page);
}

async function editUse(page: Page, charge: Charge, enabled: boolean) {
  const key = useKey(charge);
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

async function flatControls(page: Page, counts: readonly number[]) {
  // Independent flat-item imports do not exercise the count-resolution seam.
  // Exact displayed DPS equality proves the count-dependent consumer, rather
  // than just checking that the Config textbox or saved input changed.
  const values = new Map<number, string>();
  for (const count of counts) {
    await importCode(page, syntheticCode({}, [], 8 * count), values.size ? 'currentStage' : 'newBuild');
    values.set(count, await hitDps(page).innerText());
  }
  return values;
}

test.beforeEach(async ({ page }) => {
  // Only unrelated league discovery is intercepted; imports, data, calculation,
  // planner previews and persistence use the shipped actual app and real WASM.
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.goto('/build');
  await ready(page);
});

for (const charge of charges) {
  test(`${charge}: imported 1 and edited 2 reach the consumer through reload/export/reimport`, async ({ page }) => {
    const expected = await flatControls(page, [1, 2]);
    expect(numeric(expected.get(2)!)).toBeGreaterThan(numeric(expected.get(1)!));
    await importCode(page, syntheticCode({ [useKey(charge)]: true, [countKey(charge)]: 1 }, [charge]), 'currentStage');
    await expect(hitDps(page)).toHaveText(expected.get(1)!);
    const original = await saved(page);
    await openSetting(page, countKey(charge));
    await control(page, countKey(charge)).focus();
    await control(page, countKey(charge)).press('Tab');
    expect(await saved(page)).toEqual(original);
    await editCount(page, charge, 2);
    await expect(hitDps(page)).toHaveText(expected.get(2)!);
    const edited = await saved(page);
    expect(edited.params.config_inputs).toEqual({ ...original.params.config_inputs, [countKey(charge)]: 2 });
    await page.reload();
    await ready(page);
    expect(await saved(page)).toEqual(edited);
    await expect(hitDps(page)).toHaveText(expected.get(2)!);
    // Before a new import changes the reset baseline, reset must recover the
    // original imported count (not absence or max charges).
    await openSetting(page, countKey(charge));
    await reset(page, countKey(charge));
    expect(await saved(page)).toEqual(original);
    await expect(hitDps(page)).toHaveText(expected.get(1)!);
    await editCount(page, charge, 2);
    const shared = await shareCode(page);
    expect(await exportedInputs(page, shared)).toMatchObject({ ...raw, [useKey(charge)]: true, [countKey(charge)]: 2 });
    await importCode(page, shared, 'currentStage');
    await expect(hitDps(page)).toHaveText(expected.get(2)!);
    expect(await inputs(page)).toMatchObject(edited.params.config_inputs);
    expect((await saved(page)).params.enemy_tier).toBe(original.params.enemy_tier);
    await importCode(page, syntheticCode({ [useKey(charge)]: true, [countKey(charge)]: 2 }, [charge]), 'currentStage');
    await expect(hitDps(page)).toHaveText(expected.get(2)!);
  });

  test(`${charge}: absent/reset uses max3 while use-off defeats both explicit counts`, async ({ page }) => {
    const expected = await flatControls(page, [0, 1, 2, 3]);
    await importCode(page, syntheticCode({ [useKey(charge)]: true }, [charge]), 'currentStage');
    const original = await saved(page);
    expect((await inputs(page))[countKey(charge)]).toBeUndefined();
    await expect(hitDps(page)).toHaveText(expected.get(3)!);
    await editCount(page, charge, 1);
    await expect(hitDps(page)).toHaveText(expected.get(1)!);
    await editCount(page, charge, 2);
    await expect(hitDps(page)).toHaveText(expected.get(2)!);
    await editUse(page, charge, false);
    await expect(hitDps(page)).toHaveText(expected.get(0)!);
    await editCount(page, charge, 1);
    await expect(hitDps(page)).toHaveText(expected.get(0)!);
    await reset(page, countKey(charge));
    expect((await inputs(page))[countKey(charge)]).toBeUndefined();
    await expect(hitDps(page)).toHaveText(expected.get(0)!);
    await openSetting(page, useKey(charge));
    await reset(page, useKey(charge));
    expect(await saved(page)).toEqual(original);
    await expect(hitDps(page)).toHaveText(expected.get(3)!);
    const shared = await shareCode(page);
    const exported = await exportedInputs(page, shared);
    expect(exported).toMatchObject({ ...raw, [useKey(charge)]: true });
    expect(exported[countKey(charge)]).toBeUndefined();
    await importCode(page, shared, 'currentStage');
    await expect(hitDps(page)).toHaveText(expected.get(3)!);
    expect((await inputs(page))[countKey(charge)]).toBeUndefined();
  });
  // Main's accepted PoB2 reference semantics: zero selects max3; a fractional
  // current count is retained but ordinary per-charge item tags floor stacks;
  // an explicit 5 remains above the default maximum rather than being clamped.
  for (const [importedCount, consumedStacks] of [[0, 3], [1.75, 1], [5, 5]] as const) {
    test(`${charge}: raw numeric ${importedCount} survives reset/reload/export and consumes ${consumedStacks} item stacks`, async ({ page }) => {
      const expected = await flatControls(page, [0, 2, consumedStacks]);
      await importCode(page, syntheticCode({
        [useKey(charge)]: true, [countKey(charge)]: importedCount,
      }, [charge]), 'currentStage');
      const original = await saved(page);
      expect((await inputs(page))[countKey(charge)]).toBe(importedCount);
      await expect(hitDps(page)).toHaveText(expected.get(consumedStacks)!);
      await page.reload();
      await ready(page);
      expect(await saved(page)).toEqual(original);
      await expect(hitDps(page)).toHaveText(expected.get(consumedStacks)!);
      await editCount(page, charge, 2);
      await expect(hitDps(page)).toHaveText(expected.get(2)!);
      await openSetting(page, countKey(charge));
      await reset(page, countKey(charge));
      expect(await saved(page)).toEqual(original);
      await expect(hitDps(page)).toHaveText(expected.get(consumedStacks)!);
      const shared = await shareCode(page);
      expect(await exportedInputs(page, shared)).toMatchObject({
        ...raw, [useKey(charge)]: true, [countKey(charge)]: importedCount,
      });
      await importCode(page, shared, 'currentStage');
      expect(await inputs(page)).toMatchObject(original.params.config_inputs);
      await expect(hitDps(page)).toHaveText(expected.get(consumedStacks)!);
      await editUse(page, charge, false);
      expect((await inputs(page))[countKey(charge)]).toBe(importedCount);
      await expect(hitDps(page)).toHaveText(expected.get(0)!);
      const disabled = await saved(page);
      await page.reload();
      await ready(page);
      expect(await saved(page)).toEqual(disabled);
      await expect(hitDps(page)).toHaveText(expected.get(0)!);
    });
  }
}

test('all three count assumptions remain stage-scoped through switching and actual reload', async ({ page }) => {
  const expected = await flatControls(page, [3, 4, 5, 6]);
  const config = Object.fromEntries(charges.flatMap(charge => [[useKey(charge), true], [countKey(charge), 1]]));
  await importCode(page, syntheticCode(config, charges), 'currentStage');
  await expect(hitDps(page)).toHaveText(expected.get(3)!);
  const originalStage = await currentStageName(page);
  const original = await saved(page);
  await copyCurrentStage(page, 'Synthetic two-charge stage');
  const editedStage = await currentStageName(page);
  for (const [index, charge] of charges.entries()) {
    await editCount(page, charge, 2);
    await expect(hitDps(page)).toHaveText(expected.get(4 + index)!);
  }
  const edited = await saved(page);
  await page.reload();
  await ready(page);
  expect(await saved(page)).toEqual(edited);
  await expect(hitDps(page)).toHaveText(expected.get(6)!);
  await openStage(page, originalStage);
  await idle(page);
  expect(await saved(page)).toEqual(original);
  await expect(hitDps(page)).toHaveText(expected.get(3)!);
  await openStage(page, editedStage);
  await idle(page);
  expect(await saved(page)).toEqual(edited);
  await expect(hitDps(page)).toHaveText(expected.get(6)!);
  for (const [index, charge] of charges.entries()) {
    await openSetting(page, countKey(charge));
    await reset(page, countKey(charge));
    await expect(hitDps(page)).toHaveText(expected.get(5 - index)!);
  }
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
      .fill('Rarity: RARE\nSynthetic Charge Count Replacement\nSapphire Ring\nImplicits: 0\n100% increased Fire Damage\n+100 to maximum Life\n8% increased Damage per Power Charge');
    await page.getByRole('button', { name: 'Calculate replacement', exact: true }).click();
    await expect(results(page, planner)).toHaveAttribute('aria-busy', 'false');
  }
  await expect(apply(page, planner).first()).toBeEnabled();
  await idle(page);
}

for (const planner of ['support', 'passive', 'replacement'] as const) {
  test(`${planner}: count-only edits/reset invalidate real receipts, untouched raw blur retains them`, async ({ page }) => {
    test.setTimeout(300_000);
    const expected = await flatControls(page, [2]);
    await importCode(page, syntheticCode({ usePowerCharges: true, overridePowerCharges: '1' }, ['Power']), 'currentStage');
    const original = await saved(page);
    const baseline = await hitDps(page).innerText();
    await complete(page, planner, true);
    const receipt = await results(page, planner).innerText();
    await openSetting(page, 'overridePowerCharges');
    await control(page, 'overridePowerCharges').focus();
    await control(page, 'overridePowerCharges').press('Tab');
    expect(await saved(page)).toEqual(original);
    await go(page, plannerPage(planner));
    await expect(results(page, planner)).toHaveText(receipt, { useInnerText: true });
    await expect(apply(page, planner).first()).toBeEnabled();
    await editCount(page, 'Power', 2);
    await expect(hitDps(page)).toHaveText(expected.get(2)!);
    expect((await inputs(page)).overridePowerCharges).toBe(2);
    await go(page, plannerPage(planner));
    await expect(results(page, planner)).toHaveCount(0);
    await expect(apply(page, planner)).toHaveCount(0);
    await complete(page, planner, false);
    await openSetting(page, 'overridePowerCharges');
    await reset(page, 'overridePowerCharges');
    expect(await saved(page)).toEqual(original);
    await expect(hitDps(page)).toHaveText(baseline);
    await go(page, plannerPage(planner));
    await expect(results(page, planner)).toHaveCount(0);
    await expect(apply(page, planner)).toHaveCount(0);
  });
}

for (const width of [390, 1440]) {
  for (const [lang, configName] of [['en-US', 'Config'], ['zh-TW', '配置'], ['zh-CN', '配置']] as const) {
    test(`${lang} ${width}px: keyboard count edit/reset preserves raw imported types`, async ({ page }) => {
      await page.setViewportSize({ width, height: 780 });
      const expected = await flatControls(page, [2]);
      await importCode(page, syntheticCode({ usePowerCharges: true, overridePowerCharges: '1' }, ['Power']), 'currentStage');
      const original = await saved(page);
      const baseline = await hitDps(page).innerText();
      await setLanguage(page, lang);
      await openSetting(page, 'overridePowerCharges', configName);
      const input = control(page, 'overridePowerCharges');
      await input.focus();
      await input.press('Tab');
      expect(await saved(page)).toEqual(original);
      expect((await inputs(page)).overridePowerCharges).toBe('1');
      await input.focus();
      // The imported count is the text "1": a text field takes the new count typed, a number field takes arrow up.
      if (await input.getAttribute('type') === 'number') await input.press('ArrowUp');
      else { await input.press('ControlOrMeta+a'); await input.pressSequentially('2'); }
      await input.press('Tab');
      await expect.poll(async () => (await inputs(page)).overridePowerCharges).toBe(2);
      await idle(page);
      await expect(hitDps(page)).toHaveText(expected.get(2)!);
      expect(await inputs(page)).toEqual({ ...original.params.config_inputs, overridePowerCharges: 2 });
      await page.screenshot({ path: test.info().outputPath(`${lang}-${width}-charge-count.png`), fullPage: true });
      // The reset button appears only after the numeric blur commits the edit;
      // focus explicitly before checking the keyboard Tab/Enter path.
      await input.focus();
      await input.press('Tab');
      const resetButton = row(page, 'overridePowerCharges').locator('.config-reset');
      await expect(resetButton).toBeFocused();
      await resetButton.press('Enter');
      await idle(page);
      await expect(input).toHaveValue('1');
      await expect(hitDps(page)).toHaveText(baseline);
      expect(await saved(page)).toEqual(original);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
    });
  }
}
