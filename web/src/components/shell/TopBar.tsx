import type { ClassNames, LoadoutJson } from '../../api/types';
import type { CharacterState } from '../../hooks/useBuildSession';
import { LANGS, bindT, type Lang, type UiKey } from '../../lib/i18n';

export type TabId = 'build' | 'tree' | 'skills' | 'items' | 'trade' | 'guidance' | 'calcs' | 'config';

// Keep the reference panel for future shared builds, but hide it from navigation
// and saved-tab restoration while its only data source is historical fixtures.
export const TAB_IDS: TabId[] = ['build', 'tree', 'skills', 'items', 'trade', 'calcs', 'config'];

const TABS: { id: TabId; key: UiKey }[] = [
  { id: 'build', key: 'tab.build' },
  { id: 'tree', key: 'tab.tree' },
  { id: 'skills', key: 'tab.skills' },
  { id: 'items', key: 'tab.items' },
  { id: 'trade', key: 'tab.trade' },
  { id: 'calcs', key: 'tab.calcs' },
  { id: 'config', key: 'tab.config' },
];

const LANG_LABEL: Record<Lang, string> = {
  'en-US': 'EN',
  'zh-TW': '繁',
  'zh-CN': '简',
};

interface Props {
  tab: TabId;
  onTab: (tab: TabId) => void;
  lang: Lang;
  onLang: (lang: Lang) => void;
  character: CharacterState | null;
  classNames: ClassNames;
  busy: boolean;
  /** 成组切换清单；≤1 条时不渲染下拉（无可切的组）。 */
  loadouts: LoadoutJson[];
  activeLoadout: number | null;
  onLoadout: (index: number) => void;
  /** 组管理（复制 / 重命名 / 删除当前组）；无导入 code 时为 undefined。 */
  onManageLoadout?: (op: 'duplicate' | 'rename' | 'remove') => void;
}

export function TopBar({
  tab,
  onTab,
  lang,
  onLang,
  character,
  classNames,
  busy,
  loadouts,
  activeLoadout,
  onLoadout,
  onManageLoadout,
}: Props) {
  const tt = bindT(lang);
  const displayName = (c: CharacterState) => {
    const raw = c.ascendancy_name || c.class_name;
    if (lang === 'en-US') return raw;
    const map = c.ascendancy_name ? classNames.ascendancies : classNames.classes;
    return map[raw] ?? raw;
  };
  const nextLang = LANGS[(LANGS.indexOf(lang) + 1) % LANGS.length];
  return (
    <header className="topbar">
      <span className="topbar-brand">PoBR</span>
      <span className="topbar-beta">BETA</span>
      <nav className="topbar-tabs" aria-label="Main navigation">
        {TABS.map((entry) => (
          <button
            key={entry.id}
            className={`topbar-tab${tab === entry.id ? ' is-active' : ''}`}
            aria-current={tab === entry.id ? 'page' : undefined}
            onClick={() => onTab(entry.id)}
          >
            {tt(entry.key)}
          </button>
        ))}
      </nav>
      <div className="topbar-right">
        <a className="topbar-source" href="https://github.com/ackness/pobr" target="_blank" rel="noopener noreferrer"
          aria-label={tt('app.source')} title={tt('app.source')}>
          <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor" aria-hidden="true" focusable="false">
            <path d="M12 .5a12 12 0 0 0-3.79 23.39c.6.11.82-.26.82-.58v-2.24c-3.34.73-4.04-1.42-4.04-1.42-.55-1.39-1.34-1.76-1.34-1.76-1.09-.75.08-.73.08-.73 1.21.08 1.85 1.24 1.85 1.24 1.07 1.84 2.81 1.31 3.5 1 .11-.78.42-1.31.76-1.61-2.67-.3-5.47-1.34-5.47-5.93 0-1.31.47-2.38 1.24-3.22-.12-.31-.54-1.52.12-3.17 0 0 1.01-.32 3.3 1.23A11.5 11.5 0 0 1 12 6.3c1.02 0 2.04.14 3 .4 2.29-1.55 3.3-1.23 3.3-1.23.66 1.65.24 2.86.12 3.17.77.84 1.23 1.91 1.23 3.22 0 4.6-2.8 5.63-5.48 5.93.43.38.82 1.12.82 2.25v3.27c0 .32.22.7.83.58A12 12 0 0 0 12 .5Z" />
          </svg>
        </a>
        {busy && <span className="topbar-busy" role="status">⟳</span>}
        {loadouts.length > 0 && onManageLoadout && (
          <select
            className="topbar-loadout"
            aria-label={tt('loadout.switch')}
            title={tt('loadout.switch')}
            value={activeLoadout ?? ''}
            onChange={(e) => {
              const v = e.target.value;
              // 操作项用 `op:` 前缀区分于组下标；选完复位到当前组，避免下拉停在操作项上。
              if (v.startsWith('op:')) {
                e.target.value = String(activeLoadout ?? 0);
                onManageLoadout(v.slice(3) as 'duplicate' | 'rename' | 'remove');
                return;
              }
              onLoadout(Number(v));
            }}
            disabled={busy}
          >
            {activeLoadout === null && <option value="">—</option>}
            {loadouts.map((l, i) => (
              <option key={`${l.name}-${i}`} value={i}>
                {l.name}
              </option>
            ))}
            <option disabled>──────</option>
            <option value="op:duplicate">{tt('loadout.new')}</option>
            <option value="op:rename">{tt('loadout.rename')}</option>
            {loadouts.length > 1 && <option value="op:remove">{tt('loadout.remove')}</option>}
          </select>
        )}
        {character && (
          <span className="topbar-character">
            Lv{character.level} {displayName(character)}
          </span>
        )}
        <button
          className="topbar-lang"
          onClick={() => onLang(nextLang)}
          aria-label="Switch language"
          title={`→ ${LANG_LABEL[nextLang]}`}
        >
          {LANG_LABEL[lang]}
        </button>
      </div>
    </header>
  );
}
