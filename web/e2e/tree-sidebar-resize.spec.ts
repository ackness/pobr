import { expect, test, type Locator, type Page } from '@playwright/test';
import { busy } from './shell';
import { setLanguage } from './treePage';

const STORAGE_KEY = 'pobr-v2-tree-side-width';
const handle = (page: Page) => page.getByRole('separator', { name: /^(Resize sidebar|调整侧栏宽度)$/ });
const rem = (page: Page, px: number) => page.evaluate(size => size / parseFloat(getComputedStyle(document.documentElement).fontSize), px);
const sideRem = async (page: Page) => rem(page, (await page.locator('#tree-side').boundingBox())!.width);

async function open(page: Page) {
  await page.goto('/tree');
  await expect(page.locator('.tree-canvas svg')).toBeVisible({ timeout: 90_000 });
  await expect(busy(page)).toHaveCount(0);
}

/** Drag the handle's centre by `dx` pixels with real pointer events. */
async function drag(page: Page, grip: Locator, dx: number) {
  const box = (await grip.boundingBox())!;
  const x = box.x + box.width / 2;
  const y = box.y + box.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + dx / 2, y, { steps: 4 });
  await page.mouse.move(x + dx, y, { steps: 4 });
  await page.mouse.up();
}

test.describe('1440px', () => {
  test.use({ viewport: { width: 1440, height: 900 } });

  test('dragging the edge resizes the column within its limits, and keys and double-click follow', async ({ page }) => {
    await open(page);
    const grip = handle(page);
    await expect(grip).toHaveAttribute('aria-valuenow', '27');
    await expect(grip).toHaveAttribute('aria-controls', 'tree-side');
    expect(await sideRem(page)).toBeCloseTo(27, 0);

    await drag(page, grip, -80);
    const wider = await sideRem(page);
    expect(wider).toBeGreaterThan(31);
    expect(wider).toBeLessThan(33);
    await expect(grip).toHaveAttribute('aria-valuenow', String(Math.round(wider * 10) / 10));
    await expect(page.locator('.tree-panel')).not.toHaveAttribute('data-resizing');

    // At 1440px the 48% share of the panel is the wider limit, and the handle reports it.
    await drag(page, grip, -2000);
    const widest = Number(await grip.getAttribute('aria-valuemax'));
    expect(widest).toBeLessThan(44);
    expect(widest).toBeGreaterThan(wider);
    expect(await sideRem(page)).toBeCloseTo(widest, 1);
    await expect(grip).toHaveAttribute('aria-valuenow', String(widest));

    await drag(page, grip, 3000);
    expect(await sideRem(page)).toBeCloseTo(22, 1);
    await expect(grip).toHaveAttribute('aria-valuenow', '22');

    await grip.focus();
    await page.keyboard.press('ArrowLeft');
    await expect(grip).toHaveAttribute('aria-valuenow', '23');
    await page.keyboard.press('Shift+ArrowLeft');
    await expect(grip).toHaveAttribute('aria-valuenow', '27');
    await page.keyboard.press('ArrowRight');
    await expect(grip).toHaveAttribute('aria-valuenow', '26');
    await page.keyboard.press('End');
    await expect(grip).toHaveAttribute('aria-valuenow', String(widest));
    await page.keyboard.press('Home');
    await expect(grip).toHaveAttribute('aria-valuenow', '22');
    expect(await sideRem(page)).toBeCloseTo(22, 1);

    await grip.dblclick();
    await expect(grip).toHaveAttribute('aria-valuenow', '27');
    expect(await sideRem(page)).toBeCloseTo(27, 1);
  });

  test('a drag that returns to its start, or a plain click, leaves the width and storage alone', async ({ page }) => {
    await open(page);
    const grip = handle(page);
    const box = (await grip.boundingBox())!;
    const x = box.x + box.width / 2;
    const y = box.y + box.height / 2;
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x - 200, y, { steps: 4 });
    // Let a frame paint the wide width, then come back before the next one.
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => resolve(null))));
    await page.mouse.move(x, y, { steps: 4 });
    await page.mouse.up();
    await expect(page.locator('.tree-panel')).not.toHaveAttribute('data-resizing');
    await expect(grip).toHaveAttribute('aria-valuenow', '27');
    expect(await sideRem(page)).toBeCloseTo(27, 1);
    await page.mouse.click(x, y);
    expect(await sideRem(page)).toBeCloseTo(27, 1);
    expect(await page.evaluate(key => localStorage.getItem(key), STORAGE_KEY)).toBeNull();
  });

  test('the width is remembered, ignores bad stored values, and the handle goes with the closed column', async ({ page }) => {
    await open(page);
    expect(await page.evaluate(key => localStorage.getItem(key), STORAGE_KEY)).toBeNull();
    await handle(page).focus();
    await page.keyboard.press('Shift+ArrowLeft');
    expect(await page.evaluate(key => localStorage.getItem(key), STORAGE_KEY)).toBe('31');

    await page.reload();
    await expect(page.locator('.tree-canvas svg')).toBeVisible({ timeout: 90_000 });
    await expect(handle(page)).toHaveAttribute('aria-valuenow', '31');
    expect(await sideRem(page)).toBeCloseTo(31, 1);

    await page.getByRole('button', { name: 'Attributes & planning' }).click();
    await expect(page.locator('#tree-side')).toBeHidden();
    await expect(handle(page)).toHaveCount(0);

    await page.evaluate(key => localStorage.setItem(key, 'wide'), STORAGE_KEY);
    await page.reload();
    await expect(page.locator('.tree-canvas svg')).toBeVisible({ timeout: 90_000 });
    await expect(handle(page)).toHaveAttribute('aria-valuenow', '27');
  });
});

test.describe('390px', () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test('the stacked layout has no resize handle (zh-CN)', async ({ page }) => {
    await open(page);
    await setLanguage(page, 'zh-CN');
    await expect(page.locator('.tree-side-resize')).toHaveCount(1);
    await expect(handle(page)).toBeHidden();
    await expect(page.locator('#tree-side')).toBeVisible();
  });
});
