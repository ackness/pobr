import { expect, test, type Page } from '@playwright/test';

test.use({ viewport: { width: 1440, height: 1000 } });

async function language(page: Page, label: string) {
  await page.locator('.v2-settings-button').click();
  await page.locator('#v2-language').click();
  await page.getByRole('option', { name: label, exact: true }).click();
  await page.locator('.v2-settings').getByRole('button', { name: /^(Close|关闭|關閉)$/ }).click();
}

test('known passive names and Chinese search preserve raw IDs, build state and evaluated scores', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('pobr-v2-preferences', JSON.stringify({ lang: 'zh-CN', theme: 'dark', font: 'system', fontSize: 16, density: 'comfortable' }));
    localStorage.setItem('pobr-build-state', JSON.stringify({ version: 1, notes: '', state: {
      pobCode: null, character: { level: 80, class_name: 'Witch', ascendancy_name: '' },
      allocatedNodes: [], attributeChoices: {},
      socketGroups: [{ enabled: true, gems: [{ skill_id: 'FireballPlayer', level: 16, quality: 0 }] }],
      items: [], flasks: [], limbs: [], jewels: [], annotations: {}, params: { config_inputs: {} },
    } }));
  });
  await page.goto('/v2/tree');
  await expect(page.locator('.tree-canvas svg')).toBeVisible({ timeout: 90_000 });
  const saved = () => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state);
  const before = await saved();
  const report = page.locator('.node-power');
  await report.locator(':scope > summary').click();
  const search = report.getByRole('searchbox');
  for (const [id, english, chinese] of [
    [6898, 'Relentless Vindicator', '无休仲裁者'],
    [28950, 'Devoted Protector', '献身卫士'],
  ] as const) {
    const row = report.locator(`tr[data-node-id="${id}"]`);
    for (const query of [chinese, english, String(id)]) {
      await search.fill(query);
      await expect(row.getByRole('button', { name: chinese, exact: true })).toBeVisible();
      await expect(row.locator('td').first()).toContainText(`#${id}`);
    }
  }
  await search.fill('6898');
  const row = report.locator('tr[data-node-id="6898"]');
  await report.getByRole('button', { name: '评估接下来 128 个节点', exact: true }).click();
  await expect(row).toHaveAttribute('data-status', 'ready', { timeout: 60_000 });
  const values = await row.locator('td').allTextContents();
  await language(page, '繁體中文');
  await expect(row.getByRole('button', { name: '無休仲裁者', exact: true })).toBeVisible();
  await expect(row).toHaveAttribute('data-status', 'ready');
  expect((await row.locator('td').allTextContents()).slice(1, 4)).toEqual(values.slice(1, 4));
  await search.fill('無休仲裁者');
  await row.getByRole('button', { name: '無休仲裁者', exact: true }).click();
  await expect(page.locator('.tree-node-details')).toHaveAttribute('data-skill-id', '6898');
  await language(page, 'English');
  await search.fill('Relentless Vindicator');
  await expect(row.getByRole('button', { name: 'Relentless Vindicator', exact: true })).toBeVisible();
  await expect(row).toHaveAttribute('data-status', 'ready');
  await language(page, '简体中文');
  await page.locator('.tree-search input').fill('献身卫士');
  await expect(page.locator('circle.node-search-hit[data-skill-id="28950"]')).toHaveCount(1);
  await page.locator('.tree-search button').click();
  await expect(page.locator('.tree-node-details')).toHaveAttribute('data-skill-id', '28950');
  await expect(page.locator('.tree-node-details')).toContainText('献身卫士');
  expect(await saved()).toEqual(before);
});
