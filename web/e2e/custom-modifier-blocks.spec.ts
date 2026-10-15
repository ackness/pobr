import { expect, test } from '@playwright/test';
import { deflateSync, inflateSync } from 'node:zlib';

const source = `<PathOfBuilding2>
  <Build level="40" className="Witch"/>
  <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
  <Skills/><Items/>
  <Config>
    <CustomModifierBlock title="Active" enabled="true">+100 to maximum Life</CustomModifierBlock>
    <CustomModifierBlock title="Saved for later" enabled="false">+200 to maximum Life</CustomModifierBlock>
  </Config>
</PathOfBuilding2>`;

test('custom modifier groups retain titles, text, order and disabled state through edits and sharing', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await page.getByRole('button', { name: 'Build', exact: true }).click();
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(deflateSync(source).toString('base64url'));
  await page.locator('.import-submit').click();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);

  const life = page.locator('.stat-row').filter({ hasText: /^Life(?=[0-9\s])/ }).locator('dd');
  const number = (value: string) => Number(value.replaceAll(',', ''));
  const activeLife = number(await life.innerText());
  await page.getByRole('button', { name: 'Config', exact: true }).click();
  const groups = page.locator('.config-modifier-block');
  await expect(groups).toHaveCount(2);
  await expect(groups.nth(0).locator('.config-modifier-title input')).toHaveValue('Active');
  await expect(groups.nth(1).locator('.config-modifier-title input')).toHaveValue('Saved for later');
  await expect(groups.nth(1).locator('.config-modifier-enabled input')).not.toBeChecked();

  await groups.nth(0).locator('.config-modifier-enabled input').uncheck();
  await expect.poll(async () => number(await life.innerText())).toBeLessThan(activeLife);
  const baseLife = number(await life.innerText());
  await groups.nth(1).locator('.config-modifier-enabled input').check();
  await expect.poll(async () => number(await life.innerText())).toBeGreaterThan(baseLife);

  const title = groups.nth(1).locator('.config-modifier-title input');
  await title.fill('Defense & sustain');
  await title.press('Enter');
  await expect(title).toHaveValue('Defense & sustain');
  const text = '+250 to maximum Life\n\n+10 to Spirit';
  await groups.nth(1).locator('.config-extra-mods').fill(text);
  await groups.nth(1).locator('.config-modifier-title input').click();
  await expect.poll(async () => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).state.params.custom_modifier_blocks)).toEqual([
    { title: 'Active', enabled: false, text: '+100 to maximum Life' },
    { title: 'Defense & sustain', enabled: true, text },
  ]);

  await page.reload();
  await expect(groups).toHaveCount(2, { timeout: 90_000 });
  await expect(groups.nth(1).locator('.config-extra-mods')).toHaveValue(text);
  await expect(groups.nth(0).locator('.config-modifier-enabled input')).not.toBeChecked();
  await page.getByRole('button', { name: 'Build', exact: true }).click();
  await page.getByRole('button', { name: 'Generate share code', exact: true }).click();
  const code = await page.getByRole('textbox', { name: 'Share Code', exact: true }).inputValue();
  const exported = inflateSync(Buffer.from(code, 'base64url')).toString('utf8');
  expect(exported).toContain('title="Active" enabled="false"');
  expect(exported).toContain('title="Defense &amp; sustain" enabled="true"');
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);
  await page.getByRole('button', { name: 'Config', exact: true }).click();
  await expect(groups).toHaveCount(2);
  await expect(groups.nth(1).locator('.config-extra-mods')).toHaveValue(text);

  await page.locator('.config-modifier-add').click();
  await expect(groups).toHaveCount(3);
  await expect(groups.nth(2).locator('.config-modifier-title input')).toHaveValue('');
  await expect(groups.nth(2).locator('.config-modifier-enabled input')).toBeChecked();
  await groups.nth(2).getByRole('button', { name: /Remove/ }).click();

  await groups.nth(1).getByRole('button', { name: /Remove/ }).click();
  await expect(groups).toHaveCount(1);
  await groups.nth(0).getByRole('button', { name: /Remove/ }).click();
  await expect(groups).toHaveCount(0);
  await expect.poll(async () => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-build-state')!).state.params.custom_modifier_blocks)).toEqual([]);
  await page.reload();
  await expect(groups).toHaveCount(0, { timeout: 90_000 });
});

// A level 20 Comet and nothing else, so a custom modifier is the only thing
// that can change its level.
const caster = `<PathOfBuilding2>
  <Build level="90" className="Sorceress" mainSocketGroup="1"/>
  <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
  <Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true" mainActiveSkill="1">
    <Gem skillId="CometPlayer" gemId="Metadata/Items/Gems/SkillGemComet" level="20" quality="0" enabled="true"/>
  </Skill></SkillSet></Skills>
  <Items/>
  <Config/>
</PathOfBuilding2>`;

test('a custom modifier raises gem levels, and a line the calculation cannot apply is listed under the groups', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await page.getByRole('button', { name: 'Build', exact: true }).click();
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(deflateSync(caster).toString('base64url'));
  await page.locator('.import-submit').click();
  await expect(page.locator('.topbar-busy')).toHaveCount(0);

  const dps = page.locator('.main-skill-section .stat-row dd').first();
  const number = async () => Number((await dps.innerText()).replaceAll(',', ''));
  await expect.poll(number).toBeGreaterThan(0);
  const plain = await number();

  await page.getByRole('button', { name: 'Config', exact: true }).click();
  const groups = page.locator('.config-modifier-block');
  const unapplied = page.locator('.config-unapplied');
  await expect(unapplied).toHaveCount(0);
  if (await groups.count() === 0) await page.locator('.config-modifier-add').click();
  const text = groups.first().locator('.config-extra-mods');
  const leave = () => groups.first().locator('.config-modifier-title input').click();

  await text.fill('+1 to Level of all Cold Skills\nZorblax the unknowable 12%');
  await leave();
  await expect.poll(number).toBeGreaterThan(plain);
  const raised = await number();
  await expect(unapplied.locator('li')).toHaveText(['Zorblax the unknowable 12%']);

  // The same bonus for skills Comet is not leaves it alone; the notice goes with the line.
  await text.fill('+1 to Level of all Fire Skills');
  await leave();
  await expect.poll(number).toBe(plain);
  await expect(unapplied).toHaveCount(0);

  await text.fill('+1 to Level of all Spell Skills');
  await leave();
  await expect.poll(number).toBe(raised);
});

