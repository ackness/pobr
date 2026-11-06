import type { ConfigInputValue, ConfigOption } from '../api/types';
import { configListOptionLabel, stripConfigMarkup } from './configLabels';
import { bindT, enemyTierLabel, type Lang } from './i18n';
import { formatNumber } from './statDisplay';

/** The configuration editor and comparisons use the same names for list values. */
export function configListLabel(lang: Lang, label: string, translated: Readonly<Record<string, string>> = {}): string {
  return stripConfigMarkup(label).split(/\r?\n/).map(line => {
    const text = line.trim();
    return configListOptionLabel(lang, text) ?? translated[text] ?? text;
  }).join(' / ');
}

/** Keep the saved type intact; this is a display projection only. */
export function configValueLabel(lang: Lang, option: ConfigOption | undefined, value: ConfigInputValue | undefined,
  translated: Readonly<Record<string, string>> = {}): string {
  const tt = bindT(lang);
  if (value === undefined || value === '') return tt('compare.notSet');
  if (option?.var === 'enemyIsBoss' && typeof value === 'string' && ['none', 'boss', 'pinnacle', 'uber'].includes(value.toLowerCase())) return enemyTierLabel(lang, value.toLowerCase());
  const label = option?.list_options?.find(entry => String(entry.value) === String(value))?.label;
  if (label && stripConfigMarkup(label)) return configListLabel(lang, label, translated);
  if (option?.input_type === 'list') return tt('compare.unlistedChoice');
  if (typeof value === 'boolean' || option?.input_type === 'check' && typeof value === 'number') return tt(value ? 'compare.yes' : 'compare.no');
  return typeof value === 'number' ? formatNumber(value, 2) : stripConfigMarkup(String(value));
}
