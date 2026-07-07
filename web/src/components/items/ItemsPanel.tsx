import { useState } from 'react';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
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

function ItemText({ text }: { text: string }) {
  const lines = text.split('\n').map(cleanLine).filter(Boolean);
  const body = lines.filter((l) => !/^Rarity:/i.test(l));
  const [name, ...rest] = body;
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

/** 装备槽位展示顺序（PoB2 习惯）。 */
const SLOT_ORDER = [
  'weapon1',
  'weapon2',
  'helmet',
  'body_armour',
  'gloves',
  'boots',
  'amulet',
  'ring1',
  'ring2',
  'belt',
];

const ITEM_TEMPLATE = 'Rarity: RARE\nNew Item\nSapphire Ring\n+50 to maximum Life';

/** 装备编辑器：每槽 PoB 文本块直编（与导入路径同一解析器），即改即算。 */
export function ItemsPanel({ session, lang }: Props) {
  const tt = bindT(lang);
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState('');
  const build = session.build;
  const items = session.items;
  const bySlot = new Map(items.map((item) => [item.slot, item.text]));

  const startEdit = (slot: string) => {
    setEditing(slot);
    setDraft(bySlot.get(slot) ?? ITEM_TEMPLATE);
  };
  const applyEdit = () => {
    if (!editing) return;
    const rest = items.filter((item) => item.slot !== editing);
    session.setItems([...rest, { slot: editing, text: draft }]);
    setEditing(null);
  };
  const removeItem = (slot: string) => {
    session.setItems(items.filter((item) => item.slot !== slot));
    if (editing === slot) setEditing(null);
  };

  return (
    <section aria-labelledby="items-heading">
      <h2 id="items-heading" className="panel-heading">
        {tt('items.title')}
      </h2>
      <p className="items-hint">
        {tt('items.hint')}
      </p>
      <div className="item-grid">
        {SLOT_ORDER.map((slot) => {
          const text = bySlot.get(slot);
          const isEditing = editing === slot;
          return (
            <article
              key={slot}
              className={`item-card${text ? ` rarity-${rarityOf(text)}` : ' item-card-empty'}`}
            >
              <header className="item-slot">
                {slot}
                <span className="item-actions">
                  {!isEditing && (
                    <button disabled={session.busy} onClick={() => startEdit(slot)}>
                      {text ? tt('items.edit') : tt('items.add')}
                    </button>
                  )}
                  {text && !isEditing && (
                    <button
                      className="skill-remove"
                      disabled={session.busy}
                      title={tt('items.remove')}
                      onClick={() => removeItem(slot)}
                    >
                      ×
                    </button>
                  )}
                </span>
              </header>
              {isEditing ? (
                <div className="item-editor">
                  <textarea
                    rows={8}
                    value={draft}
                    spellCheck={false}
                    aria-label={`${slot} item text`}
                    onChange={(e) => setDraft(e.target.value)}
                  />
                  <div className="item-editor-actions">
                    <button disabled={session.busy} onClick={applyEdit}>
                      {tt('items.apply')}
                    </button>
                    <button onClick={() => setEditing(null)}>{tt('items.cancel')}</button>
                  </div>
                </div>
              ) : text ? (
                <ItemText text={text} />
              ) : (
                <p className="item-empty-hint">{tt('items.empty')}</p>
              )}
            </article>
          );
        })}
      </div>
      {build && build.items.flasks.length > 0 && (
        <>
          <h3 className="panel-subheading">
            {tt('items.flasks')}
          </h3>
          <div className="item-grid">
            {build.items.flasks.map((item, i) => (
              <article key={`${item.slot}-${i}`} className={`item-card rarity-${rarityOf(item.text)}`}>
                <header className="item-slot">{item.slot}</header>
                <ItemText text={item.text} />
              </article>
            ))}
          </div>
        </>
      )}
      {build && build.items.jewels.length > 0 && (
        <>
          <h3 className="panel-subheading">
            {tt('items.jewels')}
          </h3>
          <div className="item-grid">
            {build.items.jewels.map((text, i) => (
              <article key={i} className={`item-card rarity-${rarityOf(text)}`}>
                <ItemText text={text} />
              </article>
            ))}
          </div>
        </>
      )}
    </section>
  );
}
