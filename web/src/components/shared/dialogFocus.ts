/** Radix dispatches close autofocus on the content, even after its exit animation unmounts it. */
export function canRestoreDialogFocus(event: Pick<Event, 'target'>): boolean {
  const content = event.target as HTMLElement | null;
  if (!content) return false;
  const { activeElement, body } = content.ownerDocument;
  return activeElement === body || content.contains(activeElement);
}
