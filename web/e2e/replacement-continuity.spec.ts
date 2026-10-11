import { expect, test, type Page } from '@playwright/test';
import type { SlotItemInput } from '../src/api/types';

interface FixtureState {
  items: SlotItemInput[];
  [key: string]: unknown;
}

const nav = (page: Page, name: string) => page.getByRole('navigation', { name: 'Main navigation' })
  .getByRole('button', { name, exact: true });
const input = (page: Page) => page.getByRole('textbox', { name: 'Complete item text', exact: true });
const planner = (page: Page) => page.locator('.equipment-planner');
const replacement = (page: Page) => page.locator('.replacement-content');
const jointResults = (page: Page) => planner(page).locator('.equipment-plan-results');
const cap = (page: Page) => planner(page).getByRole('combobox', { name: 'Maximum replacements', exact: true });
const calculate = (page: Page) => page.getByRole('button', { name: 'Calculate replacement', exact: true });
const jointRun = (page: Page) => planner(page).getByRole('button', { name: /^Compare (one-|up to)/ });
const numeric = (text: string) => Number(text.replaceAll(',', ''));
const stat = (page: Page, label: string) => page.locator('.stat-row').filter({ hasText: new RegExp(`^${label}`) }).locator('dd');
const saved = (page: Page) => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).state as FixtureState);
const ring = 'Rarity: RARE\nSynthetic Continuity Ring A\nSapphire Ring\n--------\nRequirements:\nLevel: 60\nDex: 90\n--------\nItem Level: 100\nImplicits: 1\n+30% to Cold Resistance\n--------\n20% increased Fire Damage\n+100 to maximum Life';
const bow = 'Rarity: RARE\nSynthetic Continuity Bow\nCrude Bow\n--------\nImplicits: 0\nAdds 40 to 80 Physical Damage';

async function syntheticBuild(page: Page, augmented = false) {
  // Synthetic import/items only; parsing, preparation, scoring and Apply use real WASM/data.
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.route('**/api/import/wegame', route => route.fulfill({ json: {
    format: 'wegame', version: 1, role: { level: 85, class_name: augmented ? 'Ranger' : 'Sorceress' },
    equipments: [
      { inventoryId: 'Weapon', baseType: 'Crude Bow', name: 'Synthetic Current Bow', frameType: 2,
        explicitMods: ['Adds 20 to 40 Physical Damage'],
        ...(augmented ? { sockets: [{ type: 'rune' }], socketedItems: [{ socket: 0, baseType: 'Perfect Iron Rune' }],
          runeMods: ['20% increased Physical Damage'] } : {}) },
      { inventoryId: 'Weapon2', baseType: 'Crude Bow', name: 'Synthetic Alternate Bow', frameType: 2,
        explicitMods: ['Adds 10 to 20 Physical Damage'] },
      { inventoryId: 'Ring', baseType: 'Sapphire Ring', name: 'Synthetic Strong Ring', frameType: 2,
        explicitMods: ['60% increased Fire Damage', '+80 to maximum Life'] },
      { inventoryId: 'Ring2', baseType: 'Sapphire Ring', name: 'Synthetic Weak Ring', frameType: 2, explicitMods: ['+20 to maximum Life'] },
      { inventoryId: 'Amulet', baseType: 'Amber Amulet', frameType: 0 },
    ],
    talent_tree: { hashes: [], quest_stats: [] }, jewel_data: '[]',
    skills: [{ baseType: augmented ? 'Ice Shot' : 'Fireball', support: false, properties: [{ type: 5, values: [['16', 0]] }] }],
  } }));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true })
    .fill('https://www.wegame.com.cn/helper/poe2/#/share/SyntheticReplacementContinuityFixture');
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 30_000 });
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await nav(page, 'Upgrades').click();
  await expect(input(page)).toBeVisible();
  await page.getByRole('button', { name: 'Max total DPS', exact: true }).click();
}

async function paste(page: Page, text: string) {
  await input(page).evaluate((element, value) => {
    const data = new DataTransfer();
    data.setData('text/plain', value);
    element.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }));
  }, text);
}

async function settled(page: Page) {
  await expect(replacement(page)).toHaveAttribute('aria-busy', 'false');
  await expect(page.getByRole('button', { name: /^Apply to build/ })).toBeEnabled();
}

async function add(page: Page, text: string, slot?: string) {
  await paste(page, text);
  await settled(page);
  if (slot) await page.locator('.replacement-position').filter({ hasText: new RegExp(`^${slot}`) }).click();
  await page.getByRole('button', { name: 'Add to combinations', exact: true }).click();
}

async function completeJoint(page: Page) {
  await jointRun(page).click();
  await expect(jointResults(page)).toBeVisible({ timeout: 90_000 });
  await expect(jointRun(page)).toBeEnabled();
}

async function awayAndBack(page: Page) {
  // First lazy visits detect Suspense hide/reveal receipt loss.
  for (const name of ['Skills', 'Calcs', 'Tree']) {
    await nav(page, name).click();
    await expect(input(page)).toHaveCount(0);
    await expect(page.getByRole('button', { name: /^Apply (to build|equipment plan)/ })).toHaveCount(0);
  }
  await nav(page, 'Upgrades').click();
  await expect(input(page)).toBeVisible();
}

async function noReceipts(page: Page) {
  await expect(replacement(page)).toHaveCount(0);
  await expect(jointResults(page)).toHaveCount(0);
  await expect(page.getByRole('button', { name: /^Apply (to build|equipment plan)/ })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Cancel comparison', exact: true })).toHaveCount(0);
  await expect(calculate(page)).toBeEnabled();
}

async function assertExactItems(page: Page, before: FixtureState, items: SlotItemInput[]) {
  // Item order is not equip semantics; all item payloads and all other state are exact.
  const bySlot = (a: SlotItemInput, b: SlotItemInput) => a.slot.localeCompare(b.slot);
  await expect.poll(async () => [...(await saved(page)).items].sort(bySlot)).toEqual(
    [...before.items.filter(item => !items.some(next => next.slot === item.slot)), ...items].sort(bySlot));
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  const after = await saved(page);
  expect({ ...after, items: before.items }).toEqual(before);
  await expect(page.locator('.calc-error')).toHaveCount(0);
}

test('complete text, nondefault ring, full receipt and limits survive Upgrades → Skills/Calcs/Tree and exact raw Apply', async ({ page }) => {
  await syntheticBuild(page);
  const before = await saved(page);
  await paste(page, ring);
  await settled(page);
  await expect(page.locator('.replacement-position').filter({ hasText: /^Ring 2/ })).toHaveAttribute('aria-pressed', 'true');
  await page.locator('.replacement-position').filter({ hasText: /^Ring 1/ }).click();
  const receipt = await replacement(page).innerText();
  const predictedDps = numeric(await page.locator('.upgrade-replacement-result tbody tr').nth(0).locator('td').nth(1).innerText());
  const predictedLife = numeric(await page.locator('.upgrade-replacement-result tbody tr').nth(2).locator('td').nth(1).innerText());
  const predictedEhp = numeric(await page.locator('.upgrade-replacement-result tbody tr').nth(1).locator('td').nth(1).innerText());
  await awayAndBack(page);
  await expect(input(page)).toHaveValue(ring);
  await expect(page.locator('.replacement-position').filter({ hasText: /^Ring 1/ })).toHaveAttribute('aria-pressed', 'true');
  await expect.poll(() => replacement(page).innerText()).toBe(receipt);
  expect(await saved(page)).toEqual(before);
  await page.getByRole('button', { name: 'Apply to build · Ring 1', exact: true }).click();
  await assertExactItems(page, before, [{ slot: 'ring1', text: ring }]);
  await expect.poll(async () => numeric(await stat(page, 'Total DPS').innerText())).toBeCloseTo(predictedDps, 0);
  await expect.poll(async () => numeric(await stat(page, 'Life').innerText())).toBe(Math.round(predictedLife));
  await expect.poll(async () => numeric(await stat(page, 'Effective HP').innerText())).toBe(Math.round(predictedEhp));
});

test('custom prepared augments and exact evaluated Apply survive navigation without changing unrelated sources', async ({ page }) => {
  await syntheticBuild(page, true);
  const before = await saved(page);
  await paste(page, bow);
  await settled(page);
  await page.getByRole('button', { name: 'Customize', exact: true }).click();
  await settled(page);
  await page.getByRole('button', { name: 'Socket 1', exact: true }).click();
  await page.locator('[role="option"][data-value="Iron Rune"]').click();
  await settled(page);
  await page.getByRole('button', { name: 'Add to combinations', exact: true }).click();
  const prepared = await planner(page).locator('.equipment-candidates pre').textContent();
  expect(prepared).toContain('Rune: Iron Rune');
  expect(prepared).not.toContain('Perfect Iron Rune');
  const receipt = await replacement(page).innerText();
  const predictedDps = numeric(await page.locator('.upgrade-replacement-result tbody tr').first().locator('td').nth(1).innerText());
  const predictedEhp = numeric(await page.locator('.upgrade-replacement-result tbody tr').nth(1).locator('td').nth(1).innerText());
  const predictedLife = numeric(await page.locator('.upgrade-replacement-result tbody tr').nth(2).locator('td').nth(1).innerText());
  await awayAndBack(page);
  await expect(input(page)).toHaveValue(bow);
  await expect(page.getByRole('button', { name: 'Customize', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('button', { name: 'Socket 1', exact: true })).toContainText('Iron Rune');
  await expect.poll(() => replacement(page).innerText()).toBe(receipt);
  await page.getByRole('button', { name: /^Apply to build/ }).click();
  await assertExactItems(page, before, [{ slot: 'weapon1', text: prepared! }]);
  await expect.poll(async () => numeric(await stat(page, 'Total DPS').innerText())).toBeCloseTo(predictedDps, 0);
  await expect.poll(async () => numeric(await stat(page, 'Effective HP').innerText())).toBe(Math.round(predictedEhp));
  await expect.poll(async () => numeric(await stat(page, 'Life').innerText())).toBe(Math.round(predictedLife));
});

test('equipment candidates, nondefault cap and complete joint receipt survive navigation and Apply exactly the displayed plan', async ({ page }) => {
  await syntheticBuild(page);
  await page.getByRole('button', { name: 'Max Life', exact: true }).click();
  const before = await saved(page);
  await add(page, ring, 'Ring 1');
  await add(page, ring.replace('Ring A', 'Ring B').replace('+100 to maximum Life', '+150 to maximum Life'), 'Ring 2');
  await add(page, 'Rarity: RARE\nSynthetic Continuity Amulet\nAmber Amulet\nImplicits: 0\n+100 to maximum Life');
  await cap(page).selectOption('3');
  await completeJoint(page);
  const candidates = await planner(page).locator('.equipment-candidates').innerText();
  const prepared = await planner(page).locator('.equipment-candidates > li').evaluateAll(elements => elements.map(element => ({
    label: element.querySelector('summary')!.textContent!, text: element.querySelector('pre')!.textContent!,
  })));
  const receipt = await jointResults(page).innerText();
  const comparison = await replacement(page).innerText();
  const best = planner(page).locator('.equipment-plan').first();
  for (const label of ['Ring A', 'Ring B', 'Amulet']) await expect(best.locator('h5')).toContainText(label);
  const expectedLife = numeric(await best.locator('[data-plan-stat="Life"]').innerText());
  const expectedDps = numeric(await best.locator('[data-plan-stat="TotalDPS"]').innerText());
  const expectedEhp = numeric(await best.locator('[data-plan-stat="TotalEHP"]').innerText());
  await awayAndBack(page);
  await expect(cap(page)).toHaveValue('3');
  await expect.poll(() => planner(page).locator('.equipment-candidates').innerText()).toBe(candidates);
  await expect.poll(() => jointResults(page).innerText()).toBe(receipt);
  await expect.poll(() => replacement(page).innerText()).toBe(comparison);
  expect(await saved(page)).toEqual(before);
  await best.getByRole('button', { name: 'Apply equipment plan', exact: true }).click();
  await assertExactItems(page, before, prepared.map(candidate => ({
    slot: candidate.label.includes('Ring 1') ? 'ring1' : candidate.label.includes('Ring 2') ? 'ring2' : 'amulet', text: candidate.text,
  })));
  await expect.poll(async () => numeric(await stat(page, 'Life').innerText())).toBe(Math.round(expectedLife));
  await expect.poll(async () => numeric(await stat(page, 'Total DPS').innerText())).toBeCloseTo(expectedDps, 0);
  await expect.poll(async () => numeric(await stat(page, 'Effective HP').innerText())).toBe(Math.round(expectedEhp));
  await expect(jointResults(page)).toHaveCount(0);
});

for (const [edit, maximum] of [['build', 2], ['goal', 1], ['goal', 2], ['goal', 3]] as const) {
  test(`hidden ${edit} edits invalidate replacement and completed cap-${maximum} equipment receipts without automatic recalculation`, async ({ page }) => {
    await syntheticBuild(page);
    await add(page, ring, 'Ring 1');
    await cap(page).selectOption(String(maximum));
    await completeJoint(page);
    const candidates = await planner(page).locator('.equipment-candidates').innerText();
    const before = await saved(page);
    if (edit === 'build') {
      await nav(page, 'Config').click();
      await page.locator('.config-modifier-add').click();
      const block = page.locator('.config-modifier-block').last();
      await block.locator('.config-extra-mods').fill('100% increased Fire Damage');
      await block.locator('.config-modifier-title input').click();
      await expect(page.locator('.topbar-busy')).toHaveCount(0);
    } else {
      await nav(page, 'Skills').click();
      const group = page.locator('.skill-group').first();
      await group.locator('.skill-group-title').click();
      await group.locator('.gem-optimizer-toggle').click();
      await page.getByRole('button', { name: 'Objective', exact: true }).click();
      await page.getByRole('option', { name: 'Max Life', exact: true }).click();
    }
    await nav(page, 'Upgrades').click();
    await noReceipts(page);
    await expect(input(page)).toHaveValue(ring);
    await expect(cap(page)).toHaveValue(String(maximum));
    await expect.poll(() => planner(page).locator('.equipment-candidates').innerText()).toBe(candidates);
    expect((await saved(page)).items).toEqual(before.items);
    await awayAndBack(page);
    await noReceipts(page);
    await calculate(page).click();
    await settled(page);
    await completeJoint(page);
  });
}

test('identical copied stages reset text, candidates, replacement caps and receipts in both directions', async ({ page }) => {
  await syntheticBuild(page);
  const before = await saved(page);
  await add(page, ring, 'Ring 1');
  await cap(page).selectOption('3');
  await completeJoint(page);
  await nav(page, 'Build').click();
  const originalStage = await page.locator('.workspace-current-stage').innerText();
  await page.getByRole('button', { name: 'Copy current stage', exact: true }).click();
  await page.getByRole('textbox', { name: 'Name', exact: true }).fill('Synthetic replacement copy');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await nav(page, 'Upgrades').click();
  await expect(input(page)).toHaveValue('');
  await expect(planner(page).locator('.equipment-candidates > li')).toHaveCount(0);
  await expect(cap(page)).toHaveValue('2');
  await expect(replacement(page)).toHaveCount(0);
  await expect(jointResults(page)).toHaveCount(0);
  expect((await saved(page)).items).toEqual(before.items);
  await add(page, ring.replace('Ring A', 'Copied Stage Ring'), 'Ring 2');
  await completeJoint(page);
  await page.getByRole('combobox', { name: 'Stage', exact: true }).selectOption({ label: originalStage });
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await expect(input(page)).toHaveValue('');
  await expect(planner(page).locator('.equipment-candidates > li')).toHaveCount(0);
  await expect(cap(page)).toHaveValue('2');
  await expect(replacement(page)).toHaveCount(0);
  await expect(jointResults(page)).toHaveCount(0);
  expect((await saved(page)).items).toEqual(before.items);
});

test('leaving cancels pending paste debounce and returning never calculates without a new action', async ({ page }) => {
  await syntheticBuild(page);
  const before = await saved(page);
  await page.clock.install({ time: new Date('2026-01-01T00:00:00Z') });
  await page.clock.pauseAt(new Date('2026-01-01T00:00:01Z'));
  await paste(page, ring);
  await nav(page, 'Calcs').click({ force: true });
  await page.clock.runFor(1000);
  await nav(page, 'Upgrades').click({ force: true });
  await page.clock.runFor(1000);
  await expect(input(page)).toHaveValue(ring);
  await noReceipts(page);
  expect(await saved(page)).toEqual(before);
  await page.clock.resume();
  await calculate(page).click();
  await settled(page);
});

type Work = 'replacement' | 'joint';

async function installContinuationGate(page: Page, work: Work) {
  // Hold only the existing zero-delay continuation after a real WASM batch.
  // No backend result substitution, fake scoring or fake progress.
  await page.evaluate(work => {
    const state = window as Window & { replacementHold: boolean; replacementReleases: (() => void)[] };
    const schedule = window.setTimeout.bind(window);
    state.replacementHold = true;
    state.replacementReleases = [];
    window.setTimeout = (callback, delay, ...args) => {
      const jointStatus = document.querySelector('.equipment-planner [role="status"]');
      const replacementBusy = document.querySelector('.replacement-input .trade-primary')?.textContent === 'Comparing positions…';
      if (state.replacementHold && delay === 0 && typeof callback === 'function'
        && (work === 'joint' ? !!jointStatus && /\d+\s*\/\s*\d+/.test(jointStatus.textContent ?? '') : replacementBusy)) {
        state.replacementHold = false;
        state.replacementReleases.push(() => callback(...args));
        return schedule(() => {}, 0);
      }
      return schedule(callback, delay, ...args);
    };
  }, work);
}

async function releaseContinuation(page: Page, index: number) {
  await page.evaluate(async index => {
    (window as Window & { replacementReleases: (() => void)[] }).replacementReleases[index]();
    await new Promise<void>(resolve => window.setTimeout(resolve, 0));
  }, index);
}

for (const work of ['replacement', 'joint'] as const) {
  test(`leaving cancels in-flight ${work} and its late real-WASM continuation cannot publish hidden or return receipts`, async ({ page }) => {
    await syntheticBuild(page);
    if (work === 'joint') await add(page, ring, 'Ring 1');
    else await input(page).fill(ring);
    const before = await saved(page);
    const start = work === 'joint' ? jointRun(page) : page.locator('.replacement-input .trade-primary');
    await installContinuationGate(page, work);
    await start.click();
    await expect.poll(() => page.evaluate(() =>
      (window as Window & { replacementReleases: (() => void)[] }).replacementReleases.length)).toBe(1);
    await nav(page, 'Calcs').click();
    await releaseContinuation(page, 0);
    await expect(input(page)).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Cancel comparison', exact: true })).toHaveCount(0);
    await nav(page, 'Upgrades').click();
    await expect(start).toBeEnabled();
    await expect(jointResults(page)).toHaveCount(0);
    if (work === 'replacement') await expect(replacement(page)).toHaveCount(0);
    expect(await saved(page)).toEqual(before);
    await awayAndBack(page);
    await expect(jointResults(page)).toHaveCount(0);
    if (work === 'replacement') await expect(replacement(page)).toHaveCount(0);
    await start.click();
    if (work === 'joint') await expect(jointResults(page)).toBeVisible({ timeout: 90_000 });
    else await settled(page);
    expect(await saved(page)).toEqual(before);
  });

  test(`leaving cancels in-flight ${work} real-WASM work and rejects delayed A while overlapping B stays current`, async ({ page }) => {
    await syntheticBuild(page);
    if (work === 'joint') await add(page, ring, 'Ring 1');
    else await input(page).fill(ring);
    const before = await saved(page);
    const start = work === 'joint' ? jointRun(page) : page.locator('.replacement-input .trade-primary');
    await installContinuationGate(page, work);
    await start.click();
    await expect.poll(() => page.evaluate(() =>
      (window as Window & { replacementReleases: (() => void)[] }).replacementReleases.length)).toBe(1);
    await nav(page, 'Calcs').click();
    await nav(page, 'Upgrades').click();
    await expect(jointResults(page)).toHaveCount(0);
    if (work === 'replacement') await expect(replacement(page)).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Cancel comparison', exact: true })).toHaveCount(0);
    await expect(start).toBeEnabled();
    await page.evaluate(() => { (window as Window & { replacementHold: boolean }).replacementHold = true; });
    await start.click();
    await expect.poll(() => page.evaluate(() =>
      (window as Window & { replacementReleases: (() => void)[] }).replacementReleases.length)).toBe(2);
    const status = work === 'joint' ? planner(page).getByRole('status') : page.locator('.replacement-input .trade-primary');
    const pending = await status.innerText();
    await releaseContinuation(page, 0);
    await expect(status).toHaveText(pending);
    await expect(start).toBeDisabled();
    await expect(page.getByRole('button', { name: 'Cancel comparison', exact: true })).toBeVisible();
    if (work === 'joint') await expect(jointResults(page)).toHaveCount(0);
    else await expect(replacement(page)).toHaveCount(0);
    await releaseContinuation(page, 1);
    if (work === 'joint') await expect(jointResults(page)).toBeVisible({ timeout: 90_000 });
    else await settled(page);
    await expect(page.getByRole('button', { name: 'Cancel comparison', exact: true })).toHaveCount(0);
    expect(await saved(page)).toEqual(before);
  });
}
