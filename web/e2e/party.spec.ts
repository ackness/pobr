import { expect, test } from '@playwright/test';
import { deflateSync } from 'node:zlib';

const code = deflateSync(Buffer.from('<PathOfBuilding2><Build level="80" className="Witch"/><Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree><Skills><Skill enabled="true"><Gem skillId="DisciplinePlayer" level="20" enabled="true"/></Skill></Skills><Items/></PathOfBuilding2>')).toString('base64url');

test('party members can be imported, edited, disabled, restored and removed', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
  await page.getByRole('button', { name: 'Config', exact: true }).click();
  await page.locator('.party-editor > summary').click();
  await page.getByLabel('Member build code', { exact: true }).fill(code);
  await page.getByRole('button', { name: 'Import member', exact: true }).click();
  await expect(page.locator('.party-editor fieldset')).toHaveCount(1);
  await expect.poll(() => page.getByLabel(/level 80 Aura$/).inputValue()).toContain('Discipline');
  await page.getByLabel('Member name', { exact: true }).fill('Support');
  await page.screenshot({ path: 'test-results/party-editor.png', fullPage: true });
  await page.getByLabel('Enable member', { exact: true }).uncheck();
  await page.reload();
  await expect(page.getByRole('button', { name: 'Config', exact: true })).toBeEnabled({ timeout: 90_000 });
  await page.getByRole('button', { name: 'Config', exact: true }).click();
  await page.locator('.party-editor > summary').click();
  await expect(page.getByLabel('Member name', { exact: true })).toHaveValue('Support');
  await expect(page.getByLabel('Enable member', { exact: true })).not.toBeChecked();
  await page.getByLabel('Enable member', { exact: true }).check();
  await page.getByRole('button', { name: 'Remove member', exact: true }).click();
  await expect(page.locator('.party-editor fieldset')).toHaveCount(0);
  await page.getByRole('button', { name: 'Add member', exact: true }).click();
  await expect(page.locator('.party-editor fieldset')).toHaveCount(1);
});
