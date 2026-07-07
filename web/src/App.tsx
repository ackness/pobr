import { useState } from 'react';
import { useBuildSession } from './hooks/useBuildSession';
import type { Lang } from './lib/statDisplay';
import { TopBar, type TabId } from './components/shell/TopBar';
import { BuildPanel } from './components/import/BuildPanel';
import { StatSidebar } from './components/sidebar/StatSidebar';
import { ItemsPanel } from './components/items/ItemsPanel';
import { SkillsPanel } from './components/skills/SkillsPanel';
import { CalcsPanel } from './components/calcs/CalcsPanel';
import { TreePanel } from './components/tree/TreePanel';
import { ConfigPanel } from './components/config/ConfigPanel';
import './components/shell/shell.css';

export default function App() {
  const session = useBuildSession();
  const [tab, setTab] = useState<TabId>('build');
  const [lang, setLang] = useState<Lang>('en-US');

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
        busy={session.busy}
      />
      <div className="app-body">
        <StatSidebar calc={session.calc} lang={lang} />
        <main className="app-main">
          {session.error && (
            <div className="calc-error" role="alert">
              {session.error}
            </div>
          )}
          {tab === 'build' && <BuildPanel session={session} lang={lang} onImported={() => setTab('items')} />}
          {tab === 'tree' && <TreePanel session={session} lang={lang} />}
          {tab === 'skills' && <SkillsPanel session={session} lang={lang} />}
          {tab === 'items' && <ItemsPanel build={session.build} lang={lang} />}
          {tab === 'calcs' && <CalcsPanel session={session} lang={lang} />}
          {tab === 'config' && <ConfigPanel session={session} lang={lang} />}
          {tab === 'notes' && <NotesPlaceholder lang={lang} />}
        </main>
      </div>
    </div>
  );
}

function NotesPlaceholder({ lang }: { lang: Lang }) {
  return <div className="empty-hint">{lang === 'zh-TW' ? 'Notes（佔位）' : 'Notes (placeholder)'}</div>;
}
