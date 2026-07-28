import { useState } from 'react';
import { useBuildSession } from './hooks/useBuildSession';
import { t, type Lang } from './lib/i18n';
import { TAB_IDS, TopBar, type TabId } from './components/shell/TopBar';
import { BuildPanel } from './components/import/BuildPanel';
import { StatSidebar } from './components/sidebar/StatSidebar';
import { ItemsPanel } from './components/items/ItemsPanel';
import { TradePanel } from './components/trade/TradePanel';
import { SkillsPanel } from './components/skills/SkillsPanel';
import { CalcsPanel } from './components/calcs/CalcsPanel';
import { TreePanel } from './components/tree/TreePanel';
import { ConfigPanel } from './components/config/ConfigPanel';
import './components/shell/shell.css';

export default function App() {
  const session = useBuildSession();
  // 界面偏好（页签/语言）实时持久化到浏览器。
  const [tab, setTabState] = useState<TabId>(() => {
    // 兜底：历史存的页签可能已下线（如原独立笔记页）。
    const saved = localStorage.getItem('pobr-tab') as TabId | null;
    return saved && TAB_IDS.includes(saved) ? saved : 'build';
  });
  const [lang, setLangState] = useState<Lang>(
    () => (localStorage.getItem('pobr-lang') as Lang) || 'en-US',
  );
  const setTab = (next: TabId) => {
    setTabState(next);
    localStorage.setItem('pobr-tab', next);
    // 各页签内容高度差异大，沿用上一页的滚动位置会露出页底黑区。
    window.scrollTo(0, 0);
  };
  const setLang = (next: Lang) => {
    setLangState(next);
    localStorage.setItem('pobr-lang', next);
  };
  // Beta 提示横幅：关闭后持久化，不再打扰。
  const [betaDismissed, setBetaDismissed] = useState(
    () => localStorage.getItem('pobr-beta-dismissed') === '1',
  );
  const dismissBeta = () => {
    setBetaDismissed(true);
    localStorage.setItem('pobr-beta-dismissed', '1');
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
        loadouts={session.loadouts}
        activeLoadout={session.activeLoadout}
        onLoadout={(i) => {
          const l = session.loadouts[i];
          if (!l) return;
          // 切换是整份重解码——有未保存编辑时先确认（见 useBuildSession.switchLoadout）。
          if (session.isDirty && !window.confirm(t(lang, 'loadout.confirmDiscard'))) return;
          void session.switchLoadout({ tree: l.tree, item: l.item, skill: l.skill });
        }}
      />
      {!betaDismissed && (
        <div className="beta-banner" role="note">
          <span>{t(lang, 'beta.notice')}</span>
          <button className="beta-banner-dismiss" onClick={dismissBeta}>
            {t(lang, 'beta.dismiss')}
          </button>
        </div>
      )}
      <div className="app-body">
        <StatSidebar session={session} lang={lang} onStatClick={focusStat} />
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
          {tab === 'trade' && <TradePanel session={session} lang={lang} />}
          {tab === 'calcs' && (
            <CalcsPanel
              session={session}
              lang={lang}
              focus={calcsFocus}
              onFocusConsumed={() => setCalcsFocus(null)}
            />
          )}
          {tab === 'config' && <ConfigPanel session={session} lang={lang} />}
        </main>
      </div>
    </div>
  );
}
