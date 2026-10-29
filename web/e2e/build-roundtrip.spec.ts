import { expect, test, type Page } from '@playwright/test';
import { openItemsAfterImport, pageLink, statValue } from './shell';
import { openBuild, openExport, openLoadouts, stableText } from './buildLibrary';
import { deflateSync, inflateSync } from 'node:zlib';
import { readFileSync } from 'node:fs';

const skill = (name: string, set?: 1 | 2) => `<Skill enabled="true" set1="${set !== 2}" set2="${set !== 1}"><Gem skillId="${name}Player" gemId="Metadata/Items/Gems/SkillGem${name}" level="5" quality="0" enabled="true"/></Skill>`;
const xml = `<PathOfBuilding2>
  <Build level="20" className="Witch" mainSocketGroup="1"/>
  <Tree activeSpec="1"><Spec title="A" nodes="1,2" treeVersion="0_5"/><Spec title="B" nodes="3,4" treeVersion="0_5"/></Tree>
  <Skills activeSkillSet="1"><SkillSet id="1" title="A">${skill('Spark')}</SkillSet><SkillSet id="2" title="B">${skill('Fireball')}${skill('Firestorm')}</SkillSet></Skills>
  <Items activeItemSet="1"><ItemSet id="1" title="A"/><ItemSet id="2" title="B"/></Items>
  <Notes>Original notes</Notes>
</PathOfBuilding2>`;

const mainSkill = (page: Page) => page.getByRole('combobox', { name: 'Inspected skill', exact: true });
const hitDps = (page: Page) => statValue(page, /^Hit DPS$/).first();
/** Opens the Imported PoB loadouts section of the Build page and switches to one of them by its position. */
async function switchLoadout(page: Page, index: number) {
  const picker = page.getByRole('combobox', { name: 'PoB loadout', exact: true });
  await picker.click();
  await page.getByRole('option').nth(index).click();
  await expect(page.locator('details.bd-details select')).toHaveValue(String(index));
  await expect(picker).toBeEnabled();
}
const loadoutValue = (page: Page) => page.locator('details.bd-details select');

async function importCode(page: Page, code: string) {
  await openBuild(page);
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await openItemsAfterImport(page);
  await expect(page.getByRole('heading', { name: 'Equipment', exact: true })).toBeVisible();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
}

async function saved(page: Page) {
  return page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!));
}

test('body armour granted mitigation follows chest rarity after import and editing', async ({ page }) => {
  const source = `<PathOfBuilding2>
    <Build level="85" className="Warrior" mainSocketGroup="1"/>
    <Items activeItemSet="1">
      <Item id="1">Rarity: NORMAL
Plate Vest
Implicits: 0</Item>
      <Item id="2">Rarity: RARE
Rule Acceptance Ring
Sapphire Ring
Implicits: 0
Body Armour grants Hits against you have 100% reduced Critical Damage Bonus</Item>
      <ItemSet id="1"><Slot name="Body Armour" itemId="1"/><Slot name="Ring 1" itemId="2"/></ItemSet>
    </Items>
  </PathOfBuilding2>`;
  await page.goto('/build');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await importCode(page, deflateSync(source).toString('base64url'));
  const ehp = statValue(page, /^Effective HP$/).first();
  const normalEhp = (await stableText(ehp))!;
  const number = (text: string) => Number(text.replaceAll(',', ''));
  expect(number(normalEhp)).toBeGreaterThan(0);

  for (const rarity of ['MAGIC', 'RARE', 'UNIQUE', 'NORMAL']) {
    await page.locator('.paper-doll').getByRole('button', { name: 'Body Armour', exact: true }).click();
    await page.locator('.item-detail').getByRole('button', { name: 'Edit', exact: true }).click();
    await page.getByRole('textbox', { name: 'bodyarmour item text' }).fill(
      `Rarity: ${rarity}\n${['NORMAL', 'MAGIC'].includes(rarity) ? '' : 'Rule Acceptance Chest\n'}Plate Vest\nImplicits: 0`,
    );
    await page.getByRole('button', { name: 'Apply changes', exact: true }).click();
    await expect(page.locator('.item-editor')).toHaveCount(0);
    await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
    if (rarity === 'NORMAL') {
      await expect(ehp).toHaveText(normalEhp);
    } else {
      await expect.poll(async () => number(await ehp.innerText())).toBeLessThan(number(normalEhp));
    }
  }
  await page.reload();
  await expect(ehp).toHaveText(normalEhp, { timeout: 90_000 });
});

test('gem quality edits recalculate real WASM results and restore the baseline', async ({ page }) => {
  const code = readFileSync(new URL('../../examples/demo-bd-test/builds/mercenary-gemling-legionnaire-explosive-grenade/code.txt', import.meta.url), 'utf8').trim();
  const zeroQuality = inflateSync(Buffer.from(code, 'base64url')).toString('utf8').replace(/quality="\d+"/g, 'quality="0"');
  await page.goto('/build');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await importCode(page, deflateSync(zeroQuality).toString('base64url'));
  const dps = hitDps(page);
  const baseline = await stableText(dps);
  await pageLink(page, 'Skills').click();
  const main = page.locator('.skill-group.is-main');
  await expect(main.locator('.skill-group-name')).toHaveText('Explosive Grenade');
  await main.locator('.skill-group-title').click();
  const quality = main.getByRole('spinbutton', { name: 'Quality', exact: true }).first();
  await expect(quality).toHaveValue('0');
  await quality.fill('20');
  await expect(dps).not.toHaveText(baseline!, { timeout: 30_000 });
  await quality.fill('0');
  await expect(dps).toHaveText(baseline!, { timeout: 30_000 });
});

test('an imported trigger group retains its main active skill through edits, reload and share', async ({ page }) => {
  const source = readFileSync(new URL('../../crates/pobr-build/tests/fixtures/coc_cast_on_crit.xml', import.meta.url), 'utf8');
  await page.goto('/build');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await importCode(page, deflateSync(source).toString('base64url'));
  await expect(mainSkill(page)).toContainText('Fireball');
  expect((await saved(page)).state.socketGroups[0].main_active_skill).toBe(3);
  const dps = await stableText(hitDps(page));

  // Earlier saves omitted this metadata; restore it only for unchanged gem order.
  await page.evaluate(() => {
    const snapshot = JSON.parse(localStorage.getItem('pobr-v2-build-state')!);
    delete snapshot.state.socketGroups[0].main_active_skill;
    localStorage.setItem('pobr-v2-build-state', JSON.stringify(snapshot));
  });
  await page.reload();
  await expect(mainSkill(page)).toContainText('Fireball', { timeout: 90_000 });
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  expect((await saved(page)).state.socketGroups[0].main_active_skill).toBe(3);
  await expect(hitDps(page)).toHaveText(dps!);

  await openBuild(page);
  await openExport(page);
  await page.getByRole('button', { name: 'Generate share code', exact: true }).click();
  const code = await page.getByRole('textbox', { name: 'Share Code', exact: true }).inputValue();
  expect(inflateSync(Buffer.from(code, 'base64url')).toString('utf8')).toContain('mainActiveSkill="3"');
  await importCode(page, code);
  await expect(mainSkill(page)).toContainText('Fireball');
  await expect(hitDps(page)).toHaveText(dps!);

  await pageLink(page, 'Skills').click();
  await expect(page.locator('.skill-group-name')).toHaveText('Fireball');
  await page.locator('.skill-group-title').click();
  // Removing the preceding attack must shift the active ordinal, not select the meta shell.
  await page.locator('.skill-gems .skill-remove').first().click();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  expect((await saved(page)).state.socketGroups[0].main_active_skill).toBe(2);
  await expect(mainSkill(page)).toContainText('Fireball');
});

test('switch, reload, edit and share preserve the selected loadout and global fields', async ({ page }) => {
  await page.goto('/build');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await importCode(page, deflateSync(xml).toString('base64url'));
  await openBuild(page);
  await openLoadouts(page);
  await switchLoadout(page, 1);

  await page.reload();
  await expect(page.locator('details.bd-details summary')).toBeVisible({ timeout: 90_000 });
  await openLoadouts(page);
  await expect(loadoutValue(page)).toHaveValue('1');
  await expect(page.getByRole('combobox', { name: 'PoB loadout', exact: true })).toBeEnabled();
  await page.getByLabel('Level', { exact: true }).fill('90');
  await page.getByRole('textbox', { name: 'Notes', exact: true }).fill('Edited <notes> & 中文');
  await pageLink(page, 'Config').click();
  await page.getByLabel('Search config options…').fill('Onslaught');
  await page.locator('.config-item input[type="checkbox"]').first().check();
  // The enemy is chosen with the fight presets above the options ("Mapping" is the tier without a boss).
  await page.getByRole('group', { name: 'Enemy to fight', exact: true }).getByRole('button', { name: 'Mapping', exact: true }).click();
  await page.getByRole('tab', { name: 'Custom modifiers', exact: true }).click();
  await page.locator('.config-modifier-add').click();
  await page.getByRole('textbox', { name: 'Group title', exact: true }).fill('Default');
  await page.getByRole('textbox', { name: 'Group title', exact: true }).blur();
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  await page.locator('.config-extra-mods').fill('+100 to maximum Life');
  await pageLink(page, 'Skills').click();
  await page.locator('.skill-main-toggle').nth(1).click();
  await expect(page.locator('.skill-group.is-main .skill-group-name')).toHaveText('Firestorm');
  await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
  const before = await saved(page);
  const beforeDps = await stableText(hitDps(page));
  expect(before.state.params.enemy_tier).toBe('none');
  expect(before.state.params.custom_modifier_blocks).toEqual([{ title: 'Default', enabled: true, text: '+100 to maximum Life' }]);

  await openBuild(page);
  await openExport(page);
  await page.getByRole('button', { name: 'Generate share code', exact: true }).click();
  const code = await page.getByRole('textbox', { name: 'Share Code', exact: true }).inputValue();
  const exported = inflateSync(Buffer.from(code, 'base64url')).toString('utf8');
  expect(exported).toContain('<Spec title="A" nodes="1,2" treeVersion="0_5"/>');
  expect(exported.match(/<SkillSet\b[^>]*id="2"/g)).toHaveLength(1);
  expect(exported.match(/<ItemSet\b[^>]*id="2"/g)).toHaveLength(1);
  expect(exported).toContain('<Input name="enemyIsBoss" string="None"/>');
  expect(exported).toContain('<CustomModifierBlock title="Default" enabled="true">+100 to maximum Life</CustomModifierBlock>');

  await importCode(page, code);
  const after = await saved(page);
  expect(after.state.character).toEqual(before.state.character);
  expect(after.state.params.main_socket_group).toBe(before.state.params.main_socket_group);
  // Export also materializes default-false condition keys.
  expect(after.state.params.config_inputs).toMatchObject(before.state.params.config_inputs);
  expect(after.state.params.enemy_tier).toBe('none');
  expect(after.state.params.custom_modifier_blocks).toEqual([{ title: 'Default', enabled: true, text: '+100 to maximum Life' }]);
  expect(after.state.socketGroups).toEqual(before.state.socketGroups);
  expect(after.notes).toBe(before.notes);
  await expect(hitDps(page)).toHaveText(beforeDps!);
  await openBuild(page);
  await openLoadouts(page);
  await expect(loadoutValue(page)).toHaveValue('1');

  await switchLoadout(page, 0);
  const other = await saved(page);
  expect(other.state.allocatedNodes).toEqual([1, 2]);
  expect(other.state.socketGroups[0].gems[0].skill_id).toBe('SparkPlayer');
});

test('removing groups preserves the main skill and weapon binding until that skill is removed', async ({ page }) => {
  const source = `<PathOfBuilding2><Build level="20" className="Witch" mainSocketGroup="2"/>
    <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
    <Skills activeSkillSet="1"><SkillSet id="1">${skill('Spark', 1)}${skill('Fireball', 2)}${skill('Firestorm', 1)}</SkillSet></Skills>
    <Items activeItemSet="1"><ItemSet id="1"/></Items></PathOfBuilding2>`;
  await page.goto('/build');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await importCode(page, deflateSync(source).toString('base64url'));
  await pageLink(page, 'Skills').click();
  await expect(page.locator('.skill-group.is-main .skill-group-name')).toHaveText('Fireball');

  // A preceding group disappears; the selected skill and set must remain the same.
  await page.locator('.skill-group .skill-remove').first().click();
  await expect(page.locator('.skill-group')).toHaveCount(2);
  await expect(page.locator('.skill-group.is-main .skill-group-name')).toHaveText('Fireball');
  let state = (await saved(page)).state;
  expect(state.params.main_socket_group).toBe(0);
  expect(state.weaponSwap.active).toBe(2);

  // Deleting the selected skill chooses a remaining enabled group and follows its binding.
  await page.locator('.skill-group .skill-remove').first().click();
  await expect(page.locator('.skill-group')).toHaveCount(1);
  await expect(page.locator('.skill-group.is-main .skill-group-name')).toHaveText('Firestorm');
  state = (await saved(page)).state;
  expect(state.params.main_socket_group).toBe(0);
  expect(state.weaponSwap.active).toBe(1);

  await page.locator('.skill-group .skill-remove').first().click();
  await expect(page.locator('.skill-group')).toHaveCount(0);
  expect((await saved(page)).state.params.main_socket_group).toBeUndefined();
});
