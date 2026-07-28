import { useEffect, useMemo, useRef, useState } from 'react';
import type { GemCatalogEntry } from '../../api/types';
import { bindT, type Lang } from '../../lib/i18n';
import { gemTagLabels, gemTagMatches } from '../../lib/gemTags';

/** 宝石颜色 → 语义 CSS 变量（tokens.css）。 */
const COLOUR_VAR: Record<string, string> = {
  str: 'color-life',
  dex: 'color-positive',
  int: 'color-mana',
};

type ColourFilter = 'all' | 'str' | 'dex' | 'int';

interface Props {
  entries: GemCatalogEntry[];
  placeholder: string;
  disabled: boolean;
  lang: Lang;
  onPick: (skillId: string) => void;
}

/** 宝石显示名：简中界面优先简中名、繁中界面优先繁中名，逐级回退到英文。 */
export function gemDisplayName(entry: GemCatalogEntry, lang: Lang): string {
  if (lang === 'zh-CN') return entry.name_zh_cn ?? entry.name_zh_tw ?? entry.name;
  if (lang === 'zh-TW') return entry.name_zh_tw ?? entry.name_zh_cn ?? entry.name;
  return entry.name;
}

/**
 * 自定义宝石选择器：应用同风格下拉面板（非系统 datalist），
 * 中英文子串搜索 + 颜色分类筛选 + 键盘导航（↑↓/Enter/Esc）。
 */
export function GemPicker({ entries, placeholder, disabled, lang, onPick }: Props) {
  const tt = bindT(lang);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [colour, setColour] = useState<ColourFilter>('all');
  const [highlight, setHighlight] = useState(0);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const listRef = useRef<HTMLUListElement | null>(null);

  // 点击组件外关闭。
  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('pointerdown', onDown);
    return () => document.removeEventListener('pointerdown', onDown);
  }, [open]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return entries
      .filter((e) => colour === 'all' || e.colour === colour)
      .filter(
        (e) =>
          q === '' ||
          e.name.toLowerCase().includes(q) ||
          (e.name_zh_tw ?? '').includes(query.trim()) ||
          (e.name_zh_cn ?? '').includes(query.trim()) ||
          // 标签也参与搜索：输入 "projectile" / "投射物" 可筛出该类技能。
          gemTagMatches(e.tags, query.trim()),
      )
      .slice(0, 200);
  }, [entries, query, colour]);

  useEffect(() => setHighlight(0), [query, colour, open]);

  const pick = (entry: GemCatalogEntry) => {
    onPick(entry.skill_id);
    setQuery('');
    setOpen(false);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (!open) return;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const next = e.key === 'ArrowDown' ? highlight + 1 : highlight - 1;
      const clamped = Math.max(0, Math.min(filtered.length - 1, next));
      setHighlight(clamped);
      listRef.current?.children[clamped]?.scrollIntoView({ block: 'nearest' });
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (filtered[highlight]) pick(filtered[highlight]);
    } else if (e.key === 'Escape') {
      setOpen(false);
    }
  };

  const chips: { id: ColourFilter; label: string; colorVar?: string }[] = [
    { id: 'all', label: tt('picker.all') },
    { id: 'str', label: 'STR', colorVar: COLOUR_VAR.str },
    { id: 'dex', label: 'DEX', colorVar: COLOUR_VAR.dex },
    { id: 'int', label: 'INT', colorVar: COLOUR_VAR.int },
  ];

  return (
    <div className="gem-picker" ref={rootRef} onKeyDown={onKeyDown}>
      <input
        value={query}
        placeholder={placeholder}
        disabled={disabled}
        aria-label={placeholder}
        aria-expanded={open}
        role="combobox"
        onFocus={() => setOpen(true)}
        onChange={(e) => {
          setQuery(e.target.value);
          setOpen(true);
        }}
      />
      {open && (
        <div className="gem-picker-panel">
          <div className="gem-picker-chips" role="group" aria-label="Gem colour filter">
            {chips.map((chip) => (
              <button
                key={chip.id}
                className={`gem-chip${colour === chip.id ? ' is-active' : ''}`}
                style={chip.colorVar ? { color: `var(--${chip.colorVar})` } : undefined}
                onClick={() => setColour(chip.id)}
              >
                {chip.label}
              </button>
            ))}
            <span className="gem-picker-count">{filtered.length}</span>
          </div>
          <ul className="gem-picker-list" role="listbox" ref={listRef}>
            {filtered.length === 0 && <li className="gem-picker-empty">{tt('picker.noResults')}</li>}
            {filtered.map((entry, idx) => {
              const primary = gemDisplayName(entry, lang);
              const secondary = primary === entry.name ? entry.name_zh_tw : entry.name;
              // 限 3 个：列表项是单行 flex，再多会挤掉宝石名。
              const tags = gemTagLabels(entry.tags, lang, 3);
              return (
                <li
                  key={entry.skill_id}
                  role="option"
                  aria-selected={idx === highlight}
                  className={`gem-picker-item${idx === highlight ? ' is-highlight' : ''}`}
                  onPointerEnter={() => setHighlight(idx)}
                  onClick={() => pick(entry)}
                >
                  <span
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
        </div>
      )}
    </div>
  );
}
