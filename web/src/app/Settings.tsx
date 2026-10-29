import { ExternalLink, Moon, Sun, Monitor, Type } from 'lucide-react';
import { Button } from '../components/ui/button';
import { Label } from '../components/ui/label';
import { Slider } from '../components/ui/slider';
import { Sheet, SheetContent, SheetHeader, SheetTitle, SheetDescription } from '../components/ui/sheet';
import { AppSelectControl } from '../components/shared/controls';
import { words } from './text';
import type { Preferences } from './preferences';

export function Settings({ open, onOpenChange, preferences: p, onChange, version }: {
  open: boolean; onOpenChange: (open: boolean) => void; preferences: Preferences; onChange: (next: Preferences) => void; version: string;
}) {
  const w = (en: string, cn: string, tw?: string) => words(p.lang, en, cn, tw);
  return <Sheet open={open} onOpenChange={onOpenChange}><SheetContent className="shell-settings" aria-describedby="appearance-description">
    <SheetHeader><SheetTitle>{w('Make it yours', '显示偏好', '顯示偏好')}</SheetTitle>
      <SheetDescription id="appearance-description">{w('Saved on this device. Your build stays as it is.', '自动保存在当前设备，调整时保留编辑状态。', '自動儲存在目前裝置，調整時保留編輯狀態。')}</SheetDescription></SheetHeader>
    <div className="shell-settings-fields">
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
      </AppSelectControl></div>
      <div className="shell-settings-about"><a href="https://github.com/ackness/pobr" target="_blank" rel="noopener noreferrer"><ExternalLink aria-hidden />{w('View source on GitHub (opens in a new tab)', '在 GitHub 查看源码（新标签页打开）', '在 GitHub 查看原始碼（另開分頁）')}</a><p>{w('Saved in this browser only.', '仅保存在当前浏览器。', '僅儲存在目前瀏覽器。')}</p><small>{version}</small></div>
      <div className="shell-font-preview"><Type aria-hidden /><p>{w('A clearer view of your next build.', '清晰掌握构筑的每一次变化。', '清晰掌握構築的每一次變化。')}</p><strong>1,284.50</strong><span> Life · DPS · 75%</span></div>
    </div>
  </SheetContent></Sheet>;
}
