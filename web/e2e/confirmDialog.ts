import { expect, type Page } from '@playwright/test';

/**
 * Answers the confirmation the app has just asked for. The shell asks with its own dialog, not the
 * browser's, so call this after the action that opens it: it waits for the dialog, clicks the answer
 * and waits for the dialog to close, so the next step starts from a settled page.
 */
export async function answerConfirm(page: Page, accept: boolean) {
  // Cancel comes first in the dialog's actions, then the one that goes ahead; the labels are localized.
  const actions = page.getByRole('alertdialog').locator('.app-dialog-actions button');
  await (accept ? actions.last() : actions.first()).click();
  await expect(page.getByRole('alertdialog')).toHaveCount(0);
}
