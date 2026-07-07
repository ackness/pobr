import type { BuildJson } from '../../api/types';
import type { Lang } from '../../lib/statDisplay';
import './items.css';

interface Props {
  build: BuildJson;
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

function ItemCard({ slot, text }: { slot: string | null; text: string }) {
  const lines = text.split('\n').map(cleanLine).filter(Boolean);
  const rarity = rarityOf(text);
  // 第一行是 Rarity 行，其后 1-2 行是名称/基底。
  const body = lines.filter((l) => !/^Rarity:/i.test(l));
  const [name, ...rest] = body;
  return (
    <article className={`item-card rarity-${rarity}`}>
      {slot && <header className="item-slot">{slot}</header>}
      <h4 className="item-name">{name}</h4>
      <div className="item-lines">
        {rest.map((line, i) => (
          <div key={i} className="item-line">
            {line}
          </div>
        ))}
      </div>
    </article>
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
  'ring3',
  'belt',
];

export function ItemsPanel({ build, lang }: Props) {
  const zh = lang === 'zh-TW';
  const sorted = [...build.items.equipped].sort(
    (a, b) => SLOT_ORDER.indexOf(a.slot) - SLOT_ORDER.indexOf(b.slot),
  );
  return (
    <section aria-labelledby="items-heading">
      <h2 id="items-heading" className="panel-heading">
        {zh ? '裝備' : 'Items'}
      </h2>
      <div className="item-grid">
        {sorted.map((item) => (
          <ItemCard key={item.slot} slot={item.slot} text={item.text} />
        ))}
      </div>
      {build.items.flasks.length > 0 && (
        <>
          <h3 className="panel-subheading">{zh ? '藥劑 / 護符' : 'Flasks / Charms'}</h3>
          <div className="item-grid">
            {build.items.flasks.map((item, i) => (
              <ItemCard key={`${item.slot}-${i}`} slot={item.slot} text={item.text} />
            ))}
          </div>
        </>
      )}
      {build.items.jewels.length > 0 && (
        <>
          <h3 className="panel-subheading">{zh ? '珠寶' : 'Jewels'}</h3>
          <div className="item-grid">
            {build.items.jewels.map((text, i) => (
              <ItemCard key={i} slot={null} text={text} />
            ))}
          </div>
        </>
      )}
    </section>
  );
}
