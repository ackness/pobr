import type { ConfigInputValue, ConfigOption } from '../api/types';

/** Match the catalog defaults consumed by the Rust config interpreter. */
export function configDefaultValue(option: ConfigOption): ConfigInputValue | undefined {
  const value = option.default;
  if (value !== undefined && typeof value !== 'object') return value;
  if (option.input_type === 'check') return value?.state_bool ?? false;
  if (option.input_type === 'list') return option.list_options?.[(value?.index ?? 1) - 1]?.value;
  return value?.state_number ?? value?.placeholder_number;
}

/** Reset restores the imported value, or the catalog default when it was absent. */
export function configValueDiffers(
  option: ConfigOption,
  current: ConfigInputValue | undefined,
  imported: ConfigInputValue | undefined,
): boolean {
  const fallback = configDefaultValue(option);
  const value = current ?? fallback;
  const baseline = imported ?? fallback;
  // Checkbox inputs accept numeric flags too; keep arbitrary text distinctly typed.
  if (option.input_type === 'check'
    && (typeof value === 'boolean' || typeof value === 'number')
    && (typeof baseline === 'boolean' || typeof baseline === 'number')) {
    return Boolean(value) !== Boolean(baseline);
  }
  return value !== baseline;
}
