import { expect, test } from '@playwright/test';
import { pageLink } from './shell';

test('WeGame import preserves gem levels and ranks quiver upgrades', async ({ page }) => {
  await page.route('**/api/import/wegame', async route => {
    expect(route.request().postDataJSON().url).toContain('/#/share/');
    await route.fulfill({ json: {
      format: 'wegame', version: 1, role: { level: 59, class_name: 'Deadeye' },
      equipments: [
        { inventoryId: 'Weapon', baseType: 'Crude Bow', frameType: 0,
          explicitMods: [{ description: 'Adds 10 to 20 Physical Damage' }] },
        { inventoryId: 'Offhand', baseType: 'Primed Quiver', name: 'Current Quiver', frameType: 2,
          implicitMods: ['8% increased Attack Speed'] },
        ...['Ultimate Mana Flask', 'Ultimate Life Flask', 'Thawing Charm', 'Stone Charm', 'Silver Charm'].map((baseType, x) => ({
          inventoryId: 'Flask', x, baseType, name: 'Synthetic Utility Item', frameType: 1,
          explicitMods: x === 3 ? ['Also grants 102 Guard'] : [],
        })),
      ],
      talent_tree: { hashes: [], quest_stats: [] }, jewel_data: '[]',
      skills: [{ baseType: 'Ice Shot', support: false,
        properties: [{ type: 5, values: [['12', 0]] }, { type: 6, values: [['+20%', 1]] }] }],
    } });
  });
  await page.goto('/build');
  await expect(page.getByRole('heading', { name: 'Import a build' })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill(
    'https://www.wegame.com.cn/helper/poe2/#/share/SyntheticShareKey_123456',
  );
  await page.locator('.import-submit').click();
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  // Guard from a charm is modelled, so nothing in this import is left unsupported: the Overview reports
  // no line as not calculated and offers no list of them.
  await expect(page.locator('.ov-facts > div').filter({ has: page.locator('dt', { hasText: 'Lines not calculated' }) }).locator('dd'))
    .toHaveText('0', { timeout: 60_000 });
  await expect(page.locator('.ov-details')).toHaveCount(0);
  await pageLink(page, 'Items').click();
  await expect(page.getByRole('heading', { name: 'Items', exact: true })).toBeVisible({ timeout: 60_000 });
  await pageLink(page, 'Skills').click();
  await expect(page.locator('.skill-group').first()).toContainText('Ice Shot');
  // The persisted editing state is what the optimizer and the next reload consume.
  const session = await page.evaluate(() => localStorage.getItem('pobr-v2-build-state'));
  expect(session).not.toBeNull();
  const state = JSON.parse(session!).state;
  expect(state.character.level).toBe(59);
  expect(state.socketGroups[0].gems[0]).toMatchObject({ skill_id: 'IceShotPlayer', level: 12, quality: 20 });

  await pageLink(page, 'Items').click();
  await page.getByRole('button', { name: 'Off Hand', exact: true }).click();
  await page.getByRole('button', { name: 'Edit', exact: true }).click();
  const editor = page.getByRole('textbox', { name: 'weapon2 item text' });
  const original = await editor.inputValue();
  await editor.fill(original.replace('Current Quiver', 'Candidate Quiver') + '\nAdds 20 to 40 Physical Damage to Attacks');
  await page.locator('.item-editor-actions').getByRole('button', { name: 'Save & recalculate', exact: true }).click();
  await page.getByRole('button', { name: 'Save to library', exact: true }).click();
  await page.getByRole('button', { name: 'Edit', exact: true }).click();
  await editor.fill(original);
  await page.locator('.item-editor-actions').getByRole('button', { name: 'Save & recalculate', exact: true }).click();
  await page.getByRole('button', { name: 'Try on items from my library' }).click();
  await page.locator('.opt-run').click();
  const results = page.locator('.opt-table');
  await expect(results).toContainText('Candidate Quiver');
  await expect(results).not.toContainText('Crude Bow');
  const best = results.locator('tbody tr').nth(1);
  await expect(best).toContainText('Candidate Quiver');
  await expect(best.locator('.opt-delta')).toHaveText(/^\+[1-9][\d.]*%$/);
  await best.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Off Hand', exact: true })).toContainText('Candidate Quiver');
});
