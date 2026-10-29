import { expect, test, type Page } from '@playwright/test';
import { deflateSync } from 'node:zlib';
import { mainSkillPinned } from './buildLibrary';

const code = deflateSync('<PathOfBuilding2><Build level="60" className="Witch" mainSocketGroup="1"/><Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree><Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true"><Gem skillId="FireballPlayer" level="12" quality="0" enabled="true"/></Skill></SkillSet></Skills><Items/><Config><Input name="enemyLevel" number="82"/><Input name="buffOnslaught" boolean="false"/></Config></PathOfBuilding2>').toString('base64url');
const nav = (page: Page, name: string) => page.getByRole('navigation', { name: 'Main navigation' }).getByRole('link', { name, exact: true });
/** The build library is an action, not a workflow stage, so it has a button rather than a tab. */
const openBuilds = (page: Page) => page.getByRole('button', { name: 'BDs', exact: true }).click();
const state = (page: Page, key = 'pobr-v2-build-state') => page.evaluate(key => JSON.parse(localStorage.getItem(key)!), key);
async function preferences(page: Page, extra: Record<string, unknown> = {}) {
  await page.addInitScript(extra => {
    if (!localStorage.getItem('pobr-v2-preferences')) localStorage.setItem('pobr-v2-preferences', JSON.stringify({ lang: 'en-US', theme: 'dark', font: 'system', fontSize: 16, density: 'comfortable', ...extra }));
  }, extra);
}
async function importBuild(page: Page) {
  await preferences(page);
  await page.goto('/build');
  await expect(page.getByRole('textbox', { name: 'Build code', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click();
  await expect(page.locator('#ov-title')).toBeVisible();
  await expect(page.locator('.shell-save-state')).toHaveText('Saved locally');
}

test('starts and preserves saved builds without crypto.randomUUID', async ({ page }) => {
  await page.addInitScript(() => Object.defineProperty(crypto, 'randomUUID', { value: undefined }));
  await importBuild(page);
  const imported = await state(page);
  await page.reload();
  await expect(page.locator('#ov-title')).toBeVisible({ timeout: 90_000 });
  await expect(page.locator('.shell-save-state')).toHaveText('Saved locally');
  const restored = await state(page);
  expect(restored.workspace).toEqual(imported.workspace);
  expect(restored.state).toEqual(imported.state);
});

test('copies saves of the first interface once and config edits preserve the original workspace', async ({ page }) => {
  const dialogs: string[] = []; page.on('dialog', dialog => { dialogs.push(dialog.type()); void dialog.dismiss(); });
  await preferences(page);
  await page.goto('/build');
  await expect(page.getByRole('textbox', { name: 'Build code', exact: true })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code', exact: true }).fill(code);
  await page.locator('.import-submit').click(); await expect(page.locator('#ov-title')).toBeVisible();
  await expect(page.locator('.shell-save-state')).toHaveText('Saved locally');
  // Play a device whose last visit was to the first interface: its saves sit under the old keys and
  // none of the current keys exist yet. Opening the app must copy them over, once, without touching the source.
  const original = await page.evaluate(() => {
    const raw = localStorage.getItem('pobr-v2-build-state')!;
    localStorage.setItem('pobr-build-state', raw);
    for (const key of ['pobr-v2-build-state', 'pobr-v2-library', 'pobr-v2-notes', 'pobr-v2-storage-initialized', 'pobr-v2-original-backup']) localStorage.removeItem(key);
    return raw;
  });
  await page.goto('/config');
  await expect(page.locator('#config-heading')).toBeVisible({ timeout: 90_000 });
  // The copy is the same save (the app may add fields it now writes by default, so compare what the import carried).
  expect((await state(page)).state.pobCode).toBe(JSON.parse(original).state.pobCode);
  expect((await state(page)).state.params.config_inputs.enemyLevel).toBe(82);
  await page.getByRole('searchbox').fill('enemyLevel');
  const level = page.locator('#config-enemyLevel'); await expect(level).toBeVisible();
  await level.fill('75'); expect((await state(page)).state.params.config_inputs.enemyLevel).toBe(82);
  await level.press('Enter');
  await expect.poll(async () => (await state(page)).state.params.config_inputs.enemyLevel).toBe(75);
  expect(await page.evaluate(() => localStorage.getItem('pobr-build-state'))).toBe(original);
  await page.reload(); await expect(page.locator('#config-heading')).toBeVisible({ timeout: 90_000 });
  expect((await state(page)).state.params.config_inputs.enemyLevel).toBe(75);
  expect(JSON.parse((await page.evaluate(() => localStorage.getItem('pobr-v2-original-backup')))! ).workspace).toBe(original);
  expect(dialogs).toEqual([]);
});

/** Lets a tab be made deaf to `storage` events (`window.deaf = true`), as a frozen or sleeping tab is. */
async function deafToStorage(page: Page) {
  await page.addInitScript(() => {
    const add = window.addEventListener.bind(window);
    window.addEventListener = ((type: string, listener: EventListenerOrEventListenerObject, options?: boolean | AddEventListenerOptions) => {
      if (type !== 'storage') return add(type, listener, options);
      return add(type, (event: Event) => {
        if (Reflect.get(window, 'deaf')) return;
        if (typeof listener === 'function') listener(event); else listener.handleEvent(event);
      }, options);
    }) as typeof window.addEventListener;
  });
}

test('tabs of one browser follow each other and merge what a tab missed', async ({ page, context }) => {
  await deafToStorage(page);
  await importBuild(page);
  const other = await context.newPage();
  await other.goto('/build');
  await expect(other.locator('#build-heading')).toBeVisible({ timeout: 90_000 });
  // The list on the Build page shows what each tab holds; the header's select only renders the open one.
  const shown = (tab: Page) => tab.getByRole('group', { name: 'My builds', exact: true }).locator('li strong');
  const names = (tab: Page, list: string[]) => expect(shown(tab)).toHaveText(list.map(name => name || 'My build'));
  const notice = (tab: Page) => tab.locator('.shell-notice[data-sync]');
  const create = async (tab: Page, value: string) => {
    await tab.getByRole('button', { name: 'New build', exact: true }).last().click();
    await tab.getByRole('textbox', { name: 'Name', exact: true }).fill(value);
    await tab.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(tab.locator('.shell-save-state')).toHaveText('Saved locally');
  };
  const stored = async (tab: Page): Promise<string[]> =>
    (await state(tab)).workspace.builds.map((build: { name: string }) => build.name);
  const start = await stored(page);

  // A build made in one tab appears in the other without a reload, and says so.
  await openBuilds(page);
  await create(page, 'Made here');
  await names(other, [...start, 'Made here']);
  await expect(notice(other)).toHaveAttribute('data-sync', 'updated');

  // Deleting the build the other tab has open: that tab moves on to one that is still there.
  await other.getByRole('group', { name: 'My builds', exact: true }).getByRole('button', { name: /Made here/ }).click();
  await expect(other.locator('.shell-save-state')).toHaveText('Saved locally');
  await openBuilds(page);
  await page.getByRole('button', { name: 'Delete build', exact: true }).click();
  await page.getByRole('button', { name: 'Confirm', exact: true }).click();
  await names(other, start);
  await expect(notice(other)).toHaveAttribute('data-sync', 'current-removed');
  expect(await stored(page)).toEqual(start);

  // A tab that never heard of a change (a frozen or sleeping one) still keeps it when it writes.
  await page.evaluate(() => Reflect.set(window, 'deaf', true));
  await openBuilds(other);
  await create(other, 'Made there');
  await names(page, start);
  await openBuilds(page);
  await create(page, 'Made here too');
  expect(await stored(page)).toEqual([...start, 'Made there', 'Made here too']);
  await names(page, [...start, 'Made there', 'Made here too']);
  await expect(notice(page)).toHaveAttribute('data-sync', 'updated');
  // The other tab follows the merged library; nothing either tab made is lost.
  await names(other, [...start, 'Made there', 'Made here too']);
});

test('tabs of one browser share the saved skill sets', async ({ page, context }) => {
  await deafToStorage(page);
  await importBuild(page);
  const other = await context.newPage();
  await other.goto('/skills');
  await expect(other.locator('details.skills-advanced summary')).toBeVisible({ timeout: 90_000 });
  await nav(page, 'Skills').click();
  await expect(page.locator('details.skills-advanced summary')).toBeVisible();
  const chips = (tab: Page) => tab.locator('.skill-set-chip');
  const stored = async (tab: Page): Promise<string[]> => (await state(tab, 'pobr-v2-library')).skillSets.map((set: { name: string }) => set.name);
  const save = async (tab: Page, name: string) => {
    await tab.locator('details.skills-advanced').evaluate(details => { (details as HTMLDetailsElement).open = true; });
    await tab.getByRole('textbox', { name: 'Set name…', exact: true }).fill(name);
    await tab.getByRole('button', { name: 'Save current as set', exact: true }).click();
  };

  // An import already leaves a set named after the class; the others are counted on top of it.
  const start = await stored(page);
  const count = (extra: number) => start.length + extra;

  // A set saved in one tab shows up in the other without a reload.
  await save(page, 'Set A');
  await expect(chips(other)).toHaveCount(count(1));
  await expect(chips(other).last()).toContainText('Set A');

  // A tab that never heard of the next one keeps it when it saves its own.
  await page.evaluate(() => Reflect.set(window, 'deaf', true));
  await save(other, 'Set B');
  await expect(chips(page)).toHaveCount(count(1));
  await save(page, 'Set C');
  await expect.poll(() => stored(page)).toEqual([...start, 'Set A', 'Set B', 'Set C']);
  await expect(chips(page)).toHaveCount(count(3));
  await expect(chips(other)).toHaveCount(count(3));

  // Removing one in a tab that has been told removes it for the other too.
  await page.evaluate(() => Reflect.set(window, 'deaf', false));
  await chips(other).filter({ hasText: 'Set B' }).getByRole('button', { name: '×', exact: true }).click();
  await expect.poll(() => stored(other)).toEqual([...start, 'Set A', 'Set C']);
  await expect(chips(page)).toHaveCount(count(2));
});

test('navigation, settings and confirmations retain the current editor', async ({ page }) => {
  await importBuild(page);
  await expect(page.getByRole('note')).toContainText('configuration');
  await openBuilds(page);
  const notes = page.getByRole('textbox', { name: 'Notes', exact: true });
  await notes.fill('Persistent stage note'); await notes.blur();
  const before = (await state(page)).state;
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('button', { name: 'Light', exact: true }).click();
  await expect(page.locator('html')).not.toHaveClass(/dark/);
  await page.getByRole('slider', { name: /Text size/ }).focus(); await page.keyboard.press('End');
  await expect(page.locator('html')).toHaveCSS('font-size', '32px');
  await page.getByRole('slider', { name: /Text size/ }).focus(); await page.keyboard.press('Home');
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(notes).toHaveValue('Persistent stage note');
  expect((await state(page)).state).toEqual(before);
  await nav(page, 'Calcs').click(); await expect(page).toHaveURL(/\/calcs$/);
  await page.goBack(); await expect(page.locator('#build-heading')).toBeVisible();
  await expect(notes).toHaveValue('Persistent stage note');
  await page.getByRole('button', { name: 'Copy current stage', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Name', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.getByRole('combobox', { name: 'Class', exact: true }).click();
  await page.getByRole('option', { name: 'Warrior', exact: true }).click();
  await expect(page.getByRole('alertdialog')).toBeVisible();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  expect((await state(page)).state.character.class_name).toBe('Witch');
  expect((await state(page)).workspace.builds.at(-1).stages).toHaveLength(1);
});

test('deletes a build or a stage only after asking, and keeps the last of each', async ({ page }) => {
  await importBuild(page);
  const deleteBuild = page.getByRole('button', { name: 'Delete build', exact: true });
  const deleteStage = page.getByRole('button', { name: 'Delete stage', exact: true });
  const undo = page.getByRole('button', { name: 'Undo', exact: true });
  const name = async (action: string, value: string) => {
    // The import form has a "New build" choice of its own; the action is the later of the two.
    await page.getByRole('button', { name: action, exact: true }).last().click();
    await page.getByRole('textbox', { name: 'Name', exact: true }).fill(value);
    await page.getByRole('button', { name: 'Save', exact: true }).click();
  };
  const builds = async () => (await state(page)).workspace.builds.map((build: { name: string; stages: unknown[] }) => `${build.name}:${build.stages.length}`);
  await openBuilds(page);
  // An import leaves the empty starter build beside the imported one, so there is something to delete.
  const start: string[] = await builds();
  expect(start.length).toBeGreaterThan(1);
  await expect(deleteBuild).toBeVisible();
  // Each build here has one stage, and a build's only stage is not offered for deletion.
  await expect(deleteStage).toHaveCount(0);

  await name('New build', 'Second');
  // Opening another build is not an edit to take back: Undo would otherwise put the build
  // that was just left over this one.
  await expect(undo).toBeDisabled();
  await name('Copy current stage', 'Later');
  await expect(undo).toBeDisabled();
  await expect(deleteStage).toBeVisible();
  expect(await builds()).toEqual([...start, 'Second:2']);

  // A stage: declining changes nothing; confirming removes it and keeps the build.
  await deleteStage.click();
  await expect(page.getByRole('alertdialog')).toContainText('Later');
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  expect(await builds()).toEqual([...start, 'Second:2']);
  await deleteStage.click();
  await page.getByRole('button', { name: 'Confirm', exact: true }).click();
  await expect.poll(builds).toEqual([...start, 'Second:1']);
  await expect(deleteStage).toHaveCount(0);
  await expect(undo).toBeDisabled();

  // A build: the confirmation names it, declining changes nothing, and confirming removes it.
  await deleteBuild.click();
  await expect(page.getByRole('alertdialog')).toContainText('Second');
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  expect(await builds()).toEqual([...start, 'Second:1']);
  await deleteBuild.click();
  await page.getByRole('button', { name: 'Confirm', exact: true }).click();
  await expect.poll(builds).toEqual(start);
  // Undo must not bring the deleted build's data back into the one that opened.
  await expect(undo).toBeDisabled();
  // What opens next is a build that is still there.
  const open = (await state(page)).workspace;
  expect(open.builds.some((build: { id: string }) => build.id === open.activeBuild)).toBe(true);

  // Deleting down to the last build: it stays, and nothing offers to delete it.
  while ((await builds()).length > 1) {
    await deleteBuild.click();
    await page.getByRole('button', { name: 'Confirm', exact: true }).click();
    await expect.poll(async () => (await builds()).length).toBeLessThan(start.length);
    start.pop();
  }
  await expect(deleteBuild).toHaveCount(0);
  await expect(page.locator('.shell-save-state')).toHaveText('Saved locally');

  // It is gone for good: still one build after a reload, and it is the one that was kept.
  const kept = (await state(page)).workspace.activeBuild;
  await page.reload();
  await expect(page.locator('#build-heading')).toBeVisible({ timeout: 90_000 });
  expect(await builds()).toHaveLength(1);
  expect((await state(page)).workspace.activeBuild).toBe(kept);
});

for (const viewport of [{ width: 390, size: 32 }, { width: 1440, size: 16 }, { width: 3440, size: 16 }]) {
  test(`functional pages fit ${viewport.width}px with ${viewport.size}px text`, async ({ page }) => {
    await page.setViewportSize({ width: viewport.width, height: 900 });
    await preferences(page, { fontSize: viewport.size }); await importBuild(page);
    const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
    for (const route of ['overview', 'build', 'skills', 'items', 'calcs', 'config', 'compare', 'trade', 'tree']) {
      await page.goto(route === 'overview' ? '/' : `/${route}`);
      await expect(page.locator('.shell-main')).toBeVisible({ timeout: 90_000 });
      await expect(page.locator('.shell-panel-loading')).toHaveCount(0);
      const widths = await page.locator('main').evaluate(element => ({ client: element.clientWidth, scroll: element.scrollWidth }));
      expect(widths.scroll, `${route} has page-wide overflow`).toBeLessThanOrEqual(widths.client + 1);
      expect(await page.locator('main [title]').count(), `${route} uses browser tooltips`).toBe(0);
    }
    if (viewport.width === 390) {
      await page.getByRole('button', { name: 'Open menu', exact: true }).click();
      await page.getByRole('button', { name: 'Appearance & language', exact: true }).click();
      await page.setViewportSize({ width: 390, height: 400 });
      await page.getByRole('button', { name: 'Light', exact: true }).click();
      await expect(page.locator('body')).toHaveCSS('background-color', 'rgb(247, 246, 243)');
      const sheet = page.locator('.shell-settings');
      expect(await sheet.evaluate(element => element.scrollWidth <= element.clientWidth + 1)).toBe(true);
      await page.getByRole('button', { name: 'Close', exact: true }).click();
      await expect(sheet).toHaveCount(0);
    }
    expect(errors).toEqual([]);
  });
}

test('browser Back closes portals owned by retained skill panels', async ({ page }) => {
  await importBuild(page);
  await nav(page, 'Skills').click();
  const sets = page.getByRole('combobox', { name: 'Skill set', exact: true });
  await sets.click(); await expect(page.getByRole('listbox')).toBeVisible();
  await page.goBack(); await expect(page.locator('#ov-title')).toBeVisible();
  await expect(page.getByRole('listbox')).toHaveCount(0);
  await nav(page, 'Skills').click();
  await expect(sets).toHaveAttribute('aria-expanded', 'false');
});

// Each page that loads on demand has its own Suspense boundary. If they shared one, the first visit to Calcs, Compare,
// Upgrades or Tree would hide the retained Skills / Tree / Upgrades panels and re-run their layout effects when they
// came back, and those effects would discard the results the panels were holding.
test('a retained skill optimizer keeps its results through the first visit to a page that loads on demand', async ({ page }) => {
  await importBuild(page);
  await mainSkillPinned(page); // the best skill is pinned a moment after an import, which would discard the results
  await nav(page, 'Skills').click();
  const group = page.locator('.skill-group').first();
  if (await group.locator('.skill-group-title').getAttribute('aria-expanded') === 'false') await group.locator('.skill-group-title').click();
  if (await group.locator('.gem-optimizer-toggle').getAttribute('aria-expanded') === 'false') await group.locator('.gem-optimizer-toggle').click();
  await page.getByRole('button', { name: 'Calculate support combinations', exact: true }).click();
  await expect(page.locator('.support-plan').first()).toBeVisible({ timeout: 90_000 });
  const receipt = (await page.locator('.support-results').textContent())!; // textContent: toHaveText compares it, innerText applies CSS text-transform
  await nav(page, 'Calcs').click();
  await expect(page.locator('.shell-panel-loading')).toHaveCount(0, { timeout: 60_000 });
  await expect(page.locator('#breakdown-Life')).toBeVisible();
  await nav(page, 'Skills').click();
  await expect(page.locator('.support-results')).toHaveText(receipt);
});

test('real WASM details stay lazy and configuration tracking follows the active route', async ({ page }) => {
  await page.addInitScript(() => {
    const observed = window as typeof window & { calcRequests: Array<{ breakdowns?: string[]; config_context?: boolean }> };
    observed.calcRequests = [];
    const record = (value: unknown) => {
      if (value && typeof value === 'object' && 'character' in value && 'socket_groups' in value && 'config_inputs' in value) {
        observed.calcRequests.push(value);
      }
    };
    // On the page's own thread a request is stringified for the engine; with the engine in a Worker it is posted as an RPC call.
    const stringify = JSON.stringify;
    JSON.stringify = ((value: unknown, ...args: unknown[]) => {
      record(value);
      return Reflect.apply(stringify, JSON, [value, ...args]);
    }) as typeof JSON.stringify;
    const post = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function (this: Worker, message: unknown, ...rest: unknown[]) {
      const args = (message as { args?: unknown } | null)?.args;
      if (Array.isArray(args)) args.forEach(record);
      return Reflect.apply(post, this, [message, ...rest]);
    } as typeof Worker.prototype.postMessage;
  });
  const requests = () => page.evaluate(() => (window as typeof window & { calcRequests: Array<{ breakdowns?: string[]; config_context?: boolean; breakdown_actor?: unknown }> }).calcRequests);
  await importBuild(page);
  await nav(page, 'Calcs').click();
  await expect(page.locator('#breakdown-Life')).toBeVisible();
  expect((await requests()).filter(request => request.breakdowns?.length)).toHaveLength(0);
  await page.locator('#breakdown-Life').click();
  await expect(page.locator('#calcs-detail .formula-final')).toBeVisible();
  expect((await requests()).filter(request => request.breakdowns?.length)).toEqual([
    expect.objectContaining({ breakdowns: ['Life'], breakdown_actor: { kind: 'player' } }),
  ]);
  await nav(page, 'Config').click();
  await expect.poll(async () => (await requests()).at(-1)?.config_context).toBe(true);
  await openBuilds(page);
  await page.getByLabel('Level', { exact: true }).fill('61');
  await expect(page.locator('.shell-save-state')).toHaveText('Saved locally');
  expect((await requests()).at(-1)?.config_context).not.toBe(true);
  expect((await state(page)).state.character.level).toBe(61);
});
