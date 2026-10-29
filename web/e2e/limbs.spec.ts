import { expect, test, type Page } from '@playwright/test';
import { openItemsAfterImport, statValue } from './shell';
import { settledNumber, shareCode } from './skillsAndCalcs';
import { deflateSync, inflateSync } from 'node:zlib';

const saved = (page: Page) => page.evaluate(() => JSON.parse(localStorage.getItem('pobr-v2-build-state')!).state);
const numeric = (value: string) => Number(value.replaceAll(',', ''));
const slots = (list: { slot: string }[]) => list.map(item => item.slot);

const leg = 'Rarity: NORMAL\nEvasive Leg\nImplicits: 1\n25% increased Evasion Rating';
const code = deflateSync(`<PathOfBuilding2><Build level="90" className="Ranger"/>
  <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree><Skills/>
  <Items activeItemSet="1"><Item id="1">
Rarity: NORMAL
Strider Vest
</Item><Item id="2">
${leg}
</Item><ItemSet id="1"><Slot name="Body Armour" itemId="1"/><Slot name="Leg 2" itemId="2"/></ItemSet></Items>
</PathOfBuilding2>`).toString('base64url');

test('limbs are a set of their own: imported, calculated, edited and exported', async ({ page }) => {
  await page.goto('/build');
  await expect(page.getByRole('textbox', { name: 'Build code' })).toBeVisible({ timeout: 90_000 });
  await page.getByRole('textbox', { name: 'Build code' }).fill(code);
  await page.locator('.import-submit').click();
  await openItemsAfterImport(page);
  await expect(page.locator('.paper-doll')).toBeVisible();

  const limbs = page.getByRole('group', { name: 'Limbs', exact: true });
  await expect(limbs.getByRole('button', { name: 'Leg 2', exact: true })).toContainText('Evasive Leg');
  const imported = await saved(page);
  expect(slots(imported.limbs)).toEqual(['leg2']);
  expect(slots(imported.items)).toEqual(['bodyarmour']);

  const evasion = statValue(page, /^Evasion$/).first();
  await expect(evasion).not.toHaveText('');
  const worn = await settledNumber(async () => numeric(await evasion.innerText()));
  await limbs.getByRole('button', { name: 'Leg 2', exact: true }).click();
  await page.locator('.item-detail-header').getByRole('button', { name: 'Remove', exact: true }).click();
  await expect.poll(async () => (await saved(page)).limbs).toEqual([]);
  await expect.poll(async () => numeric(await evasion.innerText())).toBeLessThan(worn);
  // The number eases to its new value, so wait for it to arrive.
  await expect.poll(async () => worn / numeric(await evasion.innerText())).toBeCloseTo(1.25, 2);

  // With no limb worn the Limbs section folds away; unfold it to reach an empty arm.
  const section = page.locator('details.doll-limbs');
  await expect(section).not.toHaveAttribute('open', '');
  await section.locator('summary').click();
  // An empty arm opens on a Combat Arm to edit.
  await limbs.getByRole('button', { name: 'Arm 1', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'arm1 item text' })).toHaveValue(/Combat Arm/);
  await page.getByRole('button', { name: 'Apply changes', exact: true }).click();
  await expect.poll(async () => slots((await saved(page)).limbs)).toEqual(['arm1']);
  await expect(limbs.getByRole('button', { name: 'Arm 1', exact: true })).toContainText('Combat Arm');

  const shared = await shareCode(page);
  const exported = inflateSync(Buffer.from(shared, 'base64url')).toString('utf8');
  // The item library keeps the removed leg; the item set no longer wears it.
  expect(exported).toMatch(/<Slot name="Arm 1" itemId="\d+"\/>/);
  expect(exported).toContain('Combat Arm');
  expect(exported).not.toContain('<Slot name="Leg 2"');
});
