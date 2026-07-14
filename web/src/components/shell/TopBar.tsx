import type { ClassNames } from '../../api/types';
import type { CharacterState } from '../../hooks/useBuildSession';
import { LANGS, bindT, type Lang, type UiKey } from '../../lib/i18n';

export type TabId = 'build' | 'tree' | 'skills' | 'items' | 'trade' | 'calcs' | 'config';

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
}

export function TopBar({ tab, onTab, lang, onLang, character, classNames, busy }: Props) {
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
        {busy && <span className="topbar-busy" role="status">⟳</span>}
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
