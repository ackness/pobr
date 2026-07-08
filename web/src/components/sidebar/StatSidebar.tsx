import type { CalculateBuildResponse } from '../../api/types';
import { STAT_SECTIONS, formatStatValue, statMap, type Lang } from '../../lib/statDisplay';
import './sidebar.css';

interface Props {
  calc: CalculateBuildResponse | null;
  lang: Lang;
}

/** PoB2 式左侧常驻 stat 侧边栏：分组展示 display_catalog 字段。 */
export function StatSidebar({ calc, lang }: Props) {
  const values = calc ? statMap(calc.stats) : null;

  return (
    <aside className="stat-sidebar" aria-label="Character stats">
      {STAT_SECTIONS.map((section) => {
        const rows = section.rows.filter((row) => {
          if (!values) return !row.hideZero;
          const v = values.get(row.id);
          if (v === undefined) return false;
          return !(row.hideZero && (v === 0 || v === null));
        });
        if (rows.length === 0) return null;
        return (
          <section key={section.title['en-US']} className="stat-section">
            <h3>{section.title[lang]}</h3>
            <dl>
              {rows.map((row) => (
                <div className="stat-row" key={row.id}>
                  <dt>{row.label[lang]}</dt>
                  <dd style={row.colorVar ? { color: `var(--${row.colorVar})` } : undefined}>
                    {values ? formatStatValue(values.get(row.id) ?? null, row.format) : '—'}
                  </dd>
                </div>
              ))}
            </dl>
          </section>
        );
      })}
    </aside>
  );
}
