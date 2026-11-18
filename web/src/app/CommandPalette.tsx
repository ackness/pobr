import { useEffect, useMemo, useState } from 'react';
import { Command } from 'cmdk';
import { CornerDownLeft } from 'lucide-react';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '../components/ui/dialog';
import type { BuildSession } from '../hooks/useBuildSession';
import { useSkillCatalog } from '../hooks/useSkillName';
import { bindT, type Lang } from '../lib/i18n';
import { activeSkillEffects } from '../lib/skillSelection';
import { skillDisplayName } from '../lib/skillNames';
import { paletteStats } from './paletteStats';
import type { SettledCalc } from './motion/useSettledCalc';
import type { Route } from './routes';
import { matchingTiers } from './paletteTiers';
import { SHORTCUT_ROUTES } from './shortcuts';

export interface PaletteProps {
  open: boolean; onOpenChange: (open: boolean) => void; session: BuildSession; view: SettledCalc; lang: Lang;
  pageLabel: (route: Route) => string; onNavigate: (route: Route) => void; onInspect: (id: string) => void;
  onOpenImport: () => void; onSettings: () => void; onToggleTheme: () => void; themeLabel: string;
}

/** ⌘K: jump to a page, open the calculation behind any stat, switch the main skill or the enemy, or run an action. */
export function CommandPalette({ open, onOpenChange, session, view, lang, pageLabel, onNavigate, onInspect, onOpenImport, onSettings, onToggleTheme, themeLabel }: PaletteProps) {
  const tt = bindT(lang);
  const catalog = useSkillCatalog();
  const [query, setQuery] = useState('');
  useEffect(() => { if (open) setQuery(''); }, [open]);
  const q = query.trim().toLowerCase();
  const match = (...texts: string[]) => !q || texts.some(text => text.toLowerCase().includes(q));
  const run = (action: () => void) => () => { onOpenChange(false); action(); };

  const stats = useMemo(() => paletteStats(view.calc, q, lang), [q, view.calc, lang]);

  const seenSkills = new Set<string>();
  const skills = session.socketGroups.map((group, index) => {
    const effect = activeSkillEffects(group, catalog)[(group.main_active_skill ?? 1) - 1];
    const identity = effect?.skillId ?? JSON.stringify(group.gems);
    return { identity, index, enabled: group.enabled, label: `${index + 1}. ${effect ? skillDisplayName(effect.skillId, lang, catalog) : '—'}` };
  }).sort((a, b) => Number(b.enabled) - Number(a.enabled)).filter(skill => { if (seenSkills.has(skill.identity)) return false; seenSkills.add(skill.identity); return match(skill.label); });
  const pages = [...SHORTCUT_ROUTES, 'build' as Route].map((route, index) => ({ route, label: pageLabel(route), key: index < SHORTCUT_ROUTES.length ? String(index + 1) : '' })).filter(page => match(page.label));
  const tiers = matchingTiers(query, lang);
  const actions = [
    { id: 'undo', label: tt('palette.undo'), run: () => { if (session.canUndo) session.undoEdit(); } },
    { id: 'redo', label: tt('shell.redo'), run: () => { if (session.canRedo) session.redoEdit(); } },
    { id: 'import', label: tt('palette.importAction'), run: onOpenImport },
    { id: 'theme', label: themeLabel, run: onToggleTheme },
    { id: 'settings', label: tt('settings.title'), run: onSettings },
  ].filter(action => match(action.label));

  const item = (key: string, label: string, hint: string | undefined, action: () => void) =>
    <Command.Item key={key} value={key} onSelect={run(action)}><span>{label}</span>{hint ? <kbd>{hint}</kbd> : null}</Command.Item>;
  return <Dialog open={open} onOpenChange={onOpenChange}>
    <DialogContent showCloseButton={false} className="shell-palette" aria-describedby="shell-palette-hint">
      <DialogTitle className="sr-only">{tt('palette.title')}</DialogTitle>
      <DialogDescription id="shell-palette-hint" className="sr-only">{tt('palette.description')}</DialogDescription>
      <Command shouldFilter={false} loop label={tt('palette.title')}>
        <Command.Input autoFocus value={query} onValueChange={setQuery} placeholder={tt('palette.placeholder')} />
        <Command.List>
          <Command.Empty>{tt('palette.empty')}</Command.Empty>
          {pages.length > 0 && <Command.Group heading={tt('palette.pages')}>{pages.map(page => item(`page:${page.route}`, page.label, page.key, () => onNavigate(page.route)))}</Command.Group>}
          {stats.length > 0 && <Command.Group heading={tt('palette.values')}>{stats.map(stat => item(`stat:${stat.id}`, stat.label, stat.value, () => onInspect(stat.id)))}</Command.Group>}
          {skills.length > 0 && <Command.Group heading={tt('sidebar.mainSkill')}>{skills.map(skill => item(`skill:${skill.index}`, skill.label, undefined, () => session.updateParams({ main_socket_group: skill.index })))}</Command.Group>}
          {tiers.length > 0 && <Command.Group heading={tt('compare.enemy')}>{tiers.map(entry => item(`tier:${entry.tier}`, entry.label, undefined, () => session.updateParams({ enemy_tier: entry.tier })))}</Command.Group>}
          {actions.length > 0 && <Command.Group heading={tt('palette.actions')}>{actions.map(action => item(`action:${action.id}`, action.label, undefined, action.run))}</Command.Group>}
        </Command.List>
        <footer className="palette-foot"><span><kbd>↑</kbd><kbd>↓</kbd>{tt('palette.move')}</span><span><kbd><CornerDownLeft /></kbd>{tt('palette.select')}</span><span><kbd>Esc</kbd>{tt('common.close')}</span></footer>
      </Command>
    </DialogContent>
  </Dialog>;
}

/** `?`: every shortcut on one screen. */
export function ShortcutsDialog({ open, onOpenChange, lang, pageLabel }: { open: boolean; onOpenChange: (open: boolean) => void; lang: Lang; pageLabel: (route: Route) => string }) {
  const tt = bindT(lang);
  const rows: [string, string][] = [
    ['⌘ K / Ctrl K', tt('palette.searchShortcut')],
    ...SHORTCUT_ROUTES.map((route, index): [string, string] => [String(index + 1), pageLabel(route)]),
    ['[', tt('palette.railShortcut')],
    ['Esc', tt('palette.inspectorShortcut')],
    ['?', tt('palette.helpShortcut')],
  ];
  return <Dialog open={open} onOpenChange={onOpenChange}>
    <DialogContent className="shell-shortcuts">
      <DialogTitle>{tt('palette.shortcutsTitle')}</DialogTitle>
      <DialogDescription className="sr-only">{tt('palette.shortcutsHint')}</DialogDescription>
      <dl>{rows.map(([keys, label]) => <div key={keys + label}><dt><kbd>{keys}</kbd></dt><dd>{label}</dd></div>)}</dl>
    </DialogContent>
  </Dialog>;
}
