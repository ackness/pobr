import { expect, test } from '@playwright/test';
import { answerConfirm } from './confirmDialog';

for (const reducedMotion of ['no-preference', 'reduce'] as const) {
  test(`cancelling deletion returns keyboard focus and allows immediately reopening the menu (${reducedMotion})`, async ({ page }) => {
    await page.emulateMedia({ reducedMotion });
    await page.goto('/build');
    await expect(page.getByLabel('Level', { exact: true })).toBeEnabled({ timeout: 90_000 });
    await page.locator('.bd-actions').getByRole('button', { name: 'New build', exact: true }).click();
    await page.getByRole('textbox', { name: 'Name', exact: true }).fill('Keep me');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.locator('.shell-save-state[data-busy]')).toHaveCount(0);
    const trigger = page.getByRole('button', { name: 'Actions for Keep me', exact: true });
    const dialog = page.getByRole('alertdialog');
    const deletion = page.getByRole('menuitem', { name: 'Delete build', exact: true });
    for (const close of ['Escape', 'Cancel']) {
      await trigger.focus();
      await trigger.press('ArrowDown');
      await deletion.focus();
      await deletion.press('Enter');
      await expect(dialog.getByRole('button', { name: 'Cancel', exact: true })).toBeFocused();
      await page.keyboard.press('Shift+Tab');
      await expect(dialog.getByRole('button', { name: 'Confirm', exact: true })).toBeFocused();
      await page.keyboard.press('Tab');
      await expect(dialog.getByRole('button', { name: 'Cancel', exact: true })).toBeFocused();
      if (close === 'Escape') await page.keyboard.press('Escape');
      else await page.keyboard.press('Enter');
      await expect(trigger).toBeFocused();
    }
    // The old menu can still be exiting when its own trigger opens it again.
    await trigger.click();
    await page.keyboard.press('Escape');
    await trigger.click();
    await deletion.click();
    await answerConfirm(page, false);
    await trigger.click();
    await deletion.focus();
    // Cross the deferred close autofocus and menu entrance animation before checking persistence.
    await page.evaluate(async () => {
      await Promise.all(document.getAnimations().map(animation => animation.finished.catch(() => {})));
      await new Promise(resolve => setTimeout(resolve, 0));
    });
    await expect(page.getByRole('menu')).toBeVisible();
    await expect(deletion).toBeFocused();
  });
}

