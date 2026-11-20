/**
 * From-scratch build flow (PoB2 "New Build" semantics): starts without importing any code—
 * opens with a default character and computed sidebar stats; changing class, level, or passive nodes recalculates live.
 */

import { expect, test } from '@playwright/test';
import { pageLink, statValue, playerHitDps } from './shell';
import { choose, closeSettings, settled, stableText } from './buildLibrary';
import { answerConfirm } from './confirmDialog';
import { SMOKE } from './tags';

test('scratch build: class picker, level edit, tree allocation', SMOKE, async ({ page }) => {
  await page.goto('/build');
  await expect(page.getByRole('heading', { name: 'Character' })).toBeVisible({ timeout: 90_000 });

  // Switch class -> class selector follows.
  const classPicker = page.getByRole('combobox', { name: 'Class', exact: true });
  await choose(page, classPicker, 'Witch');
  // Changing class resets what the default build carries (its level and configuration), so it asks first.
  await answerConfirm(page, true);
  await expect(classPicker).toContainText('Witch', { timeout: 30_000 });

  // The results panel stays empty until the build has content; a level above 1 is content.
  const empty = page.locator('#character-stats').getByText('Import or create a build to see its results.', { exact: true });
  await expect(empty).toBeVisible();
  await page.getByLabel('Level').fill('80');
  await expect(empty).toHaveCount(0, { timeout: 30_000 });
  const lifeValue = statValue(page, /^Life$/).first();
  // A missing row would pass the checks below, so require it first.
  await expect(lifeValue).toBeVisible();
  await expect(lifeValue).not.toHaveText('—', { timeout: 30_000 });
  await expect(lifeValue).not.toHaveText('0');

  // Edit level -> Life changes (base life scales with level).
  await settled(page);
  const lifeBefore = await stableText(lifeValue);
  await page.getByLabel('Level').fill('90');
  await expect(lifeValue).not.toHaveText(lifeBefore!, { timeout: 30_000 });

  // Click a passive node -> allocates the full shortest path (PoB2 semantics; see lib/passiveGraph).
  // The exact point count depends on distance from the class start, so only assert "0 -> positive"—
  // pinning a hard number would break on tree data bumps.
  await pageLink(page, 'Tree').click();
  await expect(page.locator('.tree-canvas svg')).toBeVisible({ timeout: 30_000 });
  // Level 90 gives 89 level points plus the 24 quest points.
  await expect(page.locator('.tree-count')).toContainText('Used 0 / 113 points', { timeout: 30_000 });
  // Search and center a reachable notable so the click uses a visible target.
  await page.getByRole('searchbox', { name: 'Search nodes…' }).fill('Practiced Signs');
  const target = page.locator('.node-search-hit');
  await expect(target).toHaveCount(1);
  await page.locator('.tree-search').getByRole('button', { name: 'Next', exact: true }).click();
  await target.click();
  await expect(page.locator('.tree-count')).toHaveText(/Used [1-9]\d* \/ \d+ points/, { timeout: 30_000 });
  await expect(target).toHaveClass(/node-allocated/);

  // Manually add a skill: search Comet in custom picker -> Enter picks first -> group appears -> Hit DPS computes.
  await pageLink(page, 'Skills').click();
  const picker = page.getByPlaceholder('Search or browse gems…');
  await picker.fill('Comet');
  await expect(page.locator('.gem-picker-item').first()).toBeVisible();
  await picker.press('Enter');
  await expect(page.locator('.skill-group')).toHaveCount(1, { timeout: 30_000 });
  const dpsValue = playerHitDps(page);
  await expect(dpsValue).not.toHaveText('0', { timeout: 30_000 });
  await expect(dpsValue).not.toHaveText('—');

  // Manually add gear: click Ring 1 on paper doll -> apply default panel template (+50 Life) -> Life changes.
  await settled(page);
  const lifeBeforeItem = await stableText(lifeValue);
  await pageLink(page, 'Items').click();
  await page.locator('.paper-doll').getByRole('button', { name: 'Ring 1' }).click();
  await page.locator('.item-detail').getByRole('button', { name: 'Apply changes' }).click();
  await expect(lifeValue).not.toHaveText(lifeBeforeItem!, { timeout: 30_000 });

  // Switch between all 3 UI languages: EN -> zh-TW -> zh-CN (nav labels follow script). Language selector lives in settings.
  const language = async (name: string) => {
    await page.locator('.shell-settings-button').click();
    await choose(page, page.locator('#shell-language'), name);
    await closeSettings(page);
  };
  await language('繁體中文');
  await expect(page.getByRole('link', { name: '天賦樹', exact: true })).toBeVisible();
  await language('简体中文');
  await expect(page.getByRole('link', { name: '天赋树', exact: true })).toBeVisible();
  await language('English');
  await expect(pageLink(page, 'Tree')).toBeVisible();
});
