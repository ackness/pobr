import type { ConfigInputValue, ConfigOption } from '../../api/types';
import { bindT, type Lang } from '../../lib/i18n';
import { configValueDiffers } from '../../lib/configDefaults';
import { showConfigOption, type Applicability } from '../../lib/configVisibility';
import { ToggleGroup, ToggleGroupItem } from '../ui/toggle-group';

export type ConfigScope = 'relevant' | 'modified' | 'all';

export function matchesConfigScope(option: ConfigOption, applicability: Applicability,
  inputs: Record<string, ConfigInputValue>, scope: ConfigScope): boolean {
  return scope === 'modified' ? configValueDiffers(option, inputs[option.var], undefined)
    : showConfigOption(option, applicability, inputs, scope === 'all');
}

export function ConfigScopeControl({ scope, count, lang, onChange }: {
  scope: ConfigScope; count: number; lang: Lang; onChange: (scope: ConfigScope) => void;
}) {
  const tt = bindT(lang);
  return <ToggleGroup className="config-view flex-wrap" type="single" value={scope} aria-label={tt('config.view')}
    onValueChange={value => { if (value) onChange(value as ConfigScope); }}>
    <ToggleGroupItem value="relevant">{tt('config.applicable')}</ToggleGroupItem>
    <ToggleGroupItem value="modified">{tt('config.configuredOnly').replace('{count}', String(count))}</ToggleGroupItem>
    <ToggleGroupItem value="all">{tt('config.showAll')}</ToggleGroupItem>
  </ToggleGroup>;
}
