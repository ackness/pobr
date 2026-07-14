import { Fragment, useEffect, useMemo, useState, type ReactNode } from 'react';
import { getBackend } from '../../api/backend';
import type { ItemLineJson, RuneCatalogEntry } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import {
  cleanLine,
  itemLines,
  rarityOf,
  useItemDisplayNames,
  useLocalizedLines,
} from '../../hooks/useLocalizedLines';
import { bindT, slotLabel, type Lang } from '../../lib/i18n';
import { previewDiff, type DiffEntry } from '../../lib/compare';
import { DiffList } from '../shared/DiffList';
import { CopyButton } from '../shared/CopyButton';
import { NoteEditor } from '../shared/NoteEditor';
import { AppSelect, type AppSelectOption } from '../shared/AppSelect';
import { ItemOptimizer } from './ItemOptimizer';
import './items.css';

interface Props {
  session: BuildSession;
  lang: Lang;
}

// 行提取/翻译工具已收口在 hooks/useLocalizedLines（树页珠宝选择器共用）。

/** 词条系类别（与 explicit 之间画分隔线的左侧块）。 */
const IMPLICIT_KINDS = ['implicit', 'enchant', 'rune', 'class_req'];

/** 物品文本 → 结构化展示行（后端按桶分类；空/异常返回 null 走无区分渲染）。 */
function useClassifiedLines(text: string): ItemLineJson[] | null {
  const [lines, setLines] = useState<ItemLineJson[] | null>(null);
  useEffect(() => {
    let cancelled = false;
    setLines(null);
    getBackend()
      .then((b) => b.classifyItemLines(text))
      .then((out) => {
        if (!cancelled) setLines(out.length > 0 ? out : null);
      })
      .catch(() => {
        if (!cancelled) setLines(null);
      });
    return () => {
      cancelled = true;
    };
  }, [text]);
  return lines;
}

function ItemText({ text, lang }: { text: string; lang: Lang }) {
  const classified = useClassifiedLines(text);
  // 回落：后端未分类（mock / 旧缓存 / 异常）时，沿用原无区分拆行（首行=名，其余=普通行）。
  const fallback = useMemo<ItemLineJson[]>(() => {
    const [name, ...rest] = itemLines(text);
    return [
      ...(name ? [{ text: name, kind: 'name' as const }] : []),
      ...rest.map((t) => ({ text: t, kind: 'struct' as const })),
    ];
  }, [text]);
  const all = classified ?? fallback;
  const nameIdx = all.findIndex((l) => l.kind === 'name');
  // 名称行（唯一物品名/基底名）一并送翻译——命中名词直译表才能出中文名。
  const allTranslated = useLocalizedLines(
    all.map((l) => l.text),
    lang,
  );
  const name = nameIdx >= 0 ? allTranslated[nameIdx] : '';
  // 唯一 ID 行是纯技术标识（对比/往返用），展示视图默认隐藏；编辑态原文仍保留。
  const body = all
    .map((line, i) => ({ ...line, text: allTranslated[i] }))
    .filter((_, i) => i !== nameIdx && !/^Unique ID:/i.test(all[i].text));
  const translated = body.map((l) => l.text);
  // implicit 系词条块与 explicit 块之间插一条分隔线（PoB2/游戏内惯例）。
  const firstExplicit = body.findIndex((l) => l.kind === 'explicit');
  const dividerAt =
    firstExplicit > 0 && body.some((l) => IMPLICIT_KINDS.includes(l.kind)) ? firstExplicit : -1;
  return (
    <>
      <h4 className="item-name">{name}</h4>
      <div className="item-lines">
        {body.map((line, i) => (
          <Fragment key={i}>
            {i === dividerAt && <div className="item-line-sep" role="separator" />}
            <div className={`item-line item-line--${line.kind}`}>
              {translated[i]}
              {line.tier != null && (
                <span
                  className={`item-tier item-tier--${line.tier === 1 ? 'top' : 'rest'}`}
                  title={`${line.affix === 'prefix' ? '前缀' : '后缀'} · 该基底共 ${line.tier_total} 档`}
                >
                  T{line.tier}
                </span>
              )}
            </div>
          </Fragment>
        ))}
      </div>
    </>
  );
}

/** 紧凑物品行（物品库/珠宝列表共用）：一行名称+槽位+操作，点击行头展开完整词条。 */
function ItemRow({
  text,
  name,
  lang,
  slotTag,
  open,
  onToggle,
  actions,
  children,
}: {
  text: string;
  name: string;
  lang: Lang;
  slotTag?: string;
  open: boolean;
  onToggle: () => void;
  actions?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <article className={`library-row rarity-${rarityOf(text)}`}>
      <header className="library-row-head">
        <button className="library-row-toggle" aria-expanded={open} onClick={onToggle}>
          <span className="row-caret" aria-hidden>
            ▸
          </span>
          <span className="item-name">{name}</span>
          {slotTag && <span className="library-row-slot">{slotTag}</span>}
        </button>
        <span className="item-actions">{actions}</span>
      </header>
      {open && (
        <div className="library-row-body">
          <ItemText text={text} lang={lang} />
          {children}
        </div>
      )}
    </article>
  );
}

/** 人形布局的槽位 → grid-area（与 items.css 的 template areas 对应）。 */
const DOLL_SLOTS: { slot: string; area: string }[] = [
  { slot: 'weapon1', area: 'weapon1' },
  { slot: 'helmet', area: 'helmet' },
  { slot: 'amulet', area: 'amulet' },
  { slot: 'weapon2', area: 'weapon2' },
  { slot: 'bodyarmour', area: 'body' },
  { slot: 'ring1', area: 'ring1' },
  { slot: 'ring2', area: 'ring2' },
  { slot: 'gloves', area: 'gloves' },
  { slot: 'belt', area: 'belt' },
  { slot: 'boots', area: 'boots' },
];

const ITEM_TEMPLATE = 'Rarity: RARE\nNew Item\nSapphire Ring\n+50 to maximum Life';
const FLASK_TEMPLATE = 'Rarity: MAGIC\nUltimate Life Flask\nUltimate Life Flask';
const CHARM_TEMPLATE = 'Rarity: MAGIC\nRuby Charm\nRuby Charm';

/** PoB 药剂/护符槽（激活态；与 wasm 契约的 utility 槽名一致）。 */
const UTILITY_SLOTS = ['Flask 1', 'Flask 2', 'Charm 1', 'Charm 2', 'Charm 3'];
const isUtilitySlot = (slot: string) => slot.startsWith('Flask') || slot.startsWith('Charm');

/** 装备页：PoB2 式人形槽位布局；点槽位在下方编辑 PoB 文本，保存即重算。 */
export function ItemsPanel({ session, lang }: Props) {
  const tt = bindT(lang);
  const [selected, setSelected] = useState<string | null>(null);
  const [draft, setDraft] = useState('');
  const [editing, setEditing] = useState(false);
  const build = session.build;
  const items = session.items;
  const bySlot = new Map(items.map((item) => [item.slot, item.text]));
  const utilityBySlot = new Map(session.flasks.map((f) => [f.slot, f.text]));

  const textOf = (slot: string) =>
    isUtilitySlot(slot) ? utilityBySlot.get(slot) : bySlot.get(slot);
  // 槽位按钮上的物品显示名「名称·基底」（一批送翻译；中文界面显示本地化名）。
  const slotNames = useItemDisplayNames(
    [...DOLL_SLOTS.map(({ slot }) => slot), ...UTILITY_SLOTS].map((slot) => textOf(slot)),
    lang,
  );
  const slotName = (index: number) => slotNames[index] || null;
  const templateOf = (slot: string) =>
    slot.startsWith('Charm') ? CHARM_TEMPLATE : slot.startsWith('Flask') ? FLASK_TEMPLATE : ITEM_TEMPLATE;

  const select = (slot: string) => {
    setSelected(slot);
    setDraft(textOf(slot) ?? templateOf(slot));
    setEditing(textOf(slot) === undefined);
  };
  const applyEdit = () => {
    if (!selected) return;
    if (isUtilitySlot(selected)) {
      const rest = session.flasks.filter((f) => f.slot !== selected);
      session.setFlasks([...rest, { slot: selected, text: draft }]);
    } else {
      const rest = items.filter((item) => item.slot !== selected);
      session.setItems([...rest, { slot: selected, text: draft }]);
    }
    setEditing(false);
  };
  const removeItem = () => {
    if (!selected) return;
    if (isUtilitySlot(selected)) {
      session.setFlasks(session.flasks.filter((f) => f.slot !== selected));
    } else {
      session.setItems(items.filter((item) => item.slot !== selected));
    }
    setEditing(false);
  };

  /** 把库中某件物品直接装进当前选中槽位（PoB2 式下拉切换）。 */
  const switchTo = (text: string) => {
    if (!selected) return;
    if (isUtilitySlot(selected)) {
      const rest = session.flasks.filter((f) => f.slot !== selected);
      session.setFlasks([...rest, { slot: selected, text }]);
    } else {
      const rest = items.filter((item) => item.slot !== selected);
      session.setItems([...rest, { slot: selected, text }]);
    }
    setDraft(text);
    setEditing(false);
  };

  const selectedText = selected ? textOf(selected) : undefined;

  /** 槽位 → 注释键（药剂/护符与装备分域，避免槽名撞车）。 */
  const noteKey = (slot: string) => `${isUtilitySlot(slot) ? 'flask' : 'item'}:${slot}`;
  const hasNote = (slot: string) => Boolean(session.annotations[noteKey(slot)]?.trim());

  // 符文目录（按选中物品重取：效果行随基底槽类变化；mock/加载失败为空 → 隐藏符文编辑器）。
  const [runeCatalog, setRuneCatalog] = useState<RuneCatalogEntry[]>([]);
  useEffect(() => {
    let cancelled = false;
    getBackend()
      .then((b) => b.runeCatalog(selectedText))
      .then((entries) => {
        if (!cancelled) setRuneCatalog(entries);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [selectedText]);
  const [runeError, setRuneError] = useState<string | null>(null);
  useEffect(() => setRuneError(null), [selected]);

  // 选中物品的符文位：`Sockets: S S` 容量 + 当前 `Rune:`/`Soul Core:` 命名行。
  const socketInfo = useMemo(() => {
    if (!selectedText) return null;
    const m = selectedText.match(/^Sockets:\s*(.+)$/m);
    if (!m) return null;
    const capacity = m[1].split(/\s+/).filter((t) => t === 'S').length;
    if (capacity === 0) return null;
    const runes = [...selectedText.matchAll(/^(?:Rune|Soul Core):\s*(.+)$/gm)].map((x) =>
      x[1].trim(),
    );
    return { capacity, runes };
  }, [selectedText]);

  const runeDisplayName = (entry: RuneCatalogEntry) =>
    (lang === 'zh-CN' ? entry.name_zh_cn : lang === 'zh-TW' ? entry.name_zh_tw : null) ??
    entry.name;

  // 效果行本地化（flat 展开 → 按条目行数切回；en 直接原样）。
  const runeEffectLines = useLocalizedLines(
    runeCatalog.flatMap((r) => r.lines.map(cleanLine)),
    lang,
  );
  const runeHints = useMemo(() => {
    const map = new Map<string, string>();
    let offset = 0;
    for (const r of runeCatalog) {
      map.set(r.name, runeEffectLines.slice(offset, offset + r.lines.length).join(', '));
      offset += r.lines.length;
    }
    return map;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [runeCatalog, runeEffectLines]);

  // 符文槽下拉选项（各槽共用一份）：空槽 + 符文组 + 魂核组，选项带效果副行。
  // 有物品上下文（任一条目带效果行）时滤掉对该基底不适用的符文。
  const runeOptions = useMemo<AppSelectOption[]>(() => {
    const hasContext = runeCatalog.some((r) => r.lines.length > 0);
    const visible = hasContext ? runeCatalog.filter((r) => r.lines.length > 0) : runeCatalog;
    const toOption = (r: RuneCatalogEntry, group: string): AppSelectOption => ({
      value: r.name,
      label: runeDisplayName(r),
      group,
      hint: runeHints.get(r.name) || undefined,
    });
    return [
      { value: '', label: tt('items.emptySocket') },
      ...visible.filter((r) => !r.is_soul_core).map((r) => toOption(r, tt('items.runeGroup'))),
      ...visible.filter((r) => r.is_soul_core).map((r) => toOption(r, tt('items.soulCoreGroup'))),
    ];
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [runeCatalog, runeHints, lang]);

  /** 调用后端重写符文/孔数并整件重算。 */
  const reforge = async (runes: string[], sockets?: number) => {
    if (!selectedText) return;
    try {
      const backend = await getBackend();
      const text = await backend.reforgeRunes(selectedText, runes, sockets);
      setRuneError(null);
      switchTo(text);
    } catch (err) {
      setRuneError(String(err));
    }
  };

  /** 重插符文（第 i 槽换为 name；空串=拔除该槽）。 */
  const resocketRune = (index: number, name: string) => {
    if (!socketInfo) return;
    const next = [...socketInfo.runes];
    if (name === '') {
      next.splice(index, 1);
    } else {
      next[index] = name;
    }
    void reforge(next.filter(Boolean).slice(0, socketInfo.capacity));
  };

  /** 直接加减孔（不模拟通货）；减孔时裁掉超出的符文。 */
  const setSocketCount = (capacity: number) => {
    const clamped = Math.max(0, Math.min(6, capacity));
    void reforge((socketInfo?.runes ?? []).slice(0, clamped), clamped);
  };

  // 槽位切换候选：只收库中槽位明确匹配（族归一，ring1/2 互通）的装备。
  // 未记录槽位的旧库条目不进候选（防止项链装进主手）；重新导入 build 会回填 slot。
  const candidates = selected
    ? session.library.items.filter(
        (i) => i.kind === 'item' && i.slot != null && slotFamily(i.slot) === slotFamily(selected),
      )
    : [];
  const candidateNames = useItemDisplayNames(
    candidates.map((c) => c.text),
    lang,
  );
  const switcherValue =
    candidates.find((c) => c.text === selectedText)?.id ?? (selectedText ? '__current' : '');

  return (
    <section aria-labelledby="items-heading">
      <h2 id="items-heading" className="panel-heading">
        {tt('items.title')}
      </h2>
      <p className="items-hint">{tt('items.hint')}</p>

      <div className="paper-doll" role="group" aria-label={tt('items.title')}>
        {DOLL_SLOTS.map(({ slot, area }, slotIdx) => {
          const text = bySlot.get(slot);
          const name = text ? slotName(slotIdx) : null;
          return (
            <button
              key={slot}
              className={`doll-slot${text ? ` rarity-${rarityOf(text)}` : ' doll-slot-empty'}${
                selected === slot ? ' is-selected' : ''
              }`}
              style={{ gridArea: area }}
              onClick={() => select(slot)}
              aria-label={slotLabel(lang, slot)}
            >
              <span className="doll-slot-label">
                {slotLabel(lang, slot)}
                {hasNote(slot) && <span className="note-dot" aria-hidden />}
              </span>
              {name ? (
                <span className="doll-item-name item-name">{name}</span>
              ) : (
                <span className="doll-empty">{tt('items.empty')}</span>
              )}
            </button>
          );
        })}
      </div>

      <div className="utility-row" role="group" aria-label={tt('items.flasks')}>
        {UTILITY_SLOTS.map((slot, utilIdx) => {
          const text = utilityBySlot.get(slot);
          const name = text ? slotName(DOLL_SLOTS.length + utilIdx) : null;
          return (
            <button
              key={slot}
              className={`doll-slot${text ? ` rarity-${rarityOf(text)}` : ' doll-slot-empty'}${
                selected === slot ? ' is-selected' : ''
              }`}
              onClick={() => select(slot)}
              aria-label={slot}
            >
              <span className="doll-slot-label">
                {slotLabel(lang, slot)}
                {hasNote(slot) && <span className="note-dot" aria-hidden />}
              </span>
              {name ? (
                <span className="doll-item-name item-name">{name}</span>
              ) : (
                <span className="doll-empty">{tt('items.empty')}</span>
              )}
            </button>
          );
        })}
      </div>

      {selected && (
        <div className={`item-detail${selectedText ? ` rarity-${rarityOf(selectedText)}` : ''}`}>
          <header className="item-detail-header">
            <span className="item-slot">{slotLabel(lang, selected)}</span>
            {candidates.length > 0 && (
              <AppSelect
                ariaLabel={tt('items.switcher')}
                value={switcherValue}
                disabled={session.busy}
                options={[
                  ...(selectedText && !candidates.some((c) => c.text === selectedText)
                    ? [{ value: '__current', label: tt('items.currentItem') }]
                    : []),
                  { value: '', label: tt('items.unequip') },
                  ...candidates.map((c, i) => ({ value: c.id, label: candidateNames[i] || c.name })),
                ]}
                onChange={(v) => {
                  if (v === '__current') return;
                  if (v === '') {
                    removeItem();
                    return;
                  }
                  const entry = candidates.find((c) => c.id === v);
                  if (entry) switchTo(entry.text);
                }}
              />
            )}
            <span className="item-actions">
              {!editing && (
                <button disabled={session.busy} onClick={() => setEditing(true)}>
                  {selectedText ? tt('items.edit') : tt('items.add')}
                </button>
              )}
              {selectedText && (
                <>
                  <CopyButton text={selectedText} lang={lang} />
                  <button
                    disabled={session.busy}
                    onClick={() => session.saveLibraryItem('item', selectedText, selected)}
                  >
                    {tt('lib.save')}
                  </button>
                  <button className="skill-remove" disabled={session.busy} onClick={removeItem}>
                    {tt('items.remove')}
                  </button>
                </>
              )}
            </span>
          </header>
          {editing ? (
            <div className="item-editor">
              <textarea
                rows={10}
                value={draft}
                spellCheck={false}
                aria-label={`${selected} item text`}
                onChange={(e) => setDraft(e.target.value)}
              />
              <div className="item-editor-actions">
                <button disabled={session.busy} onClick={applyEdit}>
                  {tt('items.apply')}
                </button>
                <button onClick={() => setEditing(false)}>{tt('items.cancel')}</button>
              </div>
            </div>
          ) : selectedText ? (
            <>
              {!socketInfo && runeCatalog.length > 0 && !isUtilitySlot(selected) && (
                <div className="rune-editor" role="group" aria-label={tt('items.runes')}>
                  <button disabled={session.busy} onClick={() => setSocketCount(1)}>
                    {tt('items.addSockets')}
                  </button>
                  {runeError && <span className="calc-error">{runeError}</span>}
                </div>
              )}
              {socketInfo && runeCatalog.length > 0 && (
                <div className="rune-editor" role="group" aria-label={tt('items.runes')}>
                  <span className="rune-editor-title">{tt('items.runes')}</span>
                  {Array.from({ length: socketInfo.capacity }, (_, i) => (
                    <AppSelect
                      key={i}
                      ariaLabel={`${tt('items.runes')} ${i + 1}`}
                      value={socketInfo.runes[i] ?? ''}
                      disabled={session.busy}
                      options={runeOptions}
                      onChange={(v) => resocketRune(i, v)}
                    />
                  ))}
                  <span className="socket-count-controls">
                    <button
                      disabled={session.busy || socketInfo.capacity <= 0}
                      title={tt('items.socketRemove')}
                      aria-label={tt('items.socketRemove')}
                      onClick={() => setSocketCount(socketInfo.capacity - 1)}
                    >
                      −
                    </button>
                    <button
                      disabled={session.busy || socketInfo.capacity >= 6}
                      title={tt('items.socketAdd')}
                      aria-label={tt('items.socketAdd')}
                      onClick={() => setSocketCount(socketInfo.capacity + 1)}
                    >
                      +
                    </button>
                  </span>
                  {runeError && <span className="calc-error">{runeError}</span>}
                </div>
              )}
              <ItemText text={selectedText} lang={lang} />
              <NoteEditor
                value={session.annotations[noteKey(selected)] ?? ''}
                onCommit={(text) => session.setAnnotation(noteKey(selected), text)}
                lang={lang}
              />
            </>
          ) : (
            <p className="item-empty-hint">{tt('items.empty')}</p>
          )}
          {!isUtilitySlot(selected) && (
            <ItemOptimizer
              session={session}
              lang={lang}
              slot={selected}
              candidates={candidates}
              candidateNames={candidateNames}
              currentText={selectedText}
              onApply={(text) => (text === null ? removeItem() : switchTo(text))}
            />
          )}
        </div>
      )}

      <h3 className="panel-subheading">{tt('lib.title')}</h3>
      <LibrarySection session={session} lang={lang} selectedSlot={selected} />

      {build && build.items.jewels.length > 0 && (
        <>
          <h3 className="panel-subheading">{tt('items.jewels')}</h3>
          <JewelList texts={build.items.jewels} lang={lang} />
        </>
      )}
    </section>
  );
}

/** 珠宝列表（来自导入 build，只读；编辑走天赋树页插槽）。 */
function JewelList({ texts, lang }: { texts: string[]; lang: Lang }) {
  const [openIdx, setOpenIdx] = useState<number | null>(null);
  const names = useItemDisplayNames(texts, lang);
  return (
    <div className="library-list">
      {texts.map((text, i) => (
        <ItemRow
          key={i}
          text={text}
          name={names[i] || itemLines(text)[0] || ''}
          lang={lang}
          open={openIdx === i}
          onToggle={() => setOpenIdx(openIdx === i ? null : i)}
          actions={<CopyButton text={text} lang={lang} />}
        />
      ))}
    </div>
  );
}

/** 槽位归一化（`ring1`/`ring2` → `ring`）：库条目槽位与选中槽位的互换匹配键。 */
function slotFamily(slot: string): string {
  return slot.replace(/\s*\d+$/, '').toLowerCase();
}

/** 物品库：存起来的装备/珠宝，可搜索/按选中槽位过滤，对比差异并一键装备。 */
function LibrarySection({
  session,
  lang,
  selectedSlot,
}: {
  session: BuildSession;
  lang: Lang;
  selectedSlot: string | null;
}) {
  const tt = bindT(lang);
  const [diffFor, setDiffFor] = useState<{ id: string; diffs: DiffEntry[] } | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [slotOnly, setSlotOnly] = useState(true);
  const all = session.library.items.filter((i) => i.kind === 'item');
  // 显示名「名称·基底」一批送翻译（中文界面搜索可同时命中中英文名与基底名）。
  const names = useItemDisplayNames(
    all.map((i) => i.text),
    lang,
  );
  const q = query.trim().toLowerCase();
  const items = all.filter((entry, i) => {
    // 勾选「只看当前槽位」时，未记录槽位的旧库条目一并隐藏（槽位未知≠槽位匹配）。
    if (
      slotOnly &&
      selectedSlot &&
      (!entry.slot || slotFamily(entry.slot) !== slotFamily(selectedSlot))
    ) {
      return false;
    }
    if (!q) return true;
    return (
      entry.text.toLowerCase().includes(q) || (names[i] ?? '').toLowerCase().includes(q)
    );
  });
  if (all.length === 0) {
    return <p className="items-hint">{tt('lib.empty')}</p>;
  }

  const compare = async (id: string, text: string) => {
    const request = session.currentRequest();
    if (!request || !selectedSlot || !session.calc) return;
    const rest = (request.items ?? []).filter((it) => it.slot !== selectedSlot);
    const diffs = await previewDiff(
      { ...request, items: [...rest, { slot: selectedSlot, text }] },
      session.calc,
    );
    setDiffFor({ id, diffs });
  };

  const equip = (text: string) => {
    if (!selectedSlot) return;
    const rest = session.items.filter((it) => it.slot !== selectedSlot);
    session.setItems([...rest, { slot: selectedSlot, text }]);
    setDiffFor(null);
  };

  return (
    <>
      <div className="library-toolbar" role="group" aria-label={tt('lib.title')}>
        <input
          type="search"
          className="library-search"
          placeholder={tt('lib.search')}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          aria-label={tt('lib.search')}
        />
        {selectedSlot && (
          <label className="library-slot-filter">
            <input
              type="checkbox"
              checked={slotOnly}
              onChange={(e) => setSlotOnly(e.target.checked)}
            />
            {tt('lib.filterSlot')}（{slotLabel(lang, selectedSlot)}）
          </label>
        )}
      </div>
      {items.length === 0 && <p className="items-hint">{tt('lib.noMatch')}</p>}
    <div className="library-list">
      {items.map((entry) => (
        <ItemRow
          key={entry.id}
          text={entry.text}
          name={names[all.indexOf(entry)] ?? entry.name}
          lang={lang}
          slotTag={entry.slot ? slotLabel(lang, entry.slot) : undefined}
          open={expandedId === entry.id}
          onToggle={() => setExpandedId(expandedId === entry.id ? null : entry.id)}
          actions={
            <>
              <CopyButton text={entry.text} lang={lang} />
              <button
                disabled={session.busy || !selectedSlot}
                title={selectedSlot ? '' : tt('lib.selectSlotFirst')}
                onClick={() => {
                  setExpandedId(entry.id);
                  void compare(entry.id, entry.text);
                }}
              >
                {tt('lib.compare')}
              </button>
              <button
                disabled={session.busy || !selectedSlot}
                title={selectedSlot ? '' : tt('lib.selectSlotFirst')}
                onClick={() => equip(entry.text)}
              >
                {tt('lib.equip')}
              </button>
              <button
                className="skill-remove"
                title={tt('lib.delete')}
                onClick={() => session.removeLibraryItem(entry.id)}
              >
                ×
              </button>
            </>
          }
        >
          {diffFor?.id === entry.id && (
            <div className="library-diff">
              <DiffList diffs={diffFor.diffs} lang={lang} />
            </div>
          )}
        </ItemRow>
      ))}
    </div>
    </>
  );
}
