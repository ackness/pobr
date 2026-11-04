import type { UtilitySlotInput } from '../api/types';

/** Text edits and replacements retain selection; a newly equipped utility is selected. */
export function replaceUtility(items: UtilitySlotInput[], slot: string, text: string | null): UtilitySlotInput[] {
  const rest = items.filter(item => item.slot !== slot);
  return text === null ? rest : [...rest, { slot, text, active: items.find(item => item.slot === slot)?.active ?? true }];
}
