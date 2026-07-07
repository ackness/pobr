import { useState } from 'react';
import { useBuildSession } from './hooks/useBuildSession';
import type { Lang } from './lib/statDisplay';
import { TopBar, type TabId } from './components/shell/TopBar';
import { ImportPanel } from './components/import/ImportPanel';
import { StatSidebar } from './components/sidebar/StatSidebar';
import { ItemsPanel } from './components/items/ItemsPanel';
import { SkillsPanel } from './components/skills/SkillsPanel';
import { CalcsPanel } from './components/calcs/CalcsPanel';
import { TreePanel } from './components/tree/TreePanel';
import { ConfigPanel } from './components/config/ConfigPanel';
import './components/shell/shell.css';

export default function App() {
  const session = useBuildSession();
  const [tab, setTab] = useState<TabId>('import');
  const [lang, setLang] = useState<Lang>('en-US');

  if (session.bootError) {
    return (
      <div className="boot-screen" role="alert">
        <h1>PoBR</h1>
        <pre className="boot-error">{session.bootError}</pre>
      </div>
    );
  }
  if (session.bootMessage) {
    return (
      <div className="boot-screen" aria-busy="true">
        <h1>PoBR</h1>
        <p>{session.bootMessage}</p>
      </div>
    );
  }

  const hasBuild = session.build !== null;

  return (
    <div className="app-shell">
      <TopBar
        tab={tab}
        onTab={setTab}
        lang={lang}
        onLang={setLang}
        character={session.build?.character ?? null}
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
          {tab === 'import' && <ImportPanel session={session} lang={lang} onImported={() => setTab('items')} />}
          {tab === 'tree' && (hasBuild ? <TreePanel build={session.build!} lang={lang} /> : <EmptyHint lang={lang} />)}
          {tab === 'skills' && (hasBuild ? <SkillsPanel session={session} lang={lang} /> : <EmptyHint lang={lang} />)}
          {tab === 'items' && (hasBuild ? <ItemsPanel build={session.build!} lang={lang} /> : <EmptyHint lang={lang} />)}
          {tab === 'calcs' && (hasBuild ? <CalcsPanel session={session} lang={lang} /> : <EmptyHint lang={lang} />)}
          {tab === 'config' && (hasBuild ? <ConfigPanel session={session} lang={lang} /> : <EmptyHint lang={lang} />)}
          {tab === 'notes' && <NotesPlaceholder lang={lang} />}
        </main>
      </div>
    </div>
  );
}

function EmptyHint({ lang }: { lang: Lang }) {
  return (
    <div className="empty-hint">
      {lang === 'zh-TW' ? '先在 Import 頁匯入 Build Code' : 'Import a build code first (Import tab)'}
    </div>
  );
}

function NotesPlaceholder({ lang }: { lang: Lang }) {
  return <div className="empty-hint">{lang === 'zh-TW' ? 'Notes（佔位）' : 'Notes (placeholder)'}</div>;
}
