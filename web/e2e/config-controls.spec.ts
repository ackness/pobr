import { expect, test } from '@playwright/test';

test('every catalog control and list choice can be changed, recalculated and reset', async ({ page }) => {
  test.setTimeout(600_000);
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('navigation', { name: 'Main navigation' }).getByRole('button', { name: 'Config', exact: true }).click();
  await expect(page.locator('.config-section-header').first()).toBeVisible();
  const headers = page.locator('.config-section-header');
  const summary: { section: string; controls: number; choices: number }[] = [];
  for (let sectionIndex = 0; sectionIndex < await headers.count(); sectionIndex++) {
    const section = page.locator('.config-section').nth(sectionIndex);
    const header = section.locator('.config-section-header');
    if (await header.getAttribute('aria-expanded') !== 'true') await header.click();
    const controls = await section.locator('.config-item').evaluateAll(rows => rows.map(row => {
      const control = row.querySelector('input, select') as HTMLInputElement | HTMLSelectElement;
      return { key: row.getAttribute('title')!, id: control.id, type: control.type,
        value: control.value, checked: (control as HTMLInputElement).checked,
        choices: control instanceof HTMLSelectElement ? [...control.options].map(option => option.value) : [] };
    }));
    let choices = 0;
    for (const control of controls) {
      const row = section.locator('.config-item').filter({ has: page.locator(`[id=${JSON.stringify(control.id)}]`) });
      const input = row.locator('input, select');
      const values: (string | boolean | number)[] = control.type === 'checkbox' ? [!control.checked]
        : control.type === 'select-one' ? [...new Set(control.choices)]
        : control.type === 'number' ? [Number(control.value || 0) + 1] : ['+10 to maximum Life'];
      for (const value of values) {
        await expect(input, control.key).toBeEnabled();
        if (control.type === 'checkbox') await input.setChecked(Boolean(value));
        else if (control.type === 'select-one') await input.selectOption(String(value));
        else { await input.fill(String(value)); await input.press('Enter'); }
        await expect(input, control.key).toBeEnabled();
        await expect(page.locator('.calc-error'), control.key).toHaveCount(0);
        const actual = await page.evaluate(key => JSON.parse(localStorage.getItem('pobr-build-state')!).state.params.config_inputs[key], control.key);
        // Selecting the already displayed default need not materialize an override.
        if (control.type !== 'select-one' || String(value) !== control.value || actual !== undefined) expect(actual, control.key).toEqual(value);
        choices++;
      }
      const reset = row.locator('.config-reset');
      if (await reset.count()) await reset.click();
      await expect(input, control.key).toBeEnabled();
      const restored = await page.evaluate(key => JSON.parse(localStorage.getItem('pobr-build-state')!).state.params.config_inputs[key], control.key);
      expect(restored, `${control.key} reset`).toBeUndefined();
    }
    summary.push({ section: (await header.innerText()).replaceAll('\n', ' '), controls: controls.length, choices });
    await header.click();
  }
  console.log('Config interaction coverage:', JSON.stringify(summary));
});
