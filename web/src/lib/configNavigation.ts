/** One pending field survives the page transition and catalog loading. */
let pendingField: string | null = null;
export const CONFIG_FIELD_EVENT = 'pobr-config-field';
export function requestConfigField(field: string): void {
  pendingField = field;
  window.dispatchEvent(new Event(CONFIG_FIELD_EVENT));
}
export function takeConfigField(): string | null {
  const field = pendingField;
  pendingField = null;
  return field;
}
