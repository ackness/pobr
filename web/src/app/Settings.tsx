import { ExternalLink, Moon, Sun, Monitor } from 'lucide-react';
import { Button } from '../components/ui/button';
import { Label } from '../components/ui/label';
import { Slider } from '../components/ui/slider';
import { Sheet, SheetContent, SheetHeader, SheetTitle, SheetDescription } from '../components/ui/sheet';
import { AppSelectControl } from '../components/shared/controls';
import { words, pageNames } from './text';
import { ROUTES, type Route } from './routes';
import type { Preferences } from './preferences';
import { displayModeOptions, type DisplayMode } from '../lib/displayModes';

export function Settings({ open, onOpenChange, preferences: p, onChange, version }: {
  open: boolean; onOpenChange: (open: boolean) => void; preferences: Preferences; onChange: (next: Preferences) => void; version: string;
}) {
  const w = (en: string, cn: string, tw?: string) => words(p.lang, en, cn, tw);
  const modes = displayModeOptions(p.lang);
  const setPageMode = (page: Route, mode: string) => {
    const pageDisplayModes = { ...p.pageDisplayModes };
    if (mode === 'inherit') delete pageDisplayModes[page];
    else pageDisplayModes[page] = mode as DisplayMode;
    onChange({ ...p, pageDisplayModes });
  };
  return <Sheet open={open} onOpenChange={onOpenChange}><SheetContent className="shell-settings" aria-describedby="appearance-description">
    <SheetHeader><SheetTitle>{w('Display settings', '显示设置', '顯示設定')}</SheetTitle>
      <SheetDescription id="appearance-description">{w('Changes apply immediately and are saved in this browser.', '立即生效，保存在当前浏览器。', '立即生效，儲存在目前瀏覽器。')}</SheetDescription></SheetHeader>
    <div className="shell-settings-fields">
      <fieldset className="display-mode-picker"><legend>{w('Display mode', '展示方式', '顯示方式')}</legend>
        {modes.map(mode => <label className="display-mode-choice" key={mode.value} data-selected={p.displayMode === mode.value || undefined}>
          <input className="sr-only" type="radio" name="display-mode" value={mode.value} checked={p.displayMode === mode.value}
            onChange={() => onChange({ ...p, displayMode: mode.value })} />
          <span className="display-mode-thumbnail" data-mode={mode.value} aria-hidden="true"><i /><i /><i /><i /></span>
          <span><strong>{mode.label}</strong><small>{mode.description}</small></span>
        </label>)}
        <p className="shell-setting-hint">{w('The default for every page. Individual page choices take precedence.', '用作所有页面的默认方式。单独设置的页面保持原选择。', '各頁面預設使用此方式，個別頁面的設定優先。')}</p>
      </fieldset>
      <details className="display-page-overrides"><summary>{w('Customize individual pages', '为个别页面单独设置', '個別頁面設定')}{Object.keys(p.pageDisplayModes).length > 0 && ` (${Object.keys(p.pageDisplayModes).length})`}</summary>
        {ROUTES.map(page => <div className="display-page-option" key={page}>
          <Label htmlFor={`display-page-${page}`}>{pageNames[p.lang][page]}</Label>
          <AppSelectControl id={`display-page-${page}`} value={p.pageDisplayModes[page] ?? 'inherit'} onChange={event => setPageMode(page, event.target.value)}>
            <option value="inherit">{w('Follow global setting', '跟随全局', '跟隨全域設定')}</option>
            {modes.map(mode => <option key={mode.value} value={mode.value}>{mode.label}</option>)}
          </AppSelectControl>
        </div>)}
        {Object.keys(p.pageDisplayModes).length > 0 && <Button variant="outline" onClick={() => onChange({ ...p, pageDisplayModes: {} })}>{w('Use global setting on every page', '所有页面跟随全局', '所有頁面跟隨全域設定')}</Button>}
      </details>
      <fieldset><legend>{w('Appearance', '外观', '外觀')}</legend><div className="shell-theme-options">
        {([['dark', Moon, w('Dark', '深色')], ['light', Sun, w('Light', '浅色', '淺色')], ['system', Monitor, w('System', '跟随系统', '跟隨系統')]] as const).map(([theme, Icon, label]) =>
          <Button key={theme} variant={p.theme === theme ? 'default' : 'outline'} aria-pressed={p.theme === theme} onClick={() => onChange({ ...p, theme })}><Icon />{label}</Button>)}
      </div></fieldset>
      <div><Label htmlFor="shell-language">{w('Language', '界面语言', '介面語言')}</Label><AppSelectControl id="shell-language" value={p.lang} onChange={e => onChange({ ...p, lang: e.target.value as Preferences['lang'] })}>
        <option value="zh-CN">简体中文</option><option value="zh-TW">繁體中文</option><option value="en-US">English</option>
      </AppSelectControl></div>
      <div><Label htmlFor="shell-font">{w('Typeface', '字体', '字體')}</Label><AppSelectControl id="shell-font" value={p.font} onChange={e => onChange({ ...p, font: e.target.value as Preferences['font'] })}>
        <option value="system">{w('System default', '系统默认', '系統預設')}</option><option value="sans">{w('Sans serif', '无衬线', '無襯線')}</option><option value="serif">{w('Serif', '衬线', '襯線')}</option>
      </AppSelectControl></div>
      <div><Label id="shell-font-size">{w('Text size', '字号', '字號')} · {p.fontSize}px</Label>
        <Slider aria-labelledby="shell-font-size" min={14} max={32} step={1} value={[p.fontSize]} onValueChange={([fontSize]) => onChange({ ...p, fontSize })} />
        <p className="shell-setting-hint">{w('Up to 200% of the default size.', '最高可放大到默认字号的 200%。', '最高可放大至預設字號的 200%。')}</p>
      </div>
      <div><Label htmlFor="shell-density">{w('Density', '信息密度', '資訊密度')}</Label><AppSelectControl id="shell-density" value={p.density} onChange={e => onChange({ ...p, density: e.target.value as Preferences['density'] })}>
        <option value="comfortable">{w('Comfortable', '舒适', '舒適')}</option><option value="compact">{w('Compact', '紧凑', '緊湊')}</option>
      </AppSelectControl><p className="shell-setting-hint">{w('Adjusts spacing without shrinking text.', '只调整间距，不缩小字号。', '只調整間距，不縮小字級。')}</p></div>
      <div className="shell-settings-about"><a href="https://github.com/ackness/pobr" target="_blank" rel="noopener noreferrer"><ExternalLink aria-hidden />{w('View source on GitHub (opens in a new tab)', '在 GitHub 查看源码（新标签页打开）', '在 GitHub 查看原始碼（另開分頁）')}</a><small>{version}</small></div>
      <div className="shell-font-preview"><p>{w('Text preview', '字号预览', '字級預覽')}</p><strong>1,284.50</strong><span>{w('Life · DPS · 75%', '生命 · DPS · 75%', '生命 · DPS · 75%')}</span></div>
    </div>
  </SheetContent></Sheet>;
}
