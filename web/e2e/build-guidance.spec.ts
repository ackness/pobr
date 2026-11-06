import { expect, test } from '@playwright/test';
import { nav, pageLink } from './shell';

test('historical references stay hidden: no page for them and no reference data is ever requested', async ({ page }) => {
  const referenceRequests: string[] = [];
  page.on('request', request => {
    if (request.url().includes('/build-references/')) referenceRequests.push(request.url());
  });
  await page.goto('/build');
  await expect(page.getByRole('textbox', { name: 'Build code' })).toBeVisible({ timeout: 90_000 });
  await expect(pageLink(page, 'Build references')).toHaveCount(0);
  await expect(nav(page).getByRole('link', { name: /reference/i })).toHaveCount(0);
  await expect(page.locator('.guidance-page')).toHaveCount(0);
  // A route of that name is not a page either: the app shows the overview, never the panel.
  await page.goto('/guidance');
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  await expect(pageLink(page, 'Overview')).toHaveAttribute('aria-current', 'page');
  await expect(page.locator('.guidance-page')).toHaveCount(0);
  await pageLink(page, 'Items').click();
  await expect(page.locator('.paper-doll')).toBeVisible();
  expect(referenceRequests).toEqual([]);
});
