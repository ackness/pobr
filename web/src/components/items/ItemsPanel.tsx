import { useState } from 'react';
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

function ItemText({ text }: { text: string }) {
  const [name, ...rest] = itemLines(text);
  return (
    <>
      <h4 className="item-name">{name}</h4>
      <div className="item-lines">
        {rest.map((line, i) => (
          <div key={i} className="item-line">
            {line}
          </div>
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
            <ItemText text={selectedText} />
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
                <ItemText text={text} />
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
          <ItemText text={entry.text} />
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
