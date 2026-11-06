import type { Page } from '@playwright/test';

/**
 * Answers the confirmation the app has just asked for. The shell asks with its own dialog, not the
 * browser's, so call this after the action that opens it to click the answer.
 */
export async function answerConfirm(page: Page, accept: boolean) {
  // Cancel comes first in the dialog's actions, then the one that goes ahead; the labels are localized.
  const actions = page.getByRole('alertdialog').locator('.app-dialog-actions button');
  await (accept ? actions.last() : actions.first()).click();
}
