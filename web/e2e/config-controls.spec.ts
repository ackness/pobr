import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { go } from './configPage';

// Controls the data audit found not to exist in this game version are not offered on the page.
const gameRelevance: { data_version: string; excluded: Record<string, unknown> } =
  JSON.parse(readFileSync('src/lib/configGameRelevance.json', 'utf8'));

test('every catalog control and list choice can be changed, recalculated and reset', async ({ page }) => {
  test.setTimeout(600_000);
  await page.goto('/build');
  await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
  await go(page, 'Config');
  await page.locator('.config-view').getByRole('button', { name: 'Show all options', exact: true }).click();
  await expect(page.locator('.config-section-header').first()).toBeVisible();
  const { version } = await (await page.request.get('/data/manifest.json')).json();
  const catalog: { options: { var: string }[] } = await (await page.request.get(`/data/${version}/overlay/config_options.json`)).json();
  // Repeated catalog definitions resolve to one control per saved key.
  const absent = new Set(gameRelevance.data_version === version ? Object.keys(gameRelevance.excluded) : []);
  const expectedKeys = [...new Set(catalog.options.map(option => option.var))]
    .filter(key => key !== 'customMods' && key !== 'enemyIsBoss' && !absent.has(key)).sort();
  const visitedKeys: string[] = [];
  const headers = page.locator('.config-section-header');
  const summary: { section: string; controls: number; choices: number }[] = [];
  for (let sectionIndex = 0; sectionIndex < await headers.count(); sectionIndex++) {
    const section = page.locator('.config-section').nth(sectionIndex);
    const header = section.locator('.config-section-header');
    if (await header.getAttribute('aria-expanded') !== 'true') await header.click();
    // A list option is a picker: a combobox button that carries the id, and a hidden native select that mirrors its choices.
    const controls = await section.locator('.config-item').evaluateAll(rows => rows.map(row => {
      const control = row.querySelector('input[id^="config-"], button[id^="config-"]') as HTMLInputElement | HTMLButtonElement;
      const select = control.getAttribute('role') === 'combobox' ? control.parentElement!.querySelector('select')! : null;
      const field = select ?? control as HTMLInputElement;
      return { key: row.getAttribute('data-config-var')!, id: control.id, type: select ? 'select-one' : (control as HTMLInputElement).type,
        value: field.value, checked: (control as HTMLInputElement).checked,
        choices: select ? [...select.options].map(option => option.value) : [] };
    }));
    let choices = 0;
    for (const control of controls) {
      expect(control.id).toBe(`config-${control.key}`);
      visitedKeys.push(control.key);
      // Setting a control does not move its row to another group, which would take it out of reach when that group is folded.
      const input = section.locator(`[id=${JSON.stringify(control.id)}]`);
      const row = section.locator('.config-item').filter({ has: page.locator(`[id=${JSON.stringify(control.id)}]`) });
      const savedValue = () => page.evaluate(key => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state.params.config_inputs[key], control.key);
      const values: (string | boolean | number)[] = control.type === 'checkbox' ? [!control.checked]
        : control.type === 'select-one' ? [...new Set(control.choices)]
        : control.type === 'number' ? [Number(control.value || 0) + 1] : ['+10 to maximum Life'];
      for (const value of values) {
        await expect(input, control.key).toBeEnabled();
        if (control.type === 'checkbox') await input.setChecked(Boolean(value));
        else if (control.type === 'select-one') {
          await input.click();
          await page.getByRole('option').nth(control.choices.indexOf(String(value))).click();
          await expect(page.getByRole('listbox')).toHaveCount(0);
        }
        else { await input.fill(String(value)); await input.press('Enter'); }
        await expect(input, control.key).toBeEnabled();
        await expect(page.locator('.calc-error'), control.key).toHaveCount(0);
        const actual = await savedValue();
        // Selecting the already displayed default need not materialize an override.
        if (control.type !== 'select-one' || String(value) !== control.value || actual !== undefined) {
          await expect.poll(savedValue, control.key).toEqual(value);
        }
        choices++;
      }
      const reset = row.locator('.config-reset');
      if (await reset.count()) await reset.click();
      await expect(input, control.key).toBeEnabled();
      await expect.poll(savedValue, `${control.key} reset`).toBeUndefined();
    }
    summary.push({ section: (await header.innerText()).replaceAll('\n', ' '), controls: controls.length, choices });
    await header.click();
  }
  expect(visitedKeys.sort()).toEqual(expectedKeys);
  console.log('Config interaction coverage:', JSON.stringify(summary));
});
