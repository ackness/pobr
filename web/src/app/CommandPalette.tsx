import { useEffect, useMemo, useState } from 'react';
import { Command } from 'cmdk';
import { CornerDownLeft } from 'lucide-react';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '../components/ui/dialog';
import type { BuildSession } from '../hooks/useBuildSession';
import { useSkillCatalog } from '../hooks/useSkillName';
import type { Lang } from '../lib/i18n';
import { activeSkillEffects } from '../lib/skillSelection';
import { skillDisplayName } from '../lib/skillNames';
import { detailKind } from '../lib/statDetails';
import { formatStatValue, statMap } from '../lib/statDisplay';
import { statFormat, statLabel } from './inspector/statId';
import type { SettledCalc } from './motion/useSettledCalc';
import type { Route } from './routes';
import { matchingTiers } from './paletteTiers';
import { SHORTCUT_ROUTES } from './shortcuts';
import { words } from './text';

const MAX_STATS = 30;

export interface PaletteProps {
  open: boolean; onOpenChange: (open: boolean) => void; session: BuildSession; view: SettledCalc; lang: Lang;
  pageLabel: (route: Route) => string; onNavigate: (route: Route) => void; onInspect: (id: string) => void;
  onSettings: () => void; onToggleTheme: () => void; themeLabel: string;
}

/** ⌘K: jump to a page, open the calculation behind any stat, switch the main skill or the enemy, or run an action. */
export function CommandPalette({ open, onOpenChange, session, view, lang, pageLabel, onNavigate, onInspect, onSettings, onToggleTheme, themeLabel }: PaletteProps) {
  const w = (en: string, cn: string, tw?: string) => words(lang, en, cn, tw);
  const catalog = useSkillCatalog();
  const [query, setQuery] = useState('');
  useEffect(() => { if (open) setQuery(''); }, [open]);
  const q = query.trim().toLowerCase();
  const match = (...texts: string[]) => !q || texts.some(text => text.toLowerCase().includes(q));
  const run = (action: () => void) => () => { onOpenChange(false); action(); };

  const stats = useMemo(() => {
    if (q.length < 2 || !view.calc) return [];
    const values = statMap(view.calc.stats);
    const calc = view.calc;
    return calc.stats.filter(stat => detailKind(calc, stat.id) !== 'none' && match(statLabel(lang, stat.id), stat.id))
      .slice(0, MAX_STATS).map(stat => ({ id: stat.id, label: statLabel(lang, stat.id), value: formatStatValue(values.get(stat.id) ?? null, statFormat(stat.id), lang) }));
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `match` is derived from `q`.
  }, [q, view.calc, lang]);

  const skills = session.socketGroups.map((group, index) => {
    const effect = catalog.size ? activeSkillEffects(group, catalog)[(group.main_active_skill ?? 1) - 1] : undefined;
    return { index, label: `${index + 1}. ${effect ? skillDisplayName(effect.skillId, lang, catalog) : '—'}` };
  }).filter(skill => match(skill.label));
  const pages = [...SHORTCUT_ROUTES, 'build' as Route].map((route, index) => ({ route, label: pageLabel(route), key: index < SHORTCUT_ROUTES.length ? String(index + 1) : '' })).filter(page => match(page.label));
  const tiers = matchingTiers(query, lang);
  const actions = [
    { id: 'undo', label: w('Undo the last change', '撤销上一步修改', '復原上一步修改'), run: () => { if (session.canUndo) session.undoEdit(); } },
    { id: 'redo', label: w('Redo', '重做', '重做'), run: () => { if (session.canRedo) session.redoEdit(); } },
    { id: 'import', label: w('Import or manage builds', '导入或管理 BD', '匯入或管理 BD'), run: () => onNavigate('build') },
    { id: 'theme', label: themeLabel, run: onToggleTheme },
    { id: 'settings', label: w('Display settings', '显示设置', '顯示設定'), run: onSettings },
  ].filter(action => match(action.label));

  const item = (key: string, label: string, hint: string | undefined, action: () => void) =>
    <Command.Item key={key} value={key} onSelect={run(action)}><span>{label}</span>{hint ? <kbd>{hint}</kbd> : null}</Command.Item>;
  return <Dialog open={open} onOpenChange={onOpenChange}>
    <DialogContent showCloseButton={false} className="shell-palette" aria-describedby="shell-palette-hint">
      <DialogTitle className="sr-only">{w('Search and commands', '搜索与命令', '搜尋與命令')}</DialogTitle>
      <DialogDescription id="shell-palette-hint" className="sr-only">{w('Type to find a page, a stat or an action.', '输入以查找页面、数值或操作。', '輸入以尋找頁面、數值或操作。')}</DialogDescription>
      <Command shouldFilter={false} loop label={w('Search and commands', '搜索与命令', '搜尋與命令')}>
        <Command.Input autoFocus value={query} onValueChange={setQuery} placeholder={w('Search pages, stats, skills, actions…', '搜索页面、数值、技能、操作…', '搜尋頁面、數值、技能、操作…')} />
        <Command.List>
          <Command.Empty>{w('Nothing matches.', '没有匹配项。', '沒有符合的項目。')}</Command.Empty>
          {pages.length > 0 && <Command.Group heading={w('Pages', '页面', '頁面')}>{pages.map(page => item(`page:${page.route}`, page.label, page.key, () => onNavigate(page.route)))}</Command.Group>}
          {stats.length > 0 && <Command.Group heading={w('Why this number', '这个数从哪来', '這個數從哪來')}>{stats.map(stat => item(`stat:${stat.id}`, stat.label, stat.value, () => onInspect(stat.id)))}</Command.Group>}
          {skills.length > 0 && <Command.Group heading={w('Main skill', '主技能', '主技能')}>{skills.map(skill => item(`skill:${skill.index}`, skill.label, undefined, () => session.updateParams({ main_socket_group: skill.index })))}</Command.Group>}
          {tiers.length > 0 && <Command.Group heading={w('Enemy', '敌人', '敵人')}>{tiers.map(entry => item(`tier:${entry.tier}`, entry.label, undefined, () => session.updateParams({ enemy_tier: entry.tier })))}</Command.Group>}
          {actions.length > 0 && <Command.Group heading={w('Actions', '操作', '操作')}>{actions.map(action => item(`action:${action.id}`, action.label, undefined, action.run))}</Command.Group>}
        </Command.List>
        <footer className="palette-foot"><span><kbd>↑</kbd><kbd>↓</kbd>{w('Move', '移动', '移動')}</span><span><kbd><CornerDownLeft /></kbd>{w('Select', '选择', '選擇')}</span><span><kbd>Esc</kbd>{w('Close', '关闭', '關閉')}</span></footer>
      </Command>
    </DialogContent>
  </Dialog>;
}

/** `?`: every shortcut on one screen. */
export function ShortcutsDialog({ open, onOpenChange, lang, pageLabel }: { open: boolean; onOpenChange: (open: boolean) => void; lang: Lang; pageLabel: (route: Route) => string }) {
  const w = (en: string, cn: string, tw?: string) => words(lang, en, cn, tw);
  const rows: [string, string][] = [
    ['⌘ K / Ctrl K', w('Search pages, stats and actions', '搜索页面、数值与操作', '搜尋頁面、數值與操作')],
    ...SHORTCUT_ROUTES.map((route, index): [string, string] => [String(index + 1), pageLabel(route)]),
    ['[', w('Hide or show the results panel', '收起或展开结果栏', '收起或展開結果欄')],
    ['Esc', w('Close the calculation panel', '关闭计算面板', '關閉計算面板')],
    ['?', w('This list', '本列表', '本列表')],
  ];
  return <Dialog open={open} onOpenChange={onOpenChange}>
    <DialogContent className="shell-shortcuts">
      <DialogTitle>{w('Keyboard shortcuts', '键盘快捷键', '鍵盤快速鍵')}</DialogTitle>
      <DialogDescription className="sr-only">{w('Shortcuts work when no text field is focused.', '未聚焦输入框时可用。', '未聚焦輸入框時可用。')}</DialogDescription>
      <dl>{rows.map(([keys, label]) => <div key={keys + label}><dt><kbd>{keys}</kbd></dt><dd>{label}</dd></div>)}</dl>
    </DialogContent>
  </Dialog>;
}
