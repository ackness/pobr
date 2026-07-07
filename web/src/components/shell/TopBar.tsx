import type { CharacterState } from '../../hooks/useBuildSession';
import type { Lang } from '../../lib/statDisplay';

export type TabId = 'build' | 'tree' | 'skills' | 'items' | 'calcs' | 'config' | 'notes';

const TABS: { id: TabId; label: { 'en-US': string; 'zh-TW': string } }[] = [
  { id: 'build', label: { 'en-US': 'Build', 'zh-TW': '構建' } },
  { id: 'tree', label: { 'en-US': 'Tree', 'zh-TW': '天賦樹' } },
  { id: 'skills', label: { 'en-US': 'Skills', 'zh-TW': '技能' } },
  { id: 'items', label: { 'en-US': 'Items', 'zh-TW': '裝備' } },
  { id: 'calcs', label: { 'en-US': 'Calcs', 'zh-TW': '計算' } },
  { id: 'config', label: { 'en-US': 'Config', 'zh-TW': '配置' } },
  { id: 'notes', label: { 'en-US': 'Notes', 'zh-TW': '筆記' } },
];

interface Props {
  tab: TabId;
  onTab: (tab: TabId) => void;
  lang: Lang;
  onLang: (lang: Lang) => void;
  character: CharacterState | null;
  busy: boolean;
}

export function TopBar({ tab, onTab, lang, onLang, character, busy }: Props) {
  return (
    <header className="topbar">
      <span className="topbar-brand">PoBR</span>
      <nav className="topbar-tabs" aria-label="Main navigation">
        {TABS.map((t) => (
          <button
            key={t.id}
            className={`topbar-tab${tab === t.id ? ' is-active' : ''}`}
            aria-current={tab === t.id ? 'page' : undefined}
            onClick={() => onTab(t.id)}
          >
            {t.label[lang]}
          </button>
        ))}
      </nav>
      <div className="topbar-right">
        {busy && <span className="topbar-busy" role="status">⟳</span>}
        {character && (
          <span className="topbar-character">
            Lv{character.level} {character.ascendancy_name || character.class_name}
          </span>
        )}
        <button
          className="topbar-lang"
          onClick={() => onLang(lang === 'en-US' ? 'zh-TW' : 'en-US')}
          aria-label="Switch language"
        >
          {lang === 'en-US' ? '繁中' : 'EN'}
        </button>
      </div>
    </header>
  );
}
