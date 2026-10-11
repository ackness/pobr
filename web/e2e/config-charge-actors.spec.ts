import { expect, test, type Locator, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { deflateSync, inflateSync } from 'node:zlib';
import type { ConfigInputValue, PassiveNode } from '../src/api/types';
import type { SavedSession } from '../src/hooks/useBuildSession';

const charges = ['Power', 'Frenzy', 'Endurance'] as const;
type Charge = typeof charges[number];
type Actor = 'player' | 'minions';
type Lang = 'en-US' | 'zh-TW' | 'zh-CN';
// Exact catalog-backed identities; no broad override/minions prefix contract.
const countKeys = {
  player: { Power: 'overridePowerCharges', Frenzy: 'overrideFrenzyCharges', Endurance: 'overrideEnduranceCharges' },
  minions: { Power: 'minionsOverridePowerCharges', Frenzy: 'minionsOverrideFrenzyCharges', Endurance: 'minionsOverrideEnduranceCharges' },
} as const;
const raw: Record<string, ConfigInputValue> = {
  enemyFireResist: '23',
  syntheticUnknownFalse: false, syntheticUnknownZero: 0,
  syntheticUnknownNumber: 7.5, syntheticUnknownString: '0042 <raw> & preserved',
};
const imported: Record<string, ConfigInputValue> = {
  ...raw,
  usePowerCharges: true, useFrenzyCharges: true, useEnduranceCharges: true,
  minionsUsePowerCharges: true, minionsUseFrenzyCharges: true, minionsUseEnduranceCharges: true,
  overridePowerCharges: '1', overrideFrenzyCharges: 1, overrideEnduranceCharges: '1',
  minionsOverridePowerCharges: 1, minionsOverrideFrenzyCharges: '1', minionsOverrideEnduranceCharges: 1,
};
const version = readFileSync('../data/CURRENT', 'utf8').trim();
const nodes: PassiveNode[] = JSON.parse(readFileSync(`../data/${version}/base/passive_tree.json`, 'utf8'));
const treeCatalog = JSON.parse(readFileSync(`../data/${version}/overlay/passive_jewels.json`, 'utf8'));
const root = nodes.find(node => node.id === treeCatalog.class_starts.witch)!.skill;
const nav = (page: Page, name: string) => page.getByRole('navigation').getByRole('button', { name, exact: true });
const hitDps = (page: Page) => page.locator('.main-skill-section .stat-row dd').first();
const saved = (page: Page): Promise<SavedSession['state']> => page.evaluate(() =>
  JSON.parse(localStorage.getItem('pobr-build-state')!).state);
const numeric = (text: string) => Number(text.replaceAll(',', ''));

// Synthetic imports are named fixtures, not substitutes for data or calculation.
// Only the PLAYER's existing 8% per-charge item modifier is a numeric consumer;
// no inherent charge bonus or numerical minion coverage is asserted.
function syntheticCode(config: Record<string, ConfigInputValue>, consumer = true, flatDamage = 0) {
  const xmlInputs = Object.entries({ ...raw, ...config }).map(([name, value]) => {
    const type = typeof value === 'boolean' ? 'boolean' : typeof value === 'number' ? 'number' : 'string';
    const escaped = String(value).replaceAll('&', '&amp;').replaceAll('"', '&quot;').replaceAll('<', '&lt;');
    return `<Input name="${name}" ${type}="${escaped}"/>`;
  }).join('');
  const perCharge = consumer ? charges.map(charge => `8% increased Damage per ${charge} Charge`).join('\n') : '';
  return deflateSync(`<PathOfBuilding2>
    <Build level="85" className="Witch" ascendClassName="None" mainSocketGroup="1"/>
    <Tree activeSpec="1"><Spec nodes="${root}" treeVersion="0_5"/></Tree>
    <Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true">
      <Gem skillId="FireballPlayer" gemId="Metadata/Items/Gems/SkillGemFireball" level="16" quality="0" enabled="true"/>
    </Skill></SkillSet></Skills>
    <Items activeItemSet="1"><Item id="1">Rarity: RARE
Synthetic Actor Charge Count Ring
Sapphire Ring
Implicits: 0
40% increased Fire Damage
+80 to maximum Life
${perCharge}
${flatDamage ? `${flatDamage}% increased Damage` : ''}</Item><ItemSet id="1"><Slot name="Ring 1" itemId="1"/></ItemSet></Items>
    <Config>${xmlInputs}</Config>
  </PathOfBuilding2>`).toString('base64url');
}

async function idle(page: Page) {
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await expect(page.locator('.workspace-saved')).toBeVisible();
  await expect(page.locator('.boot-error, .calc-error, .workspace-save-error')).toHaveCount(0);
}

async function ready(page: Page, lang: Lang = 'en-US') {
  await expect(nav(page, lang === 'en-US' ? 'Build' : lang === 'zh-TW' ? '構築' : '构筑')).toBeEnabled({ timeout: 90_000 });
  await idle(page);
}

async function language(page: Page, lang: Lang) {
  for (let attempts = 0; attempts < 3; attempts++) {
    const current = await page.evaluate(() => localStorage.getItem('pobr-lang') ?? 'en-US');
    if (current === lang) return;
    await page.getByRole('button', { name: 'Switch language', exact: true }).click();
    await expect.poll(() => page.evaluate(() => localStorage.getItem('pobr-lang') ?? 'en-US')).not.toBe(current);
  }
  await expect.poll(() => page.evaluate(() => localStorage.getItem('pobr-lang') ?? 'en-US')).toBe(lang);
}

async function importCode(page: Page, code: string) {
  // Import/share helpers use English; all changed Config interactions below use
  // the requested live locale and its actor-specific accessible identity.
  await language(page, 'en-US');
  await nav(page, 'Build').click();
  await page.getByRole('combobox', { name: 'Import into', exact: true }).selectOption('currentStage');
  page.once('dialog', dialog => dialog.accept());
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await expect(page.locator('.paper-doll')).toBeVisible({ timeout: 30_000 });
  await expect.poll(async () => (await saved(page)).pobCode).toBe(code);
  await idle(page);
}

function identity(lang: Lang, actor: Actor, charge: Charge) {
  const actorName = lang === 'en-US' ? (actor === 'player' ? '\\bPlayer\\b' : '\\bMinions\\b')
    : actor === 'player' ? '角色' : '召[唤喚]物';
  const chargeName = lang === 'en-US' ? `\\b${charge}\\b`
    : charge === 'Power' ? '暴[击擊]球' : charge === 'Frenzy' ? '狂怒球' : '耐力球';
  // Actor + charge identity, not punctuation, sentence structure or suffix wording.
  return new RegExp(`(?=.*${actorName})(?=.*${chargeName})`, 'i');
}

const control = (page: Page, lang: Lang, actor: Actor, charge: Charge) =>
  page.getByRole('spinbutton', { name: identity(lang, actor, charge) });
const row = (page: Page, input: Locator) => page.locator('.config-item').filter({ has: input });

async function openCharge(page: Page, lang: Lang, charge: Charge) {
  await nav(page, lang === 'en-US' ? 'Config' : '配置').click();
  // Show both actors together. The stable suffix is only a search query, never
  // the input locator: removing actor labels must make these scenarios fail.
  await page.locator('.config-search').fill(`${charge}Charges`);
  for (const actor of ['player', 'minions'] as const) {
    const input = control(page, lang, actor, charge);
    await expect(input).toHaveCount(1);
    await expect(input).toBeVisible();
    await expect(input).toBeEnabled();
    await expect(row(page, input).locator('.config-key')).toHaveText(identity(lang, actor, charge));
  }
}

async function keyboardEdit(page: Page, lang: Lang, actor: Actor, charge: Charge) {
  const input = control(page, lang, actor, charge);
  await input.focus();
  await input.press('ArrowUp');
  await input.press('Tab');
  await expect.poll(async () => (await saved(page)).params.config_inputs[countKeys[actor][charge]]).toBe(2);
  await idle(page);
}

async function keyboardReset(page: Page, lang: Lang, actor: Actor, charge: Charge) {
  const input = control(page, lang, actor, charge);
  await input.focus();
  await input.press('Tab');
  const reset = row(page, input).getByRole('button', { name: identity(lang, actor, charge) });
  await expect(reset).toBeFocused();
  await reset.press('Enter');
  await expect(reset).toHaveCount(0);
  await expect.poll(async () => (await saved(page)).params.config_inputs[countKeys[actor][charge]])
    .toBe(imported[countKeys[actor][charge]]);
  await idle(page);
}

async function noOverflow(page: Page) {
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
}

async function flatControls(page: Page, stacks: readonly number[]) {
  // Independent flat modifiers bypass count resolution entirely.
  const values = new Map<number, string>();
  for (const count of stacks) {
    await importCode(page, syntheticCode({}, false, 8 * count));
    values.set(count, await hitDps(page).innerText());
  }
  return values;
}

async function shareCode(page: Page) {
  await language(page, 'en-US');
  await nav(page, 'Build').click();
  await page.getByRole('button', { name: 'Generate share code', exact: true }).click();
  return page.getByRole('textbox', { name: 'Share Code', exact: true }).inputValue();
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
  // No mocks for imports, data, WASM, calculation or persistence.
  await page.route('**/api/trade/leagues?realm=*', route => route.fulfill({ json: { leagues: ['Standard'] } }));
  await page.goto('/');
  await ready(page);
});

for (const lang of ['en-US', 'zh-TW', 'zh-CN'] as const) {
  for (const width of [1440, 390]) {
    test(`${lang} ${width}px: semantic actor keyboard edits/reset preserve opposite raw counts and typed roundtrip`, async ({ page }) => {
      test.setTimeout(300_000);
      await page.setViewportSize({ width, height: 780 });
      const expected = await flatControls(page, [6, 7, 8]);
      expect(numeric(expected.get(8)!)).toBeGreaterThan(numeric(expected.get(7)!));
      expect(numeric(expected.get(7)!)).toBeGreaterThan(numeric(expected.get(6)!));
      await importCode(page, syntheticCode(imported));
      const original = await saved(page);
      expect(original.params.config_inputs).toMatchObject(imported);
      // Raw string counts remain provenance, not numeric overrides: the two
      // player strings use max3, while numeric Frenzy=1 yields 7 item stacks.
      await expect(hitDps(page)).toHaveText(expected.get(7)!);
      await language(page, lang);
      for (const charge of charges) {
        await test.step(`${charge}: both actors remain distinct through untouched blur and keyboard edit/reset`, async () => {
          await openCharge(page, lang, charge);
          for (const actor of ['player', 'minions'] as const) {
            const input = control(page, lang, actor, charge);
            await input.focus();
            await input.press('Tab');
            await idle(page);
            expect(await saved(page)).toEqual(original);
          }
          await keyboardEdit(page, lang, 'minions', charge);
          expect((await saved(page)).params.config_inputs).toEqual({
            ...original.params.config_inputs, [countKeys.minions[charge]]: 2,
          });
          await expect(hitDps(page)).toHaveText(expected.get(7)!);
          await noOverflow(page);
          await keyboardReset(page, lang, 'minions', charge);
          expect(await saved(page)).toEqual(original);
          await expect(hitDps(page)).toHaveText(expected.get(7)!);
          await keyboardEdit(page, lang, 'player', charge);
          expect((await saved(page)).params.config_inputs).toEqual({
            ...original.params.config_inputs, [countKeys.player[charge]]: 2,
          });
          await expect(hitDps(page)).toHaveText(expected.get(charge === 'Frenzy' ? 8 : 6)!);
          await noOverflow(page);
          await keyboardReset(page, lang, 'player', charge);
          expect(await saved(page)).toEqual(original);
          await expect(hitDps(page)).toHaveText(expected.get(7)!);
        });
      }
      await test.step('semantic actor search refreshes when the live language changes', async () => {
        const search = page.locator('.config-search');
        for (const actor of ['player', 'minions'] as const) {
          const query = lang === 'en-US' ? (actor === 'player' ? 'Player' : 'Minions')
            : actor === 'player' ? '角色' : lang === 'zh-TW' ? '召喚物' : '召唤物';
          await search.fill(query);
          for (const charge of charges) {
            await expect(control(page, lang, actor, charge)).toBeVisible();
            await expect(control(page, lang, actor === 'player' ? 'minions' : 'player', charge)).toHaveCount(0);
          }
        }
        // A localized player query has no raw-key/original-catalog equivalent
        // in the other language. Preserve the query while switching, so stale
        // memoized filtering cannot keep the old player's numeric rows visible.
        const query = lang === 'en-US' ? 'Player' : '角色';
        await search.fill(query);
        const otherLang = lang === 'en-US' ? 'zh-TW' : 'en-US';
        await language(page, otherLang);
        await expect(search).toHaveValue(query);
        for (const charge of charges) {
          for (const actor of ['player', 'minions'] as const) {
            await expect(control(page, otherLang, actor, charge)).toHaveCount(0);
          }
        }
        await language(page, lang);
        await expect(search).toHaveValue(query);
        for (const charge of charges) {
          await expect(control(page, lang, 'player', charge)).toBeVisible();
          await expect(control(page, lang, 'minions', charge)).toHaveCount(0);
        }
        await idle(page);
        expect(await saved(page)).toEqual(original);
      });
      await page.reload();
      await ready(page, lang);
      expect(await saved(page)).toEqual(original);
      await expect(hitDps(page)).toHaveText(expected.get(7)!);
      await openCharge(page, lang, 'Power');
      await row(page, control(page, lang, 'player', 'Power')).evaluate(element => element.scrollIntoView({ block: 'end' }));
      await noOverflow(page);
      await page.screenshot({ path: test.info().outputPath(`${lang}-${width}-charge-actors.png`), fullPage: true });
      // Keep one deliberate edit while retaining mixed number/string provenance
      // on the other five actor counts through the actual PoB XML roundtrip.
      await keyboardEdit(page, lang, 'player', 'Power');
      const edited = await saved(page);
      await expect(hitDps(page)).toHaveText(expected.get(6)!);
      await page.reload();
      await ready(page, lang);
      expect(await saved(page)).toEqual(edited);
      const shared = await shareCode(page);
      expect(await exportedInputs(page, shared)).toMatchObject(edited.params.config_inputs);
      await importCode(page, shared);
      expect((await saved(page)).params.config_inputs).toMatchObject(edited.params.config_inputs);
      await expect(hitDps(page)).toHaveText(expected.get(6)!);
      const reimported = await saved(page);
      await language(page, lang);
      for (const charge of charges) {
        await openCharge(page, lang, charge);
        for (const actor of ['player', 'minions'] as const) {
          const input = control(page, lang, actor, charge);
          await input.focus();
          await input.press('Tab');
          await idle(page);
          expect(await saved(page)).toEqual(reimported);
        }
      }
    });
  }
}

test('all six actor counts stay stage-isolated through switching and full-state reload', async ({ page }) => {
  test.setTimeout(300_000);
  await page.setViewportSize({ width: 1440, height: 780 });
  const expected = await flatControls(page, [6, 7]);
  await importCode(page, syntheticCode(imported));
  const original = await saved(page);
  await expect(hitDps(page)).toHaveText(expected.get(7)!);
  await nav(page, 'Build').click();
  const originalStage = await page.locator('.workspace-current-stage').innerText();
  await page.getByRole('button', { name: 'Copy current stage', exact: true }).click();
  await page.getByRole('textbox', { name: 'Name', exact: true }).fill('Synthetic independent actor counts');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await idle(page);
  const editedStage = await page.getByRole('combobox', { name: 'Stage', exact: true }).locator('option:checked').innerText();
  const editedInputs = { ...original.params.config_inputs };
  for (const charge of charges) {
    await openCharge(page, 'en-US', charge);
    const beforeMinions = await hitDps(page).innerText();
    await keyboardEdit(page, 'en-US', 'minions', charge);
    editedInputs[countKeys.minions[charge]] = 2;
    expect((await saved(page)).params.config_inputs).toEqual(editedInputs);
    await expect(hitDps(page)).toHaveText(beforeMinions);
    await keyboardEdit(page, 'en-US', 'player', charge);
    editedInputs[countKeys.player[charge]] = 2;
    expect((await saved(page)).params.config_inputs).toEqual(editedInputs);
    await expect(hitDps(page)).toHaveText(expected.get(charge === 'Frenzy' ? 7 : 6)!);
  }
  const edited = await saved(page);
  await page.reload();
  await ready(page);
  expect(await saved(page)).toEqual(edited);
  await expect(hitDps(page)).toHaveText(expected.get(6)!);
  await nav(page, 'Build').click();
  await page.getByRole('combobox', { name: 'Stage', exact: true }).selectOption({ label: originalStage });
  await idle(page);
  expect(await saved(page)).toEqual(original);
  await expect(hitDps(page)).toHaveText(expected.get(7)!);
  await page.reload();
  await ready(page);
  expect(await saved(page)).toEqual(original);
  await nav(page, 'Build').click();
  await page.getByRole('combobox', { name: 'Stage', exact: true }).selectOption({ label: editedStage });
  await idle(page);
  expect(await saved(page)).toEqual(edited);
  await expect(hitDps(page)).toHaveText(expected.get(6)!);
});
