import { expect, test } from '@playwright/test';
import { deflateSync } from 'node:zlib';
import { go, idle } from './configPage';

const code = deflateSync(Buffer.from('<PathOfBuilding2><Build level="80" className="Witch"/><Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree><Skills><Skill enabled="true"><Gem skillId="DisciplinePlayer" level="20" enabled="true"/></Skill></Skills><Items/></PathOfBuilding2>')).toString('base64url');

test('party members can be imported, edited, disabled, restored and removed', async ({ page }) => {
  await page.goto('/build');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await go(page, 'Config');
  await page.getByRole('tab', { name: 'Party', exact: true }).click();
  await page.getByLabel('Member build code', { exact: true }).fill(code);
  await page.getByRole('button', { name: 'Import member', exact: true }).click();
  await expect(page.locator('.party-editor fieldset')).toHaveCount(1);
  await expect.poll(() => page.getByLabel(/level 80 Auras$/).inputValue()).toContain('Discipline');
  // Enter commits the name; the inputs are disabled while the recalculation runs, so wait before the next edit.
  await page.getByLabel('Member name', { exact: true }).fill('Support');
  await page.getByLabel('Member name', { exact: true }).press('Enter');
  await idle(page);
  await page.screenshot({ path: 'test-results/party-editor.png', fullPage: true });
  await page.getByLabel('Enable member', { exact: true }).uncheck();
  await page.reload();
  // The reload lands on the Config page again, on its first tab.
  await page.getByRole('tab', { name: 'Party', exact: true }).click({ timeout: 90_000 });
  await expect(page.getByLabel('Member name', { exact: true })).toHaveValue('Support');
  await expect(page.getByLabel('Enable member', { exact: true })).not.toBeChecked();
  await page.getByLabel('Enable member', { exact: true }).check();
  await page.getByRole('button', { name: 'Remove member', exact: true }).click();
  await expect(page.locator('.party-editor fieldset')).toHaveCount(0);
  await page.getByRole('button', { name: 'Add member', exact: true }).click();
  await expect(page.locator('.party-editor fieldset')).toHaveCount(1);
});
