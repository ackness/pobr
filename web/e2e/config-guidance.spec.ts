import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { deflateSync, inflateSync } from 'node:zlib';
import type { ConfigInputValue } from '../src/api/types';
import { choose, control, currentStageName, go, hitDps, idle, importCode, nativeSelect, noMotion, openStage, row, savedState, setLanguage, shareCode } from './configPage';

test.use(noMotion);

const card = (page: Page) => page.locator('.config-quick-access');
const entry = (page: Page, key: string) => card(page).locator(`[data-config-var=${JSON.stringify(key)}]`);
const numeric = (text: string) => Number(text.replaceAll(',', ''));
/** Every control with a saved key: inputs, and the buttons that open a list. */
const controlIds = (page: Page) => page.locator('.config-section .config-item :is(input, button)[id^="config-"]')
  .evaluateAll(elements => elements.map(element => element.id).sort());
/** The skill whose numbers the results panel shows. */
const inspectedSkill = (page: Page) => page.locator('.shell-rail').getByRole('combobox', { name: 'Inspected skill', exact: true });
const common = ['conditionEnemyShocked', 'conditionShockEffect'];
const hitSettings: Record<string, string> = {
  Physical: 'enemyPhysicalReduction', Fire: 'enemyFireResist', Cold: 'enemyColdResist',
  Lightning: 'enemyLightningResist', Chaos: 'enemyChaosResist',
};

// Synthetic PoB XML is fixture input, not a backend substitute. Decoding, data,
// compatibility, candidate previews and every recalculation use the real WASM.
function syntheticCode(skills: string[] = ['Fireball', 'Spark'], config: Record<string, ConfigInputValue> = {
  conditionEnemyShocked: false, conditionShockEffect: 20,
}) {
  const inputs = Object.entries(config).map(([key, value]) => {
    const type = typeof value === 'boolean' ? 'boolean' : typeof value === 'number' ? 'number' : 'string';
    const escaped = String(value).replaceAll('&', '&amp;').replaceAll('"', '&quot;').replaceAll('<', '&lt;');
    return `<Input name="${key}" ${type}="${escaped}"/>`;
  }).join('');
  return deflateSync(`<PathOfBuilding2>
    <Build level="85" className="Sorceress" ascendClassName="None" mainSocketGroup="1"/>
    <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
    <Skills activeSkillSet="1"><SkillSet id="1">${skills.map(name => `<Skill enabled="true"><Gem skillId="${name}Player" gemId="Metadata/Items/Gems/SkillGem${name}" level="16" quality="0" enabled="true"/></Skill>`).join('')}</SkillSet></Skills>
    <Items activeItemSet="1"><Item id="1">Rarity: RARE
Synthetic Guidance Ring
Sapphire Ring
Implicits: 0
40% increased Fire Damage
+80 to maximum Life</Item><ItemSet id="1"><Slot name="Ring 1" itemId="1"/></ItemSet></Items>
    <Config>${inputs}</Config>
  </PathOfBuilding2>`).toString('base64url');
}

async function ready(page: Page) {
  await page.goto('/build');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
}

async function configInputs(page: Page): Promise<Record<string, ConfigInputValue>> {
  return (await savedState(page)).params.config_inputs;
}

/** The Config page with its quick-access card open (the card sits in a disclosure that starts closed). */
async function openConfig(page: Page) {
  await go(page, 'Config');
  await openQuickAccess(page);
}

async function openQuickAccess(page: Page) {
  const disclosure = page.locator('.config-quick-disclosure');
  await expect(disclosure).toBeVisible({ timeout: 90_000 });
  if (!(await disclosure.evaluate(element => (element as HTMLDetailsElement).open))) await disclosure.locator(':scope > summary').click();
  await expect(card(page)).toBeVisible();
}

async function quickKeys(page: Page) {
  return card(page).locator('[data-config-var]').evaluateAll(elements =>
    elements.map(element => element.getAttribute('data-config-var')!).sort());
}

async function openSetting(page: Page, key: string, keyboard = false) {
  const button = entry(page, key).getByRole('button');
  await expect(button).toBeEnabled();
  if (keyboard) {
    await button.focus();
    await page.keyboard.press('Shift+Tab');
    await page.keyboard.press('Tab');
    await expect(button).toBeFocused();
    await page.keyboard.press('Enter');
  } else await button.click();
  await expect(control(page, key)).toBeFocused();
  await expect(control(page, key)).toBeEnabled();
}

async function editNumber(page: Page, key: string, value: number) {
  await openSetting(page, key);
  await control(page, key).fill(String(value));
  await control(page, key).press('Enter');
  await expect.poll(async () => (await configInputs(page))[key]).toBe(value);
  await idle(page);
}

async function reset(page: Page, key: string) {
  await row(page, key).locator('.config-reset').click();
  await expect(row(page, key).locator('.config-reset')).toHaveCount(0);
  await idle(page);
}

async function receiptNumbers(page: Page, selector: string) {
  const texts = await page.locator(selector).allTextContents();
  return texts.map(text => (text.match(/[+-]?\d[\d,]*(?:\.\d+)?/g) ?? []).map(numeric));
}

async function expectReturnedHitEntries(page: Page) {
  // This is the actual calculated main-skill hit breakdown, not class, gem colour,
  // first-gem tags, or full-DPS/DoT relevance inferred from a fixture name.
  const types = await page.locator('.shell-rail .rail-share-row .rail-label').allTextContents();
  const expected = [...common, ...types.map(type => hitSettings[type.trim()])].sort();
  expect(expected.every(Boolean)).toBe(true);
  await expect.poll(() => quickKeys(page)).toEqual(expected);
  expect(expected.length).toBeLessThanOrEqual(7);
  for (const [type, key] of Object.entries(hitSettings)) {
    if (!types.includes(type)) await expect(entry(page, key)).toHaveCount(0);
  }
  await expect(card(page).locator('input, select, textarea')).toHaveCount(0);
}

test.beforeEach(async ({ page }) => {
  // The only intercepted response is unrelated market-league discovery; game
  // data, imports and the calculation backend remain unmodified and local.
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
});

for (const [fixture, identity, types] of [
  ['sorceress-stormweaver-comet', 'Comet', ['Chaos', 'Cold', 'Lightning']],
  ['ranger-pathfinder-ice-shot', 'Ice Shot', ['Cold', 'Fire', 'Lightning', 'Physical']],
] as const) {
  test(`${identity}: shortcuts match returned positive main-skill hits, not the first imported gem`, async ({ page }) => {
    await ready(page);
    const code = readFileSync(new URL(`../../examples/demo-bd-test/builds/${fixture}/code.txt`, import.meta.url), 'utf8').trim();
    await importCode(page, code);
    expect(numeric(await hitDps(page).innerText())).toBeGreaterThan(0);
    await openConfig(page);
    await expect(card(page)).toBeVisible();
    await expect(card(page).locator('.config-quick-skill')).toHaveText(identity);
    expect((await page.locator('.shell-rail .rail-share-row .rail-label').allTextContents()).sort()).toEqual([...types].sort());
    await expectReturnedHitEntries(page);
    const before = await savedState(page);
    await openSetting(page, 'enemyColdResist');
    expect(await savedState(page)).toEqual(before);
  });
}

test('review and keyboard jump clear both filters, preserve the request and keep the unique full catalog', async ({ page }) => {
  await ready(page);
  await importCode(page, syntheticCode(['Comet']));
  await openConfig(page);
  const allOptions = page.locator('.config-view').getByRole('button', { name: 'Show all options', exact: true });
  await allOptions.click();
  const headers = page.locator('.config-section-header');
  for (let index = 0; index < await headers.count(); index++) {
    if (await headers.nth(index).getAttribute('aria-expanded') !== 'true') await headers.nth(index).click();
  }
  const ids = await controlIds(page);
  expect(new Set(ids).size).toBe(ids.length);
  // An uncertain mechanic remains in the original catalog, not just quick entries.
  await page.locator('.config-search').fill('onLeyline');
  await expect(control(page, 'onLeyline')).toBeVisible();
  await page.getByRole('button', { name: 'Applicable to this build', exact: true }).click();
  await page.locator('.config-search').fill('no-such-guidance-option');
  await page.locator('.config-filter input').check();
  await expect(control(page, 'enemyColdResist')).toHaveCount(0);
  const before = await savedState(page);
  const baseline = await hitDps(page).innerText();
  await openSetting(page, 'enemyColdResist', true);
  await expect(page.locator('.config-search')).toHaveValue('');
  await expect(page.locator('.config-filter input')).not.toBeChecked();
  await expect(allOptions).toHaveAttribute('aria-pressed', 'true');
  await expect(row(page, 'enemyColdResist').locator('..').locator('..').locator('.config-section-header'))
    .toHaveAttribute('aria-expanded', 'true');
  const afterIds = await controlIds(page);
  expect(afterIds).toEqual(ids);
  await expect(control(page, 'enemyColdResist')).toHaveCount(1);
  expect(await savedState(page)).toEqual(before);
  await expect(hitDps(page)).toHaveText(baseline);
});

test('cold resistance changes real hit DPS and reset restores the exact absent override/default state', async ({ page }) => {
  await ready(page);
  await importCode(page, syntheticCode(['Comet']));
  await openConfig(page);
  const before = await savedState(page);
  const baseline = await hitDps(page).innerText();
  const defaultValue = await entry(page, 'enemyColdResist').locator('output').innerText();
  await editNumber(page, 'enemyColdResist', 60);
  await expect.poll(async () => numeric(await hitDps(page).innerText())).toBeLessThan(numeric(baseline));
  await expect(entry(page, 'enemyColdResist').locator('output code')).toHaveText('60');
  await reset(page, 'enemyColdResist');
  expect((await configInputs(page)).enemyColdResist).toBeUndefined();
  expect(await savedState(page)).toEqual(before);
  await expect(hitDps(page)).toHaveText(baseline);
  await expect(entry(page, 'enemyColdResist').locator('output')).toHaveText(defaultValue);
});

for (const importedShock of [false, true]) {
  test(`imported shock ${importedShock}: explicit 5/20 magnitude changes DPS, off and reset preserve import precedence`, async ({ page }) => {
    await ready(page);
    await importCode(page, syntheticCode(['Fireball'], {
      conditionEnemyShocked: importedShock, conditionShockEffect: 20,
    }));
    await openConfig(page);
    const before = await savedState(page);
    const baseline = await hitDps(page).innerText();
    await openSetting(page, 'conditionEnemyShocked');
    await control(page, 'conditionEnemyShocked').check();
    await idle(page);
    await editNumber(page, 'conditionShockEffect', 5);
    const five = numeric(await hitDps(page).innerText());
    await editNumber(page, 'conditionShockEffect', 20);
    await expect.poll(async () => numeric(await hitDps(page).innerText())).toBeGreaterThan(five);
    const twenty = numeric(await hitDps(page).innerText());
    await openSetting(page, 'conditionEnemyShocked');
    await control(page, 'conditionEnemyShocked').uncheck();
    await idle(page);
    await expect.poll(async () => numeric(await hitDps(page).innerText())).toBeLessThan(five);
    const off = await hitDps(page).innerText();
    await editNumber(page, 'conditionShockEffect', 5);
    await expect(hitDps(page)).toHaveText(off);
    await reset(page, 'conditionShockEffect');
    await expect(control(page, 'conditionShockEffect')).toHaveValue('20');
    await expect(hitDps(page)).toHaveText(off);
    await openSetting(page, 'conditionEnemyShocked');
    if (importedShock) {
      await reset(page, 'conditionEnemyShocked');
      await expect(control(page, 'conditionEnemyShocked')).toBeChecked();
    } else await expect(control(page, 'conditionEnemyShocked')).not.toBeChecked();
    await expect(hitDps(page)).toHaveText(baseline);
    expect(await savedState(page)).toEqual(before);
    if (importedShock) expect(numeric(baseline)).toBe(twenty);
    else expect(baseline).toBe(off);
  });
}

test('review, jump, edit/reset, share and reload preserve imported false/zero/numeric/string and unknown raw config', async ({ page }) => {
  const raw: Record<string, ConfigInputValue> = {
    conditionEnemyShocked: false, conditionShockEffect: 0, enemyColdResist: 17,
    enemyFireResist: '23', companionInPresence: false,
    syntheticUnknownFalse: false, syntheticUnknownZero: 0,
    syntheticUnknownNumber: 7.5, syntheticUnknownString: 'raw <unmodeled> & 0042',
  };
  await ready(page);
  await importCode(page, syntheticCode(['Comet', 'Fireball'], raw));
  const before = await savedState(page);
  await expect(page.locator('.shell-notice[role=note]')).toBeVisible();
  await page.locator('.shell-notice[role=note]').getByRole('button', { name: 'Review configuration', exact: true }).click();
  await openQuickAccess(page);
  for (const [key, value] of [['conditionEnemyShocked', false], ['conditionShockEffect', 0], ['enemyColdResist', 17]] as const) {
    await expect(entry(page, key).locator('output code')).toHaveText(JSON.stringify(value));
    await openSetting(page, key);
  }
  // Select the separately calculated fire skill to expose the imported string summary.
  await choose(page, inspectedSkill(page), /2\. Fireball/);
  await idle(page);
  await expect(entry(page, 'enemyFireResist').locator('output code')).toHaveText('"23"');
  await openSetting(page, 'enemyFireResist');
  await expect(control(page, 'enemyFireResist')).toHaveValue('23');
  expect(await configInputs(page)).toEqual(before.params.config_inputs);
  await choose(page, inspectedSkill(page), /1\. Comet/);
  await idle(page);
  expect(await savedState(page)).toEqual(before);
  const baseline = await hitDps(page).innerText();
  await editNumber(page, 'enemyColdResist', 60);
  await expect.poll(async () => numeric(await hitDps(page).innerText())).toBeLessThan(numeric(baseline));
  await reset(page, 'enemyColdResist');
  await expect(control(page, 'enemyColdResist')).toHaveValue('17');
  await expect(hitDps(page)).toHaveText(baseline);
  expect(await savedState(page)).toEqual(before);
  // Unmodeled keys are typed raw values on the Advanced tab: a boolean is a picker, the rest are fields.
  await page.getByRole('tab', { name: 'Advanced values', exact: true }).click();
  for (const key of Object.keys(raw).filter(key => key.startsWith('syntheticUnknown'))) {
    const field = page.getByLabel(key, { exact: true });
    if (typeof raw[key] === 'boolean') await expect.poll(() => nativeSelect(field).inputValue()).toBe(String(raw[key]));
    else await expect(field).toHaveValue(String(raw[key]));
  }
  const shared = await shareCode(page);
  const exportedInputs = await page.evaluate(xml => {
    const document = new DOMParser().parseFromString(xml, 'application/xml');
    return Object.fromEntries([...document.querySelectorAll('Config Input')].map(input => {
      const value = input.hasAttribute('boolean') ? input.getAttribute('boolean') === 'true'
        : input.hasAttribute('number') ? Number(input.getAttribute('number')) : input.getAttribute('string');
      return [input.getAttribute('name'), value];
    }));
  }, inflateSync(Buffer.from(shared, 'base64url')).toString('utf8'));
  expect(exportedInputs).toMatchObject(raw);
  await importCode(page, shared);
  expect(await configInputs(page)).toMatchObject(raw);
  await openConfig(page);
  await openSetting(page, 'conditionShockEffect');
  await page.reload();
  await openQuickAccess(page);
  await idle(page);
  expect(await configInputs(page)).toMatchObject(raw);
  await expect(entry(page, 'conditionEnemyShocked').locator('output code')).toHaveText('false');
  await expect(entry(page, 'conditionShockEffect').locator('output code')).toHaveText('0');
  await expect(hitDps(page)).toHaveText(baseline);
});

test('main-skill, stage and independent-build changes refresh context without leaking old hit shortcuts', async ({ page }) => {
  await ready(page);
  await importCode(page, syntheticCode());
  await openConfig(page);
  await expect(card(page).locator('.config-quick-skill')).toHaveText('Fireball');
  await expect.poll(() => quickKeys(page)).toEqual([...common, 'enemyFireResist'].sort());
  await choose(page, inspectedSkill(page), /2\. Spark/);
  await idle(page);
  await expect(card(page).locator('.config-quick-skill')).toHaveText('Spark');
  await expect.poll(() => quickKeys(page)).toEqual([...common, 'enemyLightningResist'].sort());
  await page.locator('.shell-builds-button').click();
  await page.getByRole('button', { name: 'Rename build', exact: true }).click();
  await page.getByRole('textbox', { name: 'Name', exact: true }).fill('Synthetic Guidance Original');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  const originalStage = await currentStageName(page);
  await page.getByRole('button', { name: 'Copy current stage', exact: true }).click();
  await page.getByRole('textbox', { name: 'Name', exact: true }).fill('Synthetic Guidance Empty');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await importCode(page, syntheticCode([]), 'currentStage');
  await openConfig(page);
  await expect.poll(() => quickKeys(page)).toEqual([...common].sort());
  await expect(card(page).locator('.config-quick-skill')).toHaveCount(0);
  await openStage(page, originalStage);
  await idle(page);
  await expect(card(page).locator('.config-quick-skill')).toHaveText('Spark');
  await expectReturnedHitEntries(page);
  await importCode(page, syntheticCode(['Comet']));
  await openConfig(page);
  await expect(card(page).locator('.config-quick-skill')).toHaveText('Comet');
  await expectReturnedHitEntries(page);
  await page.locator('.shell-builds-button').click();
  await page.getByRole('group', { name: 'Saved builds', exact: true }).getByRole('button', { name: /^Synthetic Guidance Original/ }).click();
  await idle(page);
  await openConfig(page);
  await expect(card(page).locator('.config-quick-skill')).toHaveText('Spark');
  await expect.poll(() => quickKeys(page)).toEqual([...common, 'enemyLightningResist'].sort());
});

test('EN/zh-TW/zh-CN labels and keyboard destinations remain localized without config mutation', async ({ page }) => {
  await ready(page);
  await importCode(page, syntheticCode(['Comet']));
  await openConfig(page);
  const before = await savedState(page);
  const baseline = await hitDps(page).innerText();
  for (const [lang, action] of [['en-US', 'Open setting'], ['zh-TW', '開啟設定'], ['zh-CN', '打开设置']] as const) {
    await expect(page.locator('html')).toHaveAttribute('lang', lang);
    const current = entry(page, 'enemyColdResist');
    await expect(current.getByRole('button', { name: new RegExp(`^${action}`) })).toBeEnabled();
    await openSetting(page, 'enemyColdResist', true);
    const canonicalLabel = await control(page, 'enemyColdResist').getAttribute('aria-label');
    await expect(current.locator('strong')).toHaveText(canonicalLabel!);
    await expect(entry(page, 'conditionEnemyShocked').locator('output code')).toHaveText('false');
    await expect(entry(page, 'conditionShockEffect').locator('output code')).toHaveText('20');
    expect(await savedState(page)).toEqual(before);
    await expect(hitDps(page)).toHaveText(baseline);
    await setLanguage(page, lang === 'en-US' ? 'zh-TW' : lang === 'zh-TW' ? 'zh-CN' : 'en-US');
  }
});

for (const width of [320, 390, 1440]) {
  test(`quick card and keyboard jump fit ${width}px and focus below the sticky toolbar`, async ({ page }) => {
    await page.setViewportSize({ width, height: 780 });
    await ready(page);
    await importCode(page, syntheticCode(['Comet']));
    await openConfig(page);
    await expect(card(page)).toBeVisible();
    expect(await card(page).evaluate(element => element.scrollWidth <= element.clientWidth + 1)).toBe(true);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
    const before = await savedState(page);
    await openSetting(page, 'enemyColdResist', true);
    await expect.poll(() => control(page, 'enemyColdResist').evaluate(element => {
      const bounds = element.getBoundingClientRect();
      const toolbar = document.querySelector('.config-toolbar')!.getBoundingClientRect();
      const x = bounds.left + bounds.width / 2;
      const y = bounds.top + bounds.height / 2;
      return bounds.top >= toolbar.bottom && bounds.bottom <= innerHeight
        && document.elementFromPoint(x, y) === element;
    })).toBe(true);
    expect(await savedState(page)).toEqual(before);
  });
}

for (const planner of ['support', 'replacement'] as const) {
  test(`completed ${planner} receipt survives review/jump, but a real config edit invalidates Apply`, async ({ page }) => {
    test.setTimeout(180_000);
    await ready(page);
    await importCode(page, syntheticCode(['Fireball']));
    let receipt: number[][];
    let supportIds: (string | null)[] = [];
    if (planner === 'support') {
      await go(page, 'Skills');
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
      await page.getByRole('button', { name: 'Calculate support combinations', exact: true }).click();
      await expect(page.locator('.support-plan').first()).toBeVisible({ timeout: 90_000 });
      await expect(page.getByRole('button', { name: 'Calculate support combinations', exact: true })).toBeEnabled();
      receipt = await receiptNumbers(page, '.support-plan-metrics');
      supportIds = await page.locator('.support-plan-gems [data-skill-id]').evaluateAll(elements =>
        elements.map(element => element.getAttribute('data-skill-id')));
    } else {
      await go(page, 'Upgrades');
      await page.getByRole('button', { name: 'Max total DPS', exact: true }).click();
      await page.getByRole('textbox', { name: 'Complete item text', exact: true }).evaluate(element => {
        const data = new DataTransfer();
        data.setData('text/plain', 'Rarity: RARE\nSynthetic Guidance Replacement\nSapphire Ring\nImplicits: 0\n100% increased Fire Damage\n+100 to maximum Life');
        element.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }));
      });
      await expect(page.locator('.replacement-content')).toHaveAttribute('aria-busy', 'false');
      await expect(page.getByRole('button', { name: /^Apply to build/ })).toBeEnabled();
      receipt = await receiptNumbers(page, '.upgrade-replacement-result tbody td');
    }
    const before = await savedState(page);
    const baseline = await hitDps(page).innerText();
    await openConfig(page);
    await page.locator('.config-search').fill('nothing-matches-this');
    await page.locator('.config-filter input').check();
    await openSetting(page, 'conditionEnemyShocked', true);
    expect(await savedState(page)).toEqual(before);
    // Leaving an untouched absent number is still navigation, not Reset/apply.
    await openSetting(page, 'enemyFireResist', true);
    await control(page, 'enemyFireResist').press('Tab');
    expect(await savedState(page)).toEqual(before);
    await expect(hitDps(page)).toHaveText(baseline);
    await go(page, planner === 'support' ? 'Skills' : 'Upgrades');
    if (planner === 'support') {
      await expect.poll(() => receiptNumbers(page, '.support-plan-metrics')).toEqual(receipt);
      await expect.poll(() => page.locator('.support-plan-gems [data-skill-id]').evaluateAll(elements =>
        elements.map(element => element.getAttribute('data-skill-id')))).toEqual(supportIds);
      await expect(page.locator('.support-plan').first().getByRole('button', { name: 'Apply', exact: true })).toBeEnabled();
    } else {
      await expect.poll(() => receiptNumbers(page, '.upgrade-replacement-result tbody td')).toEqual(receipt);
      await expect(page.getByRole('button', { name: /^Apply to build/ })).toBeEnabled();
    }
    await openConfig(page);
    await openSetting(page, 'conditionEnemyShocked');
    await control(page, 'conditionEnemyShocked').check();
    await idle(page);
    await expect.poll(async () => numeric(await hitDps(page).innerText())).toBeGreaterThan(numeric(baseline));
    await go(page, planner === 'support' ? 'Skills' : 'Upgrades');
    await expect(page.locator(planner === 'support' ? '.support-results' : '.replacement-content')).toHaveCount(0);
    const plannerApply = planner === 'support'
      ? page.locator('.gem-optimizer').getByRole('button', { name: 'Apply', exact: true })
      : page.getByRole('button', { name: /^Apply to build/ });
    await expect(plannerApply).toHaveCount(0);
    // Reverting numerically does not resurrect a stale previously completed receipt.
    await openConfig(page);
    await openSetting(page, 'conditionEnemyShocked');
    await reset(page, 'conditionEnemyShocked');
    await expect(hitDps(page)).toHaveText(baseline);
    await go(page, planner === 'support' ? 'Skills' : 'Upgrades');
    await expect(page.locator(planner === 'support' ? '.support-results' : '.replacement-content')).toHaveCount(0);
    expect(await savedState(page)).toEqual(before);
  });
}
