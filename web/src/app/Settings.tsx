import { ExternalLink, Moon, Sun, Monitor } from 'lucide-react';
import { Button } from '../components/ui/button';
import { Label } from '../components/ui/label';
import { Slider } from '../components/ui/slider';
import { Sheet, SheetContent, SheetHeader, SheetTitle, SheetDescription } from '../components/ui/sheet';
import { AppSelectControl } from '../components/shared/controls';
import { pageNames } from './text';
import { ROUTES, type Route } from './routes';
import type { Preferences } from './preferences';
import { formatNumber } from '../lib/statDisplay';
import { displayModeOptions, type DisplayMode } from '../lib/displayModes';
import { bindT, LANGS, messages, statNameLabel } from '../lib/i18n';

export function Settings({ open, onOpenChange, preferences: p, onChange, version }: {
  open: boolean; onOpenChange: (open: boolean) => void; preferences: Preferences; onChange: (next: Preferences) => void; version: string;
}) {
  const tt = bindT(p.lang);
  const modes = displayModeOptions(p.lang);
  const setPageMode = (page: Route, mode: string) => {
    const pageDisplayModes = { ...p.pageDisplayModes };
    if (mode === 'inherit') delete pageDisplayModes[page];
    else pageDisplayModes[page] = mode as DisplayMode;
    onChange({ ...p, pageDisplayModes });
  };
  return <Sheet open={open} onOpenChange={onOpenChange}><SheetContent className="shell-settings" aria-describedby="appearance-description">
    <SheetHeader><SheetTitle>{tt('settings.title')}</SheetTitle>
      <SheetDescription id="appearance-description">{tt('settings.savedLocally')}</SheetDescription></SheetHeader>
    <div className="shell-settings-fields">
      <fieldset className="display-mode-picker"><legend>{tt('settings.displayMode')}</legend>
        <p className="shell-setting-hint">{tt('settings.displayScope')}</p>
        {modes.map(mode => <label className="display-mode-choice" key={mode.value} data-selected={p.displayMode === mode.value || undefined}>
          <input className="sr-only" type="radio" name="display-mode" value={mode.value} checked={p.displayMode === mode.value}
            onChange={() => onChange({ ...p, displayMode: mode.value })} />
          <span className="display-mode-thumbnail" data-mode={mode.value} aria-hidden="true"><i /><i /><i /><i /></span>
          <span><strong>{mode.label}</strong><small>{mode.description}</small></span>
        </label>)}
        <p className="shell-setting-hint">{tt('settings.displayModeHint')}</p>
      </fieldset>
      <details className="display-page-overrides"><summary className="disclosure" data-disclosure-level="group">{tt('settings.pageOverrides')}{Object.keys(p.pageDisplayModes).length > 0 && ` (${Object.keys(p.pageDisplayModes).length})`}</summary>
        {ROUTES.map(page => <div className="display-page-option" key={page}>
          <Label htmlFor={`display-page-${page}`}>{pageNames(p.lang)[page]}</Label>
          <AppSelectControl id={`display-page-${page}`} value={p.pageDisplayModes[page] ?? 'inherit'} onChange={event => setPageMode(page, event.target.value)}>
            <option value="inherit">{tt('settings.followGlobal')}</option>
            {modes.map(mode => <option key={mode.value} value={mode.value}>{mode.label}</option>)}
          </AppSelectControl>
        </div>)}
        {Object.keys(p.pageDisplayModes).length > 0 && <Button variant="outline" onClick={() => onChange({ ...p, pageDisplayModes: {} })}>{tt('settings.resetPageOverrides')}</Button>}
      </details>
      <fieldset><legend>{tt('settings.appearance')}</legend><div className="shell-theme-options segmented">
        {([['dark', Moon, tt('settings.dark')], ['light', Sun, tt('settings.light')], ['system', Monitor, tt('settings.system')]] as const).map(([theme, Icon, label]) =>
          <Button key={theme} variant={p.theme === theme ? 'default' : 'outline'} aria-pressed={p.theme === theme} onClick={() => onChange({ ...p, theme })}><Icon />{label}</Button>)}
      </div></fieldset>
      <div><Label htmlFor="shell-language">{tt('settings.language')}</Label><AppSelectControl id="shell-language" value={p.lang} onChange={e => onChange({ ...p, lang: e.target.value as Preferences['lang'] })}>
        {LANGS.map(lang => <option key={lang} value={lang}>{messages(lang, 'language').name}</option>)}
      </AppSelectControl></div>
      <div><Label htmlFor="shell-font">{tt('settings.typeface')}</Label><AppSelectControl id="shell-font" value={p.font} onChange={e => onChange({ ...p, font: e.target.value as Preferences['font'] })}>
        <option value="system">{tt('settings.systemFont')}</option><option value="sans">{tt('settings.sansFont')}</option><option value="serif">{tt('settings.serifFont')}</option>
      </AppSelectControl></div>
      <div><Label id="shell-font-size">{tt('settings.textSize')} · {p.fontSize}px</Label>
        <Slider aria-labelledby="shell-font-size" min={14} max={32} step={1} value={[p.fontSize]} onValueChange={([fontSize]) => onChange({ ...p, fontSize })} />
        <div className="shell-font-preview"><p>{tt('settings.textPreview')}</p>
          <div className="shell-font-sample"><span>{statNameLabel(p.lang, 'Life')}</span><strong>{formatNumber(1284.5, 2, 2)}</strong></div>
        </div>
        <p className="shell-setting-hint">{tt('settings.textSizeHint')}</p>
      </div>
      <div><Label htmlFor="shell-density">{tt('settings.density')}</Label><AppSelectControl id="shell-density" value={p.density} onChange={e => onChange({ ...p, density: e.target.value as Preferences['density'] })}>
        <option value="comfortable">{tt('settings.comfortable')}</option><option value="compact">{tt('settings.compact')}</option>
      </AppSelectControl><p className="shell-setting-hint">{tt('settings.densityHint')}</p></div>
      <div className="shell-settings-about"><a href="https://github.com/ackness/pobr" target="_blank" rel="noopener noreferrer"><ExternalLink aria-hidden />{tt('settings.sourceLink')}</a><small>{version}</small></div>
    </div>
  </SheetContent></Sheet>;
}
