import { expect, test, type Locator } from '@playwright/test';

for (const v2 of [false, true]) {
  test(`V${v2 ? 2 : 1}: item roll ranges preserve keyboard edits, endpoints and disabled state`, async ({ page }, testInfo) => {
    await page.goto(`/e2e/fixtures/ranges.html?v2=${v2}`);
    const catalogue = page.getByRole('region', { name: 'Unique catalogue', exact: true });
    await catalogue.getByRole('searchbox', { name: 'Search unique names and modifiers' }).fill('Astramentis');
    await catalogue.getByRole(v2 ? 'combobox' : 'button', { name: 'Choose a unique', exact: true }).click();
    await page.getByRole('option', { name: /^Astramentis/ }).click();
    await catalogue.getByRole(v2 ? 'combobox' : 'button', { name: 'Variant', exact: true }).click();
    await page.getByRole('option', { name: 'Pre 0.2.0', exact: true }).click();
    const first = catalogue.getByRole('slider', { name: 'Roll 1', exact: true });
    const second = catalogue.getByRole('slider', { name: 'Roll 2', exact: true });
    const value = async (slider: Locator, expected: number) => {
      if (v2) await expect(slider).toHaveAttribute('aria-valuenow', String(expected));
      else await expect(slider).toHaveValue(String(expected));
    };
    await value(first, 50);
    await first.focus();
    await first.press('ArrowRight');
    await value(first, 51);
    await expect(catalogue.locator('.unique-roll output').first()).toHaveText('51%');
    await value(second, 50);
    await first.press('Home');
    await value(first, 0);
    await first.press('ArrowLeft');
    await value(first, 0);
    await second.press('End');
    await second.press('ArrowRight');
    await value(second, 100);

    const editor = page.locator('.replacement-affix-editor');
    await editor.locator('summary').click();
    const life = editor.locator('[data-affix-id="equipment:IncreasedLife6"]');
    await life.getByRole('button', { name: 'Tier midpoint estimate', exact: true }).click();
    const roll = life.getByRole('slider', { name: 'Roll within tier: equipment:IncreasedLife6', exact: true });
    await roll.focus();
    await roll.press('ArrowRight');
    await value(roll, 51);
    await expect(life).toContainText('+77.14 to maximum Life');
    await roll.press('Home');
    await expect(life).toContainText('+70 to maximum Life');
    await roll.press('End');
    await expect(life).toContainText('+84 to maximum Life');
    await roll.press('ArrowLeft');
    await value(roll, 99);
    await editor.getByRole('button', { name: 'Recalculate edited item', exact: true }).click();
    await expect(page.getByLabel('Compared item')).toContainText('+83.86 to maximum Life');
    await expect(page.getByLabel('Compared item')).toContainText('+33% to Fire Resistance');
    await expect(page.getByLabel('Compared item')).toContainText('+20% to Cold Resistance');
    await expect(page.getByLabel('Dirty', { exact: true })).toHaveText('true');

    if (v2) {
      await expect(page.locator('input[type=range]')).toHaveCount(0);
      await expect(page.locator('[data-slot=slider-track]')).toHaveCount(3);
      const fill = catalogue.locator('[data-slot=slider-range]').first();
      await expect(fill).toHaveCSS('background-color', 'rgb(208, 180, 124)');
      await catalogue.screenshot({ path: testInfo.outputPath('ranges-dark.png') });
      await life.screenshot({ path: testInfo.outputPath('affix-range-dark.png') });
      await page.getByRole('button', { name: 'Toggle theme' }).click();
      await expect(fill).toHaveCSS('background-color', 'rgb(129, 97, 36)');
      await page.setViewportSize({ width: 390, height: 844 });
      const track = await catalogue.locator('[data-slot=slider-track]').first().boundingBox();
      expect(track!.width).toBeGreaterThan(80);
      expect(track!.height).toBe(4);
      await catalogue.screenshot({ path: testInfo.outputPath('ranges-light-mobile.png') });
      await life.screenshot({ path: testInfo.outputPath('affix-range-light-mobile.png') });
    } else {
      await expect(page.locator('input[type=range]')).toHaveCount(3);
      await expect(page.locator('[data-slot=slider]')).toHaveCount(0);
    }
    await value(first, 0);
    await value(second, 100);
    await value(roll, 99);
    await expect(catalogue.getByRole(v2 ? 'combobox' : 'button', { name: 'Variant', exact: true })).toContainText('Pre 0.2.0');
    await page.getByRole('button', { name: 'Toggle busy' }).click();
    for (const slider of [first, second, roll]) {
      if (v2) await expect(slider).toHaveAttribute('data-disabled', '');
      else await expect(slider).toBeDisabled();
      await slider.press('ArrowLeft');
      await slider.click({ force: true });
    }
    await page.getByRole('button', { name: 'Toggle busy' }).click();
    await value(first, 0);
    await value(second, 100);
    await value(roll, 99);

    await catalogue.getByRole('button', { name: 'Preview unique', exact: true }).click();
    await expect(page.getByLabel('Unique preview', { exact: true })).toContainText('+100 to all Attributes');
    const selection = JSON.parse(await page.getByLabel('Unique selection', { exact: true }).innerText());
    expect(selection.variant).toBe(1);
    expect(Object.values(selection.rolls)).toEqual([0, 1]);
    if (v2) await expect(first).toHaveAttribute('data-disabled', '');
    else await expect(first).toBeDisabled();
    await first.press('End');
    await value(first, 0);
  });
}
