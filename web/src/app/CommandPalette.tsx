import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from 'react';
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

interface PaletteEntry { key: string; label: string; hint?: string; action: () => void }
interface PaletteGroup { id: string; heading: string; items: PaletteEntry[] }

/** ⌘K: jump to a page, open the calculation behind any stat, switch the main skill or the enemy, or run an action. */
export function CommandPalette({ open, onOpenChange, session, view, lang, pageLabel, onNavigate, onInspect, onOpenImport, onSettings, onToggleTheme, themeLabel }: PaletteProps) {
  const tt = bindT(lang);
  const catalog = useSkillCatalog();
  const baseId = useId();
  const listRef = useRef<HTMLDivElement>(null);
  const [query, setQuery] = useState('');
  const [activeIndex, setActiveIndex] = useState(0);
  const [prevOpen, setPrevOpen] = useState(open);
  if (open !== prevOpen) {
    setPrevOpen(open);
    if (open) { setQuery(''); setActiveIndex(0); }
  }
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

  const groups: PaletteGroup[] = [
    { id: 'pages', heading: tt('palette.pages'), items: pages.map(page => ({ key: `page:${page.route}`, label: page.label, hint: page.key || undefined, action: () => onNavigate(page.route) })) },
    { id: 'values', heading: tt('palette.values'), items: stats.map(stat => ({ key: `stat:${stat.id}`, label: stat.label, hint: stat.value, action: () => onInspect(stat.id) })) },
    { id: 'skills', heading: tt('sidebar.mainSkill'), items: skills.map(skill => ({ key: `skill:${skill.index}`, label: skill.label, action: () => session.updateParams({ main_socket_group: skill.index }) })) },
    { id: 'tiers', heading: tt('compare.enemy'), items: tiers.map(entry => ({ key: `tier:${entry.tier}`, label: entry.label, action: () => session.updateParams({ enemy_tier: entry.tier }) })) },
    { id: 'actions', heading: tt('palette.actions'), items: actions.map(action => ({ key: `action:${action.id}`, label: action.label, action: action.run })) },
  ].filter(group => group.items.length > 0);

  const entries = groups.flatMap(group => group.items);
  const selectedIndex = entries.length ? Math.min(activeIndex, entries.length - 1) : -1;
  const activeEntry = selectedIndex >= 0 ? entries[selectedIndex] : undefined;
  const optionId = (index: number) => `${baseId}-option-${index}`;
  const listId = `${baseId}-list`;

  useEffect(() => {
    if (!open || selectedIndex < 0) return;
    listRef.current?.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView?.({ block: 'nearest' });
  }, [open, selectedIndex]);

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === 'Escape') { event.preventDefault(); onOpenChange(false); return; }
    if (!entries.length) return;
    if (event.key === 'ArrowDown') { event.preventDefault(); setActiveIndex(i => (i + 1) % entries.length); }
    else if (event.key === 'ArrowUp') { event.preventDefault(); setActiveIndex(i => (i - 1 + entries.length) % entries.length); }
    else if (event.key === 'Home') { event.preventDefault(); setActiveIndex(0); }
    else if (event.key === 'End') { event.preventDefault(); setActiveIndex(entries.length - 1); }
    else if (event.key === 'Enter' && !event.nativeEvent.isComposing && activeEntry) {
      event.preventDefault();
      run(activeEntry.action)();
    }
  };

  let offset = 0;
  return <Dialog open={open} onOpenChange={onOpenChange}>
    <DialogContent showCloseButton={false} className="shell-palette" aria-describedby="shell-palette-hint">
      <DialogTitle className="sr-only">{tt('palette.title')}</DialogTitle>
      <DialogDescription id="shell-palette-hint" className="sr-only">{tt('palette.description')}</DialogDescription>
      <div className="palette-root" onKeyDown={onKeyDown}>
        <input
          className="palette-input"
          type="text"
          role="combobox"
          aria-expanded="true"
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={selectedIndex >= 0 ? optionId(selectedIndex) : undefined}
          aria-label={tt('palette.title')}
          autoFocus
          value={query}
          onChange={event => { setQuery(event.target.value); setActiveIndex(0); }}
          placeholder={tt('palette.placeholder')}
        />
        <div ref={listRef} id={listId} className="palette-list" role="listbox" aria-label={tt('palette.title')}>
          {entries.length === 0 ? <div className="palette-empty">{tt('palette.empty')}</div> : groups.map(group => {
            const headingId = `${baseId}-group-${group.id}`;
            const start = offset;
            offset += group.items.length;
            return <div key={group.id} role="group" aria-labelledby={headingId}>
              <div id={headingId} className="palette-group-heading">{group.heading}</div>
              {group.items.map((entry, idx) => {
                const index = start + idx;
                const selected = index === selectedIndex;
                return <div
                  key={entry.key}
                  id={optionId(index)}
                  role="option"
                  aria-selected={selected}
                  data-selected={selected ? 'true' : undefined}
                  data-command={entry.key}
                  className="palette-item"
                  onMouseMove={() => setActiveIndex(index)}
                  onClick={run(entry.action)}
                ><span>{entry.label}</span>{entry.hint ? <kbd>{entry.hint}</kbd> : null}</div>;
              })}
            </div>;
          })}
        </div>
        <footer className="palette-foot"><span><kbd>↑</kbd><kbd>↓</kbd>{tt('palette.move')}</span><span><kbd><CornerDownLeft /></kbd>{tt('palette.select')}</span><span><kbd>Esc</kbd>{tt('common.close')}</span></footer>
      </div>
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
