import type { ConfigInputValue, ConfigOption } from '../api/types';

/** Match the catalog defaults consumed by the Rust config interpreter. */
export function configDefaultValue(option: ConfigOption): ConfigInputValue | undefined {
  const value = option.default;
  if (value !== undefined && typeof value !== 'object') return value;
  if (option.input_type === 'check') return value?.state_bool ?? false;
  if (option.input_type === 'list') return option.list_options?.[(value?.index ?? 1) - 1]?.value;
  return value?.state_number ?? value?.placeholder_number;
}
