import { useState } from 'react';
import { useBuildSession } from './hooks/useBuildSession';
import type { Lang } from './lib/i18n';
import { TopBar, type TabId } from './components/shell/TopBar';
import { BuildPanel } from './components/import/BuildPanel';
import { StatSidebar } from './components/sidebar/StatSidebar';
import { ItemsPanel } from './components/items/ItemsPanel';
import { SkillsPanel } from './components/skills/SkillsPanel';
import { CalcsPanel } from './components/calcs/CalcsPanel';
import { TreePanel } from './components/tree/TreePanel';
import { ConfigPanel } from './components/config/ConfigPanel';
import { NotesPanel } from './components/notes/NotesPanel';
import './components/shell/shell.css';

export default function App() {
  const session = useBuildSession();
  // 界面偏好（页签/语言）实时持久化到浏览器。
  const [tab, setTabState] = useState<TabId>(
    () => (localStorage.getItem('pobr-tab') as TabId) || 'build',
  );
  const [lang, setLangState] = useState<Lang>(
    () => (localStorage.getItem('pobr-lang') as Lang) || 'en-US',
  );
  const setTab = (next: TabId) => {
    setTabState(next);
    localStorage.setItem('pobr-tab', next);
  };
  const setLang = (next: Lang) => {
    setLangState(next);
    localStorage.setItem('pobr-lang', next);
  };
  // 侧边栏数值点击 → 跳 Calcs 并展开对应 breakdown（对象每次新建，重复点击同一项也触发）。
  const [calcsFocus, setCalcsFocus] = useState<{ id: string } | null>(null);
  const focusStat = (id: string) => {
    setCalcsFocus({ id });
    setTab('calcs');
  };

  if (session.bootError) {
    return (
      <div className="boot-screen" role="alert">
        <h1>PoBR</h1>
        <pre className="boot-error">{session.bootError}</pre>
      </div>
    );
  }
  if (session.bootMessage || !session.character) {
    return (
      <div className="boot-screen" aria-busy="true">
        <h1>PoBR</h1>
        <p>{session.bootMessage ?? '…'}</p>
      </div>
    );
  }

  return (
    <div className="app-shell">
      <TopBar
        tab={tab}
        onTab={setTab}
        lang={lang}
        onLang={setLang}
        character={session.character}
        classNames={session.classNames}
        busy={session.busy}
      />
      <div className="app-body">
        <StatSidebar calc={session.calc} lang={lang} onStatClick={focusStat} />
        <main className="app-main">
          {session.error && (
            <div className="calc-error" role="alert">
              {session.error}
            </div>
          )}
          {tab === 'build' && <BuildPanel session={session} lang={lang} onImported={() => setTab('items')} />}
          {tab === 'tree' && <TreePanel session={session} lang={lang} />}
          {tab === 'skills' && <SkillsPanel session={session} lang={lang} />}
          {tab === 'items' && <ItemsPanel session={session} lang={lang} />}
          {tab === 'calcs' && <CalcsPanel session={session} lang={lang} focus={calcsFocus} />}
          {tab === 'config' && <ConfigPanel session={session} lang={lang} />}
          {tab === 'notes' && <NotesPanel session={session} lang={lang} />}
        </main>
      </div>
    </div>
  );
}
