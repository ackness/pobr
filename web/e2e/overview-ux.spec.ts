import { expect, test } from '@playwright/test';
import { deflateSync } from 'node:zlib';
import { importBuild } from './skillsAndCalcs';

const code = deflateSync(`<PathOfBuilding2><Build level="80" className="Witch" mainSocketGroup="1"/>
<Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
<Skills activeSkillSet="1"><SkillSet id="1">
<Skill enabled="true"><Gem skillId="FireballPlayer" level="15" quality="0" enabled="true"/></Skill>
<Skill enabled="true"><Gem skillId="SparkPlayer" level="15" quality="0" enabled="true"/></Skill>
</SkillSet></Skills><Items/><Config/></PathOfBuilding2>`).toString('base64url');

for (const width of [390, 1440, 2560]) {
  test(`Overview keeps its layout and calculated-skill selection clear at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await importBuild(page, code);
    const skills = page.locator('[aria-labelledby="ov-skills"]');
    await expect(skills).toContainText('Select a skill to use it for calculations across all pages.');
    const current = skills.locator('button[aria-pressed="true"]');
    await expect(current).toContainText('Current');
    const other = skills.locator('button[aria-pressed="false"]').first();
    await expect(other).toBeEnabled();
    await page.locator('[aria-labelledby="ov-contribution"]').getByRole('button', { name: 'Source contributions', exact: true }).click();
    await expect(page.locator('.ov-contrib')).toBeVisible();
    await other.click();
    await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
    await expect(page.locator('.rail-chip')).toHaveCount(0);
    await expect(page.locator('[aria-labelledby="ov-contribution"]')).toContainText('Calculated skill changed; run the analysis again');
    await expect(page.locator('.ov-resistances > div')).toHaveCount(4);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    if (width === 2560) expect(await page.locator('.ov').evaluate(element => element.getBoundingClientRect().width)).toBeGreaterThan(1600);
  });
}

test('a first visit focuses import/create and keeps blank character stats out of the rail', async ({ page }) => {
  await page.goto('/');
  const empty = page.locator('.ov-empty');
  await expect(empty).toBeVisible({ timeout: 90_000 });
  await expect(empty.getByRole('button', { name: 'Import or create a build', exact: true })).toBeFocused();
  await expect(page.locator('#character-stats .rail-row')).toHaveCount(0);
});
