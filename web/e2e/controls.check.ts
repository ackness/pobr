import { expect, test } from '@playwright/test';

for (const v2 of [false, true]) {
  test(`V${v2 ? 2 : 1}: scoped dialogs, native form events and refs`, async ({ page }) => {
    page.on('dialog', () => { throw new Error('Browser dialogs must not be used'); });
    await page.goto(`/e2e/fixtures/controls.html?v2=${v2}`);
    const result = page.getByLabel('Result');
    await page.getByRole('button', { name: 'Remove', exact: true }).click();
    const confirm = page.getByRole('alertdialog');
    await expect(confirm).toBeVisible();
    await expect(confirm).toHaveCSS('position', 'fixed');
    await expect(confirm.getByRole('button', { name: 'Cancel', exact: true })).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(result).toHaveText('cancelled');
    await expect(page.getByRole('button', { name: 'Remove', exact: true })).toBeFocused();
    await page.getByRole('button', { name: 'Remove', exact: true }).click();
    await page.evaluate(() => document.dispatchEvent(new Event('fixture-stage')));
    await expect(confirm).toHaveCount(0);
    await expect(result).toHaveText('cancelled');
    await page.getByRole('button', { name: 'Remove', exact: true }).click();
    await page.evaluate(() => document.dispatchEvent(new Event('fixture-edit')));
    await confirm.getByRole('button', { name: 'Confirm', exact: true }).click();
    await expect(result).toHaveText('cancelled');
    await page.getByRole('button', { name: 'Remove', exact: true }).click();
    await confirm.getByRole('button', { name: 'Confirm', exact: true }).click();
    await expect(result).toHaveText('removed');
    await page.getByRole('button', { name: 'Rename', exact: true }).click();
    const prompt = page.getByRole('dialog');
    await expect(prompt.getByRole('textbox')).toBeFocused();
    await expect(prompt.getByRole('textbox')).toHaveValue('Original');
    await prompt.getByRole('textbox').fill('New name');
    await page.keyboard.press('Enter');
    await expect(result).toHaveText('New name');
    await page.getByLabel('Enabled', { exact: true }).check();
    await expect(result).toHaveText('true:enabled:true');
    if (v2) {
      await page.getByRole('combobox', { name: 'Slot', exact: true }).click();
      await page.getByRole('option', { name: 'Ring', exact: true }).click();
    } else await page.getByRole('combobox', { name: 'Slot', exact: true }).selectOption('ring');
    await expect(result).toHaveText('true:ring');
    await page.getByRole('button', { name: 'Submit slot' }).click();
    await expect(result).toHaveText('ring');
    const choice = v2 ? page.getByRole('combobox', { name: 'Choice' }) : page.getByRole('button', { name: 'Choice' });
    await choice.focus();
    await choice.press('Enter');
    await page.getByRole('option', { name: /Beta/ }).click();
    await expect(choice).toHaveText(v2 ? 'Beta' : 'Beta▾');
    await expect(choice).toBeFocused();
    await page.getByRole('button', { name: 'Focus notes' }).click();
    await expect(page.getByRole('textbox', { name: 'Notes' })).toBeFocused();
    await page.getByLabel('File', { exact: true }).setInputFiles({ name: 'build.json', mimeType: 'application/json', buffer: Buffer.from('{}') });
    await expect(result).toHaveText('build.json');
    await expect(page.locator('[title]')).toHaveCount(0);
  });
}

for (const lang of ['en-US', 'zh-CN', 'zh-TW']) {
  test(`V2 gem picker preserves multilingual search, IME and keyboard (${lang})`, async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto(`/e2e/fixtures/controls.html?lang=${lang}`);
    const input = page.getByRole('combobox', { name: 'Find a gem' });
    await input.fill('Fireball');
    await expect(page.locator('.gem-picker-popover')).toBeVisible();
    expect((await page.locator('.gem-picker-popover').boundingBox())!.width).toBeLessThanOrEqual(390);
    await expect(page.getByRole('option')).toHaveCount(1);
    await input.dispatchEvent('keydown', { key: 'Enter', code: 'Enter', isComposing: true });
    await expect(page.getByLabel('Gem', { exact: true })).toHaveText('none');
    await input.press('Enter');
    await expect(page.getByLabel('Gem', { exact: true })).toHaveText('fire');
    await expect(input).toBeFocused();
    await expect(input).toHaveValue('');
    await input.fill('电球');
    await expect(page.getByRole('option')).toHaveCount(1);
    await input.fill('電球');
    await expect(page.getByRole('option')).toHaveCount(1);
    await input.press('Enter');
    await expect(page.getByLabel('Gem', { exact: true })).toHaveText('lightning');
    await input.fill('');
    await input.press('ArrowDown');
    await input.press('Home');
    await input.press('Enter');
    await expect(page.getByLabel('Gem', { exact: true })).toHaveText('fire');
    await input.fill('Ice');
    await page.getByRole('option').click();
    await expect(input).toBeFocused();
    await expect(page.getByLabel('Gem', { exact: true })).toHaveText('ice');
  });
}
