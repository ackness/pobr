/**
 * 白手起 build 流程（PoB2 新建语义）：不导入任何 code——启动即有默认角色、
 * 结果栏出数；切职业、改等级、树上点选加点均实时重算。
 */

import { expect, test } from '@playwright/test';
import { pageLink, statValue } from './shell';
import { choose, closeSettings, settled, stableText } from './buildLibrary';

test('scratch build: class picker, level edit, tree allocation', async ({ page }) => {
  await page.goto('/build');
  await expect(page.getByRole('heading', { name: 'Character' })).toBeVisible({ timeout: 90_000 });

  // 默认空 build 已计算：结果栏 Life 有值且非 0/—。
  const lifeValue = statValue(page, /^Life$/).first();
  await expect(lifeValue).not.toHaveText('—');
  await expect(lifeValue).not.toHaveText('0');

  // 切职业 → 职业选择器跟随，Life 随之重算。
  const classPicker = page.getByRole('combobox', { name: 'Class', exact: true });
  await choose(page, classPicker, 'Witch');
  await expect(classPicker).toContainText('Witch', { timeout: 30_000 });

  // 改等级 → Life 变化（等级驱动基础生命）。
  await settled(page);
  const lifeBefore = await stableText(lifeValue);
  await page.getByLabel('Level').fill('90');
  await expect(lifeValue).not.toHaveText(lifeBefore!, { timeout: 30_000 });

  // 树上点一个节点 → 沿最短路径整条点亮（PoB2 语义，见 lib/passiveGraph）。
  // 具体点数取决于该节点离职业起点多远，故只断言「从 0 变成正数」——钉死数字会
  // 让树数据一升级就挂。
  await pageLink(page, 'Tree').click();
  await expect(page.locator('.tree-canvas svg')).toBeVisible({ timeout: 30_000 });
  await expect(page.locator('.tree-count')).toContainText('0 allocated', { timeout: 30_000 });
  // Search and center a reachable notable so the click uses a visible target.
  await page.getByRole('searchbox', { name: 'Search nodes…' }).fill('Practiced Signs');
  const target = page.locator('.node-search-hit');
  await expect(target).toHaveCount(1);
  await page.locator('.tree-search').getByRole('button', { name: 'Next', exact: true }).click();
  await target.click();
  await expect(page.locator('.tree-count')).toHaveText(/[1-9]\d* allocated/, { timeout: 30_000 });
  await expect(target).toHaveClass(/node-allocated/);

  // 手动添加技能：自定义选择器搜 Comet → 回车选中首项 → 新组出现 → Total DPS 出数。
  await pageLink(page, 'Skills').click();
  const picker = page.getByRole('combobox', { name: /Search an active gem/ });
  await picker.fill('Comet');
  await expect(page.locator('.gem-picker-item').first()).toBeVisible();
  await picker.press('Enter');
  await expect(page.locator('.skill-group')).toHaveCount(1, { timeout: 30_000 });
  const dpsValue = statValue(page, /^Total DPS$/).first();
  await expect(dpsValue).not.toHaveText('0', { timeout: 30_000 });
  await expect(dpsValue).not.toHaveText('—');

  // 手动添加装备：人形布局点 Ring 1 槽位 → 编辑面板默认模板（+50 Life）→ Life 变化。
  await settled(page);
  const lifeBeforeItem = await stableText(lifeValue);
  await pageLink(page, 'Items').click();
  await page.locator('.paper-doll').getByRole('button', { name: 'Ring 1' }).click();
  await page.locator('.item-detail').getByRole('button', { name: 'Apply changes' }).click();
  await expect(lifeValue).not.toHaveText(lifeBeforeItem!, { timeout: 30_000 });

  // 三语切换：EN → 繁 → 简（页面名称跟随，简繁字形区分）。语言在设置面板里选。
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
