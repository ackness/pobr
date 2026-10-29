import { expect, test } from '@playwright/test';
import { deflateSync, inflateSync } from 'node:zlib';
import { pageLink } from './shell';
import { headlineDps, importBuild, settledNumber, shareCode } from './skillsAndCalcs';

// The ring's only modifier names corrupted gems, so the gem's corrupted state
// alone decides whether the skill gains a level.
const source = `<PathOfBuilding2>
  <Build level="90" className="Sorceress" mainSocketGroup="1"/>
  <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
  <Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true" mainActiveSkill="1">
    <Gem skillId="CometPlayer" gemId="Metadata/Items/Gems/SkillGemComet" level="20" quality="0" enabled="true"/>
  </Skill></SkillSet></Skills>
  <Items activeItemSet="1">
    <Item id="1">Rarity: RARE
Corruption Test
Sapphire Ring
Implicits: 0
+1 to Level of all Corrupted Skill Gems</Item>
    <ItemSet id="1"><Slot name="Ring 1" itemId="1"/></ItemSet>
  </Items>
  <Config/>
</PathOfBuilding2>`;

test('a gem can be marked corrupted on Skills, which decides modifiers to corrupted gems and survives saving and sharing', async ({ page }) => {
  await importBuild(page, deflateSync(source).toString('base64url'));

  const dps = headlineDps(page);
  const number = async () => Number((await dps.innerText()).replaceAll(',', ''));
  await expect.poll(number).toBeGreaterThan(0);
  const plain = await settledNumber(number);

  await pageLink(page, 'Skills').click();
  const group = page.locator('.skill-group').first();
  await group.locator('.skill-group-title').click();
  const corrupted = group.locator('.skill-gem').first().getByRole('checkbox', { name: 'Corrupted', exact: true });
  await expect(corrupted).not.toBeChecked();

  await corrupted.check();
  await expect.poll(number).toBeGreaterThan(plain);
  const raised = await settledNumber(number);
  await expect.poll(async () => page.evaluate(() =>
    JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.socketGroups[0].gems[0].corrupted)).toBe(true);

  await page.reload();
  await expect(dps).toBeVisible({ timeout: 90_000 });
  await expect.poll(number).toBe(raised);
  const code = await shareCode(page);
  expect(inflateSync(Buffer.from(code, 'base64url')).toString('utf8')).toMatch(/<Gem\b[^>]*corrupted="true"/);

  await pageLink(page, 'Skills').click();
  await group.locator('.skill-group-title').click();
  await expect(corrupted).toBeChecked();
  await corrupted.uncheck();
  await expect.poll(number).toBe(plain);
  await expect.poll(async () => page.evaluate(() =>
    'corrupted' in JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.socketGroups[0].gems[0])).toBe(false);
});
