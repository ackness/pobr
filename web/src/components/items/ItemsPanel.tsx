import { Fragment, useEffect, useMemo, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { ItemLineJson } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, slotLabel, type Lang } from '../../lib/i18n';
import { previewDiff, type DiffEntry } from '../../lib/compare';
import { DiffList } from '../shared/DiffList';
import { CopyButton } from '../shared/CopyButton';
import './items.css';

interface Props {
  session: BuildSession;
  lang: Lang;
}

/** 从 PoB 原始文本块提取稀有度（`Rarity: RARE` 行）。 */
function rarityOf(text: string): string {
  const m = text.match(/^Rarity:\s*(\w+)/im);
  return (m?.[1] ?? 'normal').toLowerCase();
}

/** 展示用：剥掉行内 `{tags}` 与 `[A|B]` 标注（PoB 词条内部标注语法）。 */
function cleanLine(line: string): string {
  return line
    .replace(/\{[^}]*\}/g, '')
    .replace(/\[([^\]|]*)\|([^\]]*)\]/g, '$2')
    .replace(/\[([^\]]*)\]/g, '$1')
    .trim();
}

function itemLines(text: string): string[] {
  return text
    .split('\n')
    .map(cleanLine)
    .filter((l) => l && !/^Rarity:/i.test(l));
}

/** PoB 结构标注行的键 → 本地化（词条模板翻译层不认识这些行）。 */
const STRUCT_LINE_KEYS: Record<string, { 'zh-TW': string; 'zh-CN': string }> = {
  'Item Level': { 'zh-TW': '物品等級', 'zh-CN': '物品等级' },
  'LevelReq': { 'zh-TW': '需求等級', 'zh-CN': '需求等级' },
  'Requires Level': { 'zh-TW': '需求等級', 'zh-CN': '需求等级' },
  'Implicits': { 'zh-TW': '固有詞綴', 'zh-CN': '固有词缀' },
  'Unique ID': { 'zh-TW': '唯一 ID', 'zh-CN': '唯一 ID' },
  'Quality': { 'zh-TW': '品質', 'zh-CN': '品质' },
  'Armour': { 'zh-TW': '護甲', 'zh-CN': '护甲' },
  'Evasion': { 'zh-TW': '閃避', 'zh-CN': '闪避' },
  'Energy Shield': { 'zh-TW': '能量護盾', 'zh-CN': '能量护盾' },
  'Charm Slots': { 'zh-TW': '護符插槽', 'zh-CN': '护符插槽' },
  'Radius': { 'zh-TW': '範圍', 'zh-CN': '范围' },
  'Limited to': { 'zh-TW': '限裝', 'zh-CN': '限装' },
};

/** 结构行本地化：`Item Level: 83` → `物品等级: 83`；非结构行返回 null。 */
function localizeStructLine(line: string, lang: Lang): string | null {
  if (lang === 'en-US') return null;
  const m = line.match(/^([A-Za-z' ]+):\s*(.*)$/);
  if (!m) return null;
  const entry = STRUCT_LINE_KEYS[m[1].trim()];
  return entry ? `${entry[lang]}: ${m[2]}` : null;
}

/** 中文界面：词条行批量反查翻译（结构行走本地词典；翻译不中原样显示）。 */
function useLocalizedLines(lines: string[], lang: Lang): string[] {
  const key = lines.join('\n');
  const [translated, setTranslated] = useState<Record<string, string>>({});
  useEffect(() => {
    if (lang === 'en-US' || lines.length === 0) return;
    const pending = lines.filter((l) => !localizeStructLine(l, lang));
    if (pending.length === 0) return;
    let cancelled = false;
    getBackend()
      .then((b) => b.translateLines(pending))
      .then((out) => {
        if (cancelled) return;
        const map: Record<string, string> = {};
        pending.forEach((en, i) => {
          if (out[i] !== en) map[en] = out[i];
        });
        setTranslated(map);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, lang]);
  return useMemo(
    () =>
      lang === 'en-US'
        ? lines
        : lines.map((l) => localizeStructLine(l, lang) ?? translated[l] ?? l),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [key, lang, translated],
  );
}

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
  const name = all.find((l) => l.kind === 'name')?.text ?? '';
  const body = all.filter((l) => l.kind !== 'name');
  const translated = useLocalizedLines(
    body.map((l) => l.text),
    lang,
  );
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

  const selectedText = selected ? textOf(selected) : undefined;

  return (
    <section aria-labelledby="items-heading">
      <h2 id="items-heading" className="panel-heading">
        {tt('items.title')}
      </h2>
      <p className="items-hint">{tt('items.hint')}</p>

      <div className="paper-doll" role="group" aria-label={tt('items.title')}>
        {DOLL_SLOTS.map(({ slot, area }) => {
          const text = bySlot.get(slot);
          const name = text ? itemLines(text)[0] : null;
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
              <span className="doll-slot-label">{slotLabel(lang, slot)}</span>
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
        {UTILITY_SLOTS.map((slot) => {
          const text = utilityBySlot.get(slot);
          const name = text ? itemLines(text)[0] : null;
          return (
            <button
              key={slot}
              className={`doll-slot${text ? ` rarity-${rarityOf(text)}` : ' doll-slot-empty'}${
                selected === slot ? ' is-selected' : ''
              }`}
              onClick={() => select(slot)}
              aria-label={slot}
            >
              <span className="doll-slot-label">{slotLabel(lang, slot)}</span>
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
                    onClick={() => session.saveLibraryItem('item', selectedText)}
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
            <ItemText text={selectedText} lang={lang} />
          ) : (
            <p className="item-empty-hint">{tt('items.empty')}</p>
          )}
        </div>
      )}

      <h3 className="panel-subheading">{tt('lib.title')}</h3>
      <LibrarySection session={session} lang={lang} selectedSlot={selected} />

      {build && build.items.jewels.length > 0 && (
        <>
          <h3 className="panel-subheading">{tt('items.jewels')}</h3>
          <div className="item-grid">
            {build.items.jewels.map((text, i) => (
              <article key={i} className={`item-card rarity-${rarityOf(text)}`}>
                <header className="item-slot">
                  <span className="item-actions">
                    <CopyButton text={text} lang={lang} />
                  </span>
                </header>
                <ItemText text={text} lang={lang} />
              </article>
            ))}
          </div>
        </>
      )}
    </section>
  );
}

/** 物品库：存起来的装备/珠宝，选中槽位后可对比差异并一键装备。 */
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
  const items = session.library.items.filter((i) => i.kind === 'item');
  if (items.length === 0) {
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
    <div className="library-grid">
      {items.map((entry) => (
        <article key={entry.id} className={`item-card rarity-${rarityOf(entry.text)}`}>
          <header className="item-slot">
            {entry.name}
            <span className="item-actions">
              <CopyButton text={entry.text} lang={lang} />
              <button
                disabled={session.busy || !selectedSlot}
                title={selectedSlot ? '' : tt('lib.selectSlotFirst')}
                onClick={() => compare(entry.id, entry.text)}
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
            </span>
          </header>
          <ItemText text={entry.text} lang={lang} />
          {diffFor?.id === entry.id && (
            <div className="library-diff">
              <DiffList diffs={diffFor.diffs} lang={lang} />
            </div>
          )}
        </article>
      ))}
    </div>
  );
}
