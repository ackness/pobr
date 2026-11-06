import { Popover, PopoverAnchor, PopoverContent } from '../ui/popover';
import { usePanelActive } from '../shared/UiContext';
import { AppButton, AppInput } from '../shared/controls';
import { useEffect, useId, useMemo, useRef, useState } from 'react';
import type { GemCatalogEntry } from '../../api/types';
import { bindT, type Lang } from '../../lib/i18n';
import { gemTagLabels, gemTagMatches } from '../../lib/gemTags';
import { normalizeGemName, gemDisplayName } from '../../lib/skillNames';
export { gemDisplayName } from '../../lib/skillNames';

/** 宝石颜色 → 语义 CSS 变量（tokens.css）。 */
const COLOUR_VAR: Record<string, string> = {
  str: 'color-life',
  dex: 'color-positive',
  int: 'color-mana',
};

export type ColourFilter = 'all' | 'str' | 'dex' | 'int';

interface Props {
  entries: GemCatalogEntry[];
  placeholder: string;
  disabled: boolean;
  lang: Lang;
  onPick: (skillId: string) => void;
}

/**
 * 自定义宝石选择器：应用同风格下拉面板（非系统 datalist），
 * 中英文子串搜索 + 颜色分类筛选 + 键盘导航（↑↓/Enter/Esc）。
 */
export function rankGemResults(entries: GemCatalogEntry[], query: string, colour: ColourFilter, lang: Lang): GemCatalogEntry[] {
  const searchText = (text: string) => normalizeGemName(text).toLocaleLowerCase(lang).replace(/\s/g, '');
  const q = searchText(query.trim());
  const rank = (gem: GemCatalogEntry) => {
    const chineseNames = [gem.name_zh_cn, gem.name_zh_tw].filter((name): name is string => Boolean(name)).map(searchText);
    if (!q || chineseNames.some(name => name.startsWith(q))) return 0;
    if (chineseNames.some(name => name.includes(q))) return 1;
    if (searchText(gem.name).includes(q)) return 2;
    return gemTagMatches(gem.tags, query.trim()) ? 3 : 4;
  };
  return [...new Map(entries.map(gem => [gem.skill_id, gem])).values()]
    // The catalog has no socketability field or base ID. SkillGemUnique* bases
    // expose Unique* effect IDs. Hide them only here; imports still use the full catalog.
    .filter(gem => !gem.skill_id.startsWith('Unique'))
    .filter(gem => (colour === 'all' || gem.colour === colour) && rank(gem) < 4)
    .sort((a, b) => rank(a) - rank(b) || gemDisplayName(a, lang).localeCompare(gemDisplayName(b, lang), lang) || a.skill_id.localeCompare(b.skill_id));
}

export function GemPicker({ entries, placeholder, disabled, lang, onPick }: Props) {
  const tt = bindT(lang);
  const active = usePanelActive();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [colour, setColour] = useState<ColourFilter>('all');
  const [highlight, setHighlight] = useState(0);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const listRef = useRef<HTMLUListElement | null>(null);
  const panelRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const listId = useId();

  // 点击组件外关闭。
  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node) && !panelRef.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('pointerdown', onDown);
    return () => document.removeEventListener('pointerdown', onDown);
  }, [open]);

  const filtered = useMemo(() => rankGemResults(entries, query, colour, lang), [entries, query, colour, lang]);

  useEffect(() => setHighlight(0), [query, colour, open, entries]);
  useEffect(() => { if (disabled || !active) setOpen(false); }, [disabled, active]);

  const pick = (entry: GemCatalogEntry) => {
    inputRef.current?.focus({ preventScroll: true });
    onPick(entry.skill_id);
    setQuery('');
    setOpen(false);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (disabled || e.nativeEvent.isComposing || e.nativeEvent.keyCode === 229) return;
    if (open && e.key === 'Tab' && !e.shiftKey) {
      // The filters are drawn in the popover, after everything else in the document: lead the focus there.
      const first = panelRef.current?.querySelector<HTMLElement>('button');
      if (first) { e.preventDefault(); first.focus(); }
      return;
    }
    if (!open) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        setOpen(true);
      }
      return;
    }
    if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) {
      e.preventDefault();
      const next = e.key === 'Home' ? 0 : e.key === 'End' ? filtered.length - 1 : e.key === 'ArrowDown' ? highlight + 1 : highlight - 1;
      const clamped = Math.max(0, Math.min(filtered.length - 1, next));
      setHighlight(clamped);
      listRef.current?.children[clamped]?.scrollIntoView({ block: 'nearest' });
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (filtered[highlight]) pick(filtered[highlight]);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      setOpen(false);
    }
  };

  /** Tab walks the filters and then continues from the field, as if they were drawn right after it. */
  const onPanelKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== 'Tab') return;
    const buttons = [...e.currentTarget.querySelectorAll<HTMLElement>('button')];
    if (e.shiftKey && e.target === buttons[0]) { e.preventDefault(); inputRef.current?.focus(); }
    // Focus moved to the field here, so the browser's own Tab step carries on from the field to what follows it.
    else if (!e.shiftKey && e.target === buttons.at(-1)) inputRef.current?.focus();
  };

  const chips: { id: ColourFilter; label: string; colorVar?: string }[] = [
    { id: 'all', label: tt('picker.all') },
    { id: 'str', label: tt('tree.attr.str'), colorVar: COLOUR_VAR.str },
    { id: 'dex', label: tt('tree.attr.dex'), colorVar: COLOUR_VAR.dex },
    { id: 'int', label: tt('tree.attr.int'), colorVar: COLOUR_VAR.int },
  ];

  const panelContents = <>
          <div className="gem-picker-chips" role="group" aria-label={tt('picker.colour')}>
            <div className="segmented segmented--sm">{chips.map((chip) => (
              <AppButton
                key={chip.id}
                type="button"
                aria-pressed={colour === chip.id}
                className={`gem-chip${colour === chip.id ? ' is-active' : ''}`}
                style={chip.colorVar ? { color: `var(--${chip.colorVar})` } : undefined}
                onClick={() => setColour(chip.id)}
              >
                {chip.label}
              </AppButton>
            ))}</div>
            <span className="gem-picker-count">{tt('picker.resultCount').replace('{count}', String(filtered.length))}</span>
          </div>
          {filtered.length === 0 && <p className="gem-picker-empty" role="status">{tt('picker.noResults')}</p>}
          <ul className="gem-picker-list" role="listbox" ref={listRef} id={listId} aria-label={placeholder}>
            {filtered.map((entry, idx) => {
              const primary = gemDisplayName(entry, lang);
              const secondary = primary === entry.name
                ? (lang === 'en-US' && entry.name_zh_tw !== entry.name ? entry.name_zh_tw : null) : entry.name;
              // 限 3 个：列表项是单行 flex，再多会挤掉宝石名。
              const tags = gemTagLabels(entry.tags, lang, 3);
              return (
                <li
                  key={entry.skill_id}
                  id={`${listId}-${idx}`}
                  role="option"
                  aria-selected={idx === highlight}
                  className={`gem-picker-item${idx === highlight ? ' is-highlight' : ''}`}
                  onPointerEnter={() => setHighlight(idx)}
                  onMouseDown={event => event.preventDefault()}
                  onClick={() => pick(entry)}
                >
                  <span
                    role="img" aria-label={entry.colour && ['str', 'dex', 'int'].includes(entry.colour) ? tt(`tree.attr.${entry.colour}`) : tt('picker.colour')}
                    className="gem-dot"
                    style={{
                      background: entry.colour ? `var(--${COLOUR_VAR[entry.colour]})` : 'var(--text-muted)',
                    }}
                  />
                  <span className="gem-primary">{primary}</span>
                  {entry.is_lineage && (
                    <span className="gem-lineage-badge">{tt('picker.lineage')}</span>
                  )}
                  {secondary && <span className="gem-secondary">{secondary}</span>}
                  {tags.length > 0 && (
                    <span className="gem-tags">
                      {tags.map((tag) => (
                        <span key={tag} className="gem-tag">
                          {tag}
                        </span>
                      ))}
                    </span>
                  )}
                </li>
              );
            })}
          </ul>
  </>;

  return (
    <div className="gem-picker" ref={rootRef}
      onBlur={event => {
        if (!event.currentTarget.contains(event.relatedTarget) && !panelRef.current?.contains(event.relatedTarget)) setOpen(false);
      }}
      onKeyDown={event => { if (event.key === 'Escape' && !event.nativeEvent.isComposing) setOpen(false); }}>
      <AppInput
        ref={inputRef}
        value={query}
        placeholder={placeholder}
        disabled={disabled}
        aria-label={placeholder}
        aria-expanded={open && !disabled}
        aria-controls={open && !disabled ? listId : undefined}
        aria-activedescendant={open && !disabled && filtered[highlight] ? `${listId}-${highlight}` : undefined}
        aria-autocomplete="list"
        role="combobox"
        onKeyDown={onKeyDown}
        onFocus={() => setOpen(true)}
        onChange={(e) => {
          setQuery(e.target.value);
          setOpen(true);
        }}
      />
      <Popover open={open && !disabled && active} onOpenChange={setOpen}>
        <PopoverAnchor virtualRef={inputRef} />
        <PopoverContent ref={panelRef} className="gem-picker-popover" align="start"
          onKeyDown={onPanelKeyDown}
          onOpenAutoFocus={event => event.preventDefault()}
          onCloseAutoFocus={event => event.preventDefault()}
          onInteractOutside={event => { if (rootRef.current?.contains(event.target as Node)) event.preventDefault(); }}>
          {panelContents}
        </PopoverContent>
      </Popover>
    </div>
  );
}
