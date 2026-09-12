import { expect, test, type Page } from '@playwright/test';
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

async function importCode(page: Page, code: string) {
  await page.getByRole('button', { name: 'Build', exact: true }).click();
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await expect(page.getByRole('heading', { name: 'Items', exact: true })).toBeVisible();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
}

async function saved(page: Page) {
  return page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!));
}

test('an imported trigger group retains its main active skill through edits, reload and share', async ({ page }) => {
  const source = readFileSync(new URL('../../crates/pobr-build/tests/fixtures/coc_cast_on_crit.xml', import.meta.url), 'utf8');
  await page.goto('/');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await importCode(page, deflateSync(source).toString('base64url'));
  await expect(page.locator('.main-skill-select option:checked')).toContainText('Fireball');
  expect((await saved(page)).state.socketGroups[0].main_active_skill).toBe(3);
  const dps = await page.locator('.main-skill-section .stat-row dd').first().textContent();

  // Earlier saves omitted this metadata; restore it only for unchanged gem order.
  await page.evaluate(() => {
    const snapshot = JSON.parse(localStorage.getItem('pobr-build-state')!);
    delete snapshot.state.socketGroups[0].main_active_skill;
    localStorage.setItem('pobr-build-state', JSON.stringify(snapshot));
  });
  await page.reload();
  await expect(page.locator('.main-skill-select option:checked')).toContainText('Fireball', { timeout: 90_000 });
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  expect((await saved(page)).state.socketGroups[0].main_active_skill).toBe(3);
  await expect(page.locator('.main-skill-section .stat-row dd').first()).toHaveText(dps!);

  await page.getByRole('button', { name: 'Build', exact: true }).click();
  await page.getByRole('button', { name: 'Generate code', exact: true }).click();
  const code = await page.getByRole('textbox', { name: 'Share Code', exact: true }).inputValue();
  expect(inflateSync(Buffer.from(code, 'base64url')).toString('utf8')).toContain('mainActiveSkill="3"');
  await importCode(page, code);
  await expect(page.locator('.main-skill-select option:checked')).toContainText('Fireball');
  await expect(page.locator('.main-skill-section .stat-row dd').first()).toHaveText(dps!);

  await page.getByRole('button', { name: 'Skills', exact: true }).click();
  await expect(page.locator('.skill-group-name')).toHaveText('Fireball');
  await page.locator('.skill-group-title').click();
  // Removing the preceding attack must shift the active ordinal, not select the meta shell.
  await page.locator('.skill-gems .skill-remove').first().click();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  expect((await saved(page)).state.socketGroups[0].main_active_skill).toBe(2);
  await expect(page.locator('.main-skill-select option:checked')).toContainText('Fireball');
});

test('switch, reload, edit and share preserve the selected loadout and global fields', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await importCode(page, deflateSync(xml).toString('base64url'));
  await page.getByLabel('Loadout', { exact: true }).selectOption('1');
  await expect(page.getByLabel('Loadout', { exact: true })).toHaveValue('1');
  await expect(page.getByLabel('Loadout', { exact: true })).toBeEnabled();

  await page.reload();
  await expect(page.getByLabel('Loadout', { exact: true })).toHaveValue('1', { timeout: 90_000 });
  await expect(page.getByLabel('Loadout', { exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Build', exact: true }).click();
  await page.getByLabel('Level', { exact: true }).fill('90');
  await page.getByRole('textbox', { name: 'Notes', exact: true }).fill('Edited <notes> & 中文');
  await page.getByRole('button', { name: 'Config', exact: true }).click();
  await page.getByLabel('Search config options…').fill('Onslaught');
  await page.locator('.config-item input[type="checkbox"]').first().check();
  await page.getByRole('button', { name: 'Skills', exact: true }).click();
  await page.locator('.skill-main-toggle').nth(1).click();
  await expect(page.locator('.skill-group.is-main .skill-group-name')).toHaveText('Firestorm');
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  const before = await saved(page);

  await page.getByRole('button', { name: 'Build', exact: true }).click();
  await page.getByRole('button', { name: 'Generate code', exact: true }).click();
  const code = await page.getByRole('textbox', { name: 'Share Code', exact: true }).inputValue();
  const exported = inflateSync(Buffer.from(code, 'base64url')).toString('utf8');
  expect(exported).toContain('<Spec title="A" nodes="1,2" treeVersion="0_5"/>');
  expect(exported.match(/<SkillSet\b[^>]*id="2"/g)).toHaveLength(1);
  expect(exported.match(/<ItemSet\b[^>]*id="2"/g)).toHaveLength(1);

  await importCode(page, code);
  const after = await saved(page);
  expect(after.state.character).toEqual(before.state.character);
  expect(after.state.params.main_socket_group).toBe(before.state.params.main_socket_group);
  // Export also materializes default-false condition keys.
  expect(after.state.params.config_inputs).toMatchObject(before.state.params.config_inputs);
  expect(after.state.socketGroups).toEqual(before.state.socketGroups);
  expect(after.notes).toBe(before.notes);
  await expect(page.getByLabel('Loadout', { exact: true })).toHaveValue('1');

  await page.getByLabel('Loadout', { exact: true }).selectOption('0');
  await expect(page.getByLabel('Loadout', { exact: true })).toHaveValue('0');
  await expect(page.getByLabel('Loadout', { exact: true })).toBeEnabled();
  const other = await saved(page);
  expect(other.state.allocatedNodes).toEqual([1, 2]);
  expect(other.state.socketGroups[0].gems[0].skill_id).toBe('SparkPlayer');
});

test('removing groups preserves the main skill and weapon binding until that skill is removed', async ({ page }) => {
  const source = `<PathOfBuilding2><Build level="20" className="Witch" mainSocketGroup="2"/>
    <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
    <Skills activeSkillSet="1"><SkillSet id="1">${skill('Spark', 1)}${skill('Fireball', 2)}${skill('Firestorm', 1)}</SkillSet></Skills>
    <Items activeItemSet="1"><ItemSet id="1"/></Items></PathOfBuilding2>`;
  await page.goto('/');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await importCode(page, deflateSync(source).toString('base64url'));
  await page.getByRole('button', { name: 'Skills', exact: true }).click();
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
