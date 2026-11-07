import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { choose, configGroupNames, go, idle, nativeSelect, noMotion, openConfigGroup, savedState } from './configPage';
import { EXTENDED } from './tags';

test.use(noMotion);

// Controls the data audit found not to exist in this game version are not offered on the page.
const gameRelevance: { data_version: string; excluded: Record<string, unknown> } =
  JSON.parse(readFileSync('src/lib/configGameRelevance.json', 'utf8'));

// The full catalog sweep takes over two minutes, so it is split into shares that run on separate workers: share k
// exercises every SHARES-th control in catalog order, and every share checks that the groups cover the whole catalog.
// Focused config interactions stay in the default specs.
const SHARES = 4;
for (let share = 0; share < SHARES; share++) {
  test(`every catalog control and list choice can be changed, recalculated and reset (share ${share + 1}/${SHARES})`, EXTENDED, async ({ page }) => {
    test.setTimeout(600_000);
    await page.goto('/build');
    await expect(page.getByRole('heading', { name: 'Character', exact: true })).toBeVisible({ timeout: 90_000 });
    await go(page, 'Configuration');
    await page.locator('.config-view').getByRole('radio', { name: 'All', exact: true }).click();
    await expect(page.locator('.config-section-header').first()).toBeVisible();
    await idle(page);
    const initialInputs = (await savedState(page)).params.config_inputs;
    const { version } = await (await page.request.get('/data/manifest.json')).json();
    const catalog: { options: { var: string }[] } = await (await page.request.get(`/data/${version}/overlay/config_options.json`)).json();
    // Repeated catalog definitions resolve to one control per saved key.
    const absent = new Set(gameRelevance.data_version === version ? Object.keys(gameRelevance.excluded) : []);
    const expectedKeys = [...new Set(catalog.options.map(option => option.var))]
      .filter(key => key !== 'customMods' && key !== 'enemyIsBoss' && !absent.has(key)).sort();
    const visitedKeys: string[] = [];
    let index = 0;
    const summary: { section: string; controls: number; choices: number }[] = [];
    for (const name of await configGroupNames(page)) {
      const section = await openConfigGroup(page, name);
      // A list option is a picker: a combobox button that carries the id, and a hidden native select that mirrors its choices.
      const controls = await section.locator('.config-item').evaluateAll(rows => rows.map(row => {
        const control = row.querySelector('input[id^="config-"], button[id^="config-"]') as HTMLInputElement | HTMLButtonElement;
        const select = control.getAttribute('role') === 'combobox' ? control.parentElement!.querySelector('select')! : null;
        const field = select ?? control as HTMLInputElement;
        return { key: row.getAttribute('data-config-var')!, id: control.id, type: select ? 'select-one' : (control as HTMLInputElement).type,
          value: field.value, checked: (control as HTMLInputElement).checked,
          placeholder: (control as HTMLInputElement).placeholder,
          choices: select ? [...select.options].map(option => ({ value: option.value, label: option.label })) : [] };
      }));
      let choices = 0;
      for (const control of controls) {
        expect(control.id).toBe(`config-${control.key}`);
        visitedKeys.push(control.key);
        if (index++ % SHARES !== share) continue;
        // Setting a control does not move its row to another group, which would take it out of reach when that group is folded.
        const input = section.locator(`[id=${JSON.stringify(control.id)}]`);
        const row = section.locator('.config-item').filter({ has: page.locator(`[id=${JSON.stringify(control.id)}]`) });
        const savedValue = async () => (await savedState(page)).params.config_inputs[control.key];
        const displayedNumber = Number.parseFloat(control.value || control.placeholder);
        const values: (string | boolean | number)[] = control.type === 'checkbox' ? [!control.checked]
          // Visit the baseline first and finish on a different value, so Reset is offered even when the
          // catalog's last choice is its default. Choosing the displayed default need not save an override.
          : control.type === 'select-one' ? [control.value, ...new Set(control.choices.map(choice => choice.value).filter(value => value !== control.value))]
          : control.type === 'number' ? [(Number.isFinite(displayedNumber) ? displayedNumber : 0) + 1] : ['+10 to maximum Life'];
        for (const value of values) {
          if (control.type === 'checkbox') await input.setChecked(Boolean(value));
          else if (control.type === 'select-one') {
            await choose(page, input, control.choices.find(choice => choice.value === String(value))!.label);
            await expect(nativeSelect(input), control.key).toHaveValue(String(value));
          }
          else { await input.fill(String(value)); await input.press('Enter'); }
          if (control.type !== 'select-one' || String(value) !== control.value) {
            await expect.poll(savedValue, control.key).toEqual(value);
          } else {
            expect(await savedValue(), control.key).toEqual(initialInputs[control.key]);
          }
          await idle(page);
          choices++;
        }
        const reset = row.locator('.config-reset');
        await reset.click();
        await expect.poll(savedValue, `${control.key} reset`).toEqual(initialInputs[control.key]);
        await idle(page);
        if (control.type === 'checkbox') await expect(input, `${control.key} reset`).toBeChecked({ checked: control.checked });
        else await expect(control.type === 'select-one' ? nativeSelect(input) : input, `${control.key} reset`).toHaveValue(control.value);
      }
      summary.push({ section: name, controls: controls.length, choices });
    }
    // Every share reads every group, so this checks the whole catalog whichever share exercised a control.
    expect(visitedKeys.sort()).toEqual(expectedKeys);
    expect((await savedState(page)).params.config_inputs).toEqual(initialInputs);
    console.log(`Config interaction coverage (share ${share + 1}/${SHARES}):`, JSON.stringify(summary));
  });
}
