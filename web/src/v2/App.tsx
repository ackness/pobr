import { Fragment, lazy, Suspense, useEffect, useRef, useState } from 'react';
import { LayoutDashboard, Library, Network, Gem, Swords, TrendingUp, Calculator, SlidersHorizontal, Columns2, Settings2, Menu, Undo2, Redo2, PanelRightOpen, PanelLeftClose, PanelLeftOpen, Search, Check, LoaderCircle, Download } from 'lucide-react';
import { useBuildSession, type SyncNotice } from '../hooks/useBuildSession';
import { UpgradeGoalProvider } from '../hooks/useUpgradeGoal';
import { UiProvider, PanelActivity } from '../components/shared/UiContext';
import { AppDialogsProvider } from '../components/shared/AppDialogs';
import { AppSelectControl } from '../components/shared/controls';
import { TooltipProvider } from '../components/ui/tooltip';
import { Button } from '../components/ui/button';
import { Skeleton } from '../components/ui/skeleton';
import { Sheet, SheetContent, SheetHeader, SheetTitle, SheetDescription } from '../components/ui/sheet';
import { ItemsPanel } from '../components/items/ItemsPanel';
import { SkillsPanel } from '../components/skills/SkillsPanel';
import { ConfigPanel } from '../components/config/ConfigPanel';
import { bootMessageLabel, t, type Lang } from '../lib/i18n';
import { buildImportErrorMessage } from '../api/import';
import { V2_STORAGE } from '../lib/sessionStorage';
import { initializeV2Storage } from './storage';
import { DEFAULT_PREFERENCES, PREFERENCES_KEY, parsePreferences, type Preferences } from './preferences';
import { NAV_GROUPS, readRoute, routePath, type Route } from './routes';
import { words, pageNames } from './text';
import { Settings } from './Settings';
import { ResultsRail } from './ResultsRail';
import { useSettledCalc } from './motion/useSettledCalc';
import { withViewTransition } from './motion/viewTransition';
import { Overview } from './Overview';
import { BuildPage } from './BuildPage';
import { CommandPalette, ShortcutsDialog } from './CommandPalette';
import { GlobalPaste } from './GlobalPaste';
import { PreviewProvider } from './preview/PreviewProvider';
import { V2NavigateContext } from './pages/navigate';
import { ChangeBar } from './preview/ChangeBar';
import { isTyping, shortcutAction } from './shortcuts';
import { useActiveJobCount } from './jobs/activeJobs';
import { useShellRems } from './useShellRems';
import { usePinnedStats } from './pinned';

const ComparePanel = lazy(() => import('../components/compare/ComparePanel').then(m => ({ default: m.ComparePanel })));
const TradePanel = lazy(() => import('../components/trade/TradePanel').then(m => ({ default: m.TradePanel })));
const CalcsPanel = lazy(() => import('../components/calcs/CalcsPanel').then(m => ({ default: m.CalcsPanel })));
const Inspector = lazy(() => import('./inspector/Inspector'));
const TreePanel = lazy(() => import('../components/tree/TreePanel').then(m => ({ default: m.TreePanel })));
/** What a tab says after it followed a change another tab made to the library. */
function syncMessage(kind: SyncNotice['kind'], w: (en: string, cn: string, tw?: string) => string) {
  switch (kind) {
    case 'current-updated': return w('Another tab changed this stage. It was reloaded here.', '其他标签页修改了当前阶段，已在此重新载入。', '其他分頁修改了目前階段，已在此重新載入。');
    case 'current-removed': return w('The BD or stage you had open was removed in another tab. Showing one that is still there.', '当前打开的 BD 或阶段已在其他标签页删除，已切换到仍保留的内容。', '目前開啟的 BD 或階段已在其他分頁刪除，已切換至仍保留的內容。');
    case 'conflict': return w('Another tab changed the same stage. This tab\'s version was kept.', '其他标签页修改了同一阶段，已保留此标签页的版本。', '其他分頁修改了同一階段，已保留此分頁的版本。');
    default: return w('Another tab changed your BDs. This tab has those changes now.', '其他标签页修改了 BD 库，已同步到此标签页。', '其他分頁修改了 BD 庫，已同步至此分頁。');
  }
}

const icons = { overview: LayoutDashboard, build: Library, tree: Network, skills: Gem, items: Swords, trade: TrendingUp, calcs: Calculator, config: SlidersHorizontal, compare: Columns2 };

function download(text: string, name: string) {
  const url = URL.createObjectURL(new Blob([text], { type: 'application/json' }));
  const link = document.createElement('a'); link.href = url; link.download = name; link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function usePreferences() {
  const [preferences, setPreferences] = useState<Preferences>(() => {
    try { return parsePreferences(localStorage.getItem(PREFERENCES_KEY)); } catch { return DEFAULT_PREFERENCES; }
  });
  const [saveError, setSaveError] = useState(false);
  const update = (next: Preferences) => {
    setPreferences(next);
    try { localStorage.setItem(PREFERENCES_KEY, JSON.stringify(next)); setSaveError(false); } catch { setSaveError(true); }
  };
  useEffect(() => {
    const root = document.documentElement;
    const query = matchMedia('(prefers-color-scheme: dark)');
    const apply = () => root.classList.toggle('dark', preferences.theme === 'dark' || preferences.theme === 'system' && query.matches);
    apply(); query.addEventListener('change', apply);
    root.lang = preferences.lang; root.dataset.font = preferences.font; root.dataset.density = preferences.density;
    root.style.fontSize = `${preferences.fontSize}px`;
    return () => query.removeEventListener('change', apply);
  }, [preferences]);
  return { preferences, update, saveError };
}

export default function V2App() {
  const { preferences, update, saveError } = usePreferences();
  const [migration] = useState(() => {
    try { return { result: initializeV2Storage(localStorage), error: '' }; }
    catch (error) { return { result: '', error: error instanceof Error ? error.message : String(error) }; }
  });
  return <UiProvider v2 lang={preferences.lang}><TooltipProvider><UpgradeGoalProvider storageKey="pobr-v2-upgrade-goal">
    {migration.error ? <Recovery lang={preferences.lang} error={migration.error} /> : <Workspace preferences={preferences} onPreferences={update} preferencesFailed={saveError} />}
  </UpgradeGoalProvider></TooltipProvider></UiProvider>;
}

function Recovery({ lang, error }: { lang: Lang; error: string }) {
  let raw: string | null = null;
  try { raw = localStorage.getItem(V2_STORAGE.workspace) ?? localStorage.getItem('pobr-build-state'); } catch { /* Storage access itself can fail. */ }
  return <div className="v2-boot" role="alert"><div className="v2-brand-mark">P</div><h1>{words(lang, 'Your build could not be opened', '暂时无法打开 BD', '暫時無法開啟 BD')}</h1><p>{words(lang, 'Your original save is preserved. Download it before retrying.', '原始存档已保留。可先下载备份，再尝试重新打开。', '原始存檔已保留。可先下載備份，再嘗試重新開啟。')}</p><pre>{error}</pre>
    {raw && <Button onClick={() => download(raw!, 'pobr-recovery.json')}><Download />{words(lang, 'Download original save', '下载原始存档', '下載原始存檔')}</Button>}
    <Button variant="outline" asChild><a href="/">{words(lang, 'Open previous version', '打开旧版', '開啟舊版')}</a></Button></div>;
}

function Workspace({ preferences, onPreferences, preferencesFailed }: { preferences: Preferences; onPreferences: (next: Preferences) => void; preferencesFailed: boolean }) {
  const lang = preferences.lang;
  const w = (en: string, cn: string, tw?: string) => words(lang, en, cn, tw);
  const [route, setRoute] = useState(() => readRoute(location.pathname));
  const [menuOpen, setMenuOpen] = useState(false);
  const [statsOpen, setStatsOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const main = useRef<HTMLElement>(null);
  const [shellEl, setShellEl] = useState<HTMLElement | null>(null);
  const docked = useShellRems(shellEl) >= 94;
  const [inspect, setInspect] = useState<string | null>(null);
  const jobs = useActiveJobCount();
  const { pinned, toggle: togglePin } = usePinnedStats();
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);
  const [railHidden, setRailHidden] = useState(() => { try { return localStorage.getItem('pobr-v2-rail-hidden') === '1'; } catch { return false; } });
  const toggleRail = () => setRailHidden(hidden => { try { localStorage.setItem('pobr-v2-rail-hidden', hidden ? '0' : '1'); } catch { /* A remembered layout is a convenience only. */ } return !hidden; });
  const session = useBuildSession(route === 'config', V2_STORAGE);
  const activeBuild = session.workspace?.builds.find(build => build.id === session.workspace?.activeBuild);
  const stageKey = activeBuild ? `${activeBuild.id}:${activeBuild.activeStage}` : undefined;
  const view = useSettledCalc(session, stageKey);
  const [visited, setVisited] = useState<{ scope?: string; pages: Route[] }>({ pages: [] });
  // Retain planner state only within its owning stage; inactive planners receive active=false.
  useEffect(() => { setVisited(previous => ({ scope: stageKey, pages: [...new Set([...(previous.scope === stageKey ? previous.pages : []), route])] })); }, [route, stageKey]);
  const [calcsFocus, setCalcsFocus] = useState<{ id: string } | null>(null);
  const [upgradeFocus, setUpgradeFocus] = useState<{ slot: string; nonce: number; jewelType?: 'base' | 'radius' }>();
  const [skillFocus, setSkillFocus] = useState<{ group: number; nonce: number }>();
  const [treeFocus, setTreeFocus] = useState<{ nonce: number }>();
  const [reviewStage, setReviewStage] = useState<{ scope: string | null } | null>(null);
  const previousStage = useRef(stageKey);
  const stageChanged = previousStage.current !== stageKey;
  useEffect(() => {
    previousStage.current = stageKey;
    setCalcsFocus(null); setUpgradeFocus(undefined); setSkillFocus(undefined); setTreeFocus(undefined);
    setStatsOpen(false); setInspect(null);
  }, [stageKey]);
  useEffect(() => {
    if (!reviewStage || !stageKey) return;
    if (reviewStage.scope === null) setReviewStage({ scope: stageKey });
    else if (reviewStage.scope !== stageKey) setReviewStage(null);
  }, [stageKey, reviewStage]);
  useEffect(() => {
    const pop = () => { setRoute(readRoute(location.pathname)); setMenuOpen(false); setStatsOpen(false); };
    addEventListener('popstate', pop); return () => removeEventListener('popstate', pop);
  }, []);
  useEffect(() => { main.current?.scrollTo(0, 0); document.title = `${pageNames[lang][route]} · PoBR`; }, [route, lang]);
  const navigate = (next: Route) => {
    const commit = () => { if (next !== route) history.pushState(null, '', routePath(next)); setRoute(next); setMenuOpen(false); setStatsOpen(false); };
    if (next === route) commit(); else withViewTransition(commit);
  };
  const focusStat = (id: string) => { setCalcsFocus({ id }); setInspect(null); navigate('calcs'); };
  /** A number opens the calculation behind it beside the page you are on; the skill overview is a page of its own. */
  const inspectStat = (id: string) => { if (id === 'skill-metrics') focusStat(id); else { setStatsOpen(false); setInspect(id); } };
  const dialogOpen = paletteOpen || helpOpen || settingsOpen || menuOpen || statsOpen || (!docked && inspect !== null);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const action = shortcutAction(event, isTyping(event.target));
      if (!action) return;
      if (action.type === 'palette') { event.preventDefault(); setPaletteOpen(open => !open); return; }
      if (dialogOpen) return;
      if (action.type === 'help') setHelpOpen(true);
      else if (action.type === 'rail') toggleRail();
      else if (action.type === 'close') setInspect(null);
      else if (action.type === 'undo') { event.preventDefault(); if (session.canUndo) session.undoEdit(); }
      else if (action.type === 'redo') { event.preventDefault(); if (session.canRedo) session.redoEdit(); }
      else navigate(action.route);
    };
    addEventListener('keydown', onKey); return () => removeEventListener('keydown', onKey);
  });
  const retained = (page: Route) => route === page || !stageChanged && visited.scope === stageKey && visited.pages.includes(page);
  if (session.bootError) return <Recovery lang={lang} error={session.bootError} />;
  if (session.bootMessage || !session.character) return <div className="v2-boot" role="status" aria-busy="true"><div className="v2-brand-mark">P</div><h1>PoBR</h1><p>{bootMessageLabel(lang, session.bootMessage ?? '初始化…')}</p><Skeleton className="h-2 w-56" /></div>;
  const tabLabel = (page: Route) => page === 'overview' ? w('Overview', '概览', '概覽') : t(lang, `tab.${page}`);
  const go = (page: Route) => (event: React.MouseEvent) => {
    if (event.button || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault(); navigate(page);
  };
  const link = (page: Route, withIcon = false) => { const Icon = icons[page];
    return <a key={page} className="v2-tab" href={routePath(page)} aria-current={page === route ? 'page' : undefined} onClick={go(page)}>{withIcon && <Icon />}<span>{tabLabel(page)}</span></a>; };
  const tabs = (withIcons = false) => <nav className="v2-tabs" aria-label={t(lang, 'a11y.mainNavigation')}>
    {NAV_GROUPS.map((group, index) => <Fragment key={index}>{index > 0 && <span className="v2-tab-divider" aria-hidden="true" />}{group.map(page => link(page, withIcons))}</Fragment>)}</nav>;
  const brand = <a className="v2-brand" href={routePath('overview')} onClick={go('overview')}><span className="v2-brand-mark">P</span><strong>PoBR</strong></a>;
  const inspector = inspect && <Suspense fallback={<div className="insp" role="status"><Skeleton className="h-6 w-1/2" /><Skeleton className="mt-4 h-40 w-full" /></div>}>
    <Inspector session={session} view={view} lang={lang} id={inspect} onClose={() => setInspect(null)} onOpenCalcs={focusStat} pinned={pinned.includes(inspect)} onTogglePin={() => togglePin(inspect)} /></Suspense>;
  const rail = <ResultsRail session={session} view={view} lang={lang} onStatClick={inspectStat} onNavigate={navigate} pinned={pinned} onTogglePin={togglePin} />;
  return <AppDialogsProvider scope={stageKey}><PreviewProvider session={session} view={view}><V2NavigateContext.Provider value={navigate}><div className="v2-root" ref={setShellEl}>
    <a href="#v2-main" className="v2-skip-link">{t(lang, 'a11y.skipToContent')}</a>
    <header className="v2-topbar">
      <Button className="v2-menu-button" size="icon" variant="ghost" aria-label={t(lang, 'a11y.openMenu')} onClick={() => setMenuOpen(true)}><Menu /></Button>
      {brand}
      <div className="v2-crumb">
        <AppSelectControl aria-label={w('Current BD', '当前 BD', '目前 BD')} value={activeBuild?.id ?? ''} onChange={e => session.selectStage(e.target.value)}>{session.workspace?.builds.map((build, index) => <option key={build.id} value={build.id}>{build.name || `${w('BD', 'BD', 'BD')} ${index + 1}`}</option>)}</AppSelectControl>
        <span className="v2-crumb-sep" aria-hidden="true">/</span>
        <AppSelectControl aria-label={w('Current stage', '当前阶段', '目前階段')} value={activeBuild?.activeStage ?? ''} onChange={e => { if (activeBuild) session.selectStage(activeBuild.id, e.target.value); }}>{activeBuild?.stages.map((stage, index) => <option key={stage.id} value={stage.id}>{stage.name || `${w('Stage', '阶段', '階段')} ${index + 1}`}</option>)}</AppSelectControl>
      </div>
      {tabs()}
      <div className="v2-topbar-actions">
        <Button variant="ghost" size="sm" className="v2-palette-button" aria-label={w('Search and commands', '搜索与命令', '搜尋與命令')} onClick={() => setPaletteOpen(true)}><Search /><kbd>{/Mac|iPhone|iPad/.test(navigator.platform) ? '⌘K' : 'Ctrl K'}</kbd></Button>
        <Button variant="ghost" size="sm" className="v2-builds-button" aria-current={route === 'build' ? 'page' : undefined} onClick={() => navigate('build')}><Library /><span className="v2-quiet-label">{w('BDs', 'BD 库', 'BD 庫')}</span></Button>
        <span className={`v2-save-state${session.storageFailed ? ' is-error' : ''}`} role="status">{session.busy ? <LoaderCircle className="animate-spin" /> : <Check />}<span className="v2-quiet-label">{session.storageFailed ? w('Save failed', '保存失败', '儲存失敗') : session.busy ? w('Calculating', '计算中', '計算中') : w('Saved locally', '已保存到本地', '已儲存至本地')}</span></span>
        <Button className="v2-history-button" variant="ghost" size="icon" aria-label={w('Undo', '撤销', '復原')} disabled={!session.canUndo || session.busy} onClick={session.undoEdit}><Undo2 /></Button>
        <Button className="v2-history-button" variant="ghost" size="icon" aria-label={w('Redo', '重做', '重做')} disabled={!session.canRedo || session.busy} onClick={session.redoEdit}><Redo2 /></Button>
        <Button className="v2-rail-toggle" variant="ghost" size="icon" aria-pressed={railHidden} aria-label={w('Results panel', '结果栏', '結果欄')} onClick={toggleRail}>{railHidden ? <PanelLeftOpen /> : <PanelLeftClose />}</Button>
        <Button className="v2-stats-button" variant="ghost" size="icon" aria-label={w('Character stats', '角色属性', '角色屬性')} onClick={() => setStatsOpen(true)}><PanelRightOpen /></Button>
        <Button className="v2-settings-button" variant="ghost" size="icon" aria-label={t(lang, 'common.settings')} onClick={() => setSettingsOpen(true)}><Settings2 /></Button>
      </div>
      <div className="v2-progress" data-active={session.busy || jobs > 0 || undefined} aria-hidden="true" />
    </header>
    <div className="v2-body" data-rail={railHidden || undefined} data-inspector={docked && inspect ? '' : undefined}>
      <div className="v2-rail">{rail}</div>
      <main id="v2-main" tabIndex={-1} className={`v2-main v2-page-${route}`} ref={main} key={stageKey}>
        {session.storageFailed && <div className="v2-alert" role="alert"><p>{w('Browser storage is full or unavailable. Download a backup before leaving.', '浏览器存储已满或不可用，请先下载完整备份。', '瀏覽器儲存空間已滿或無法使用，請先下載完整備份。')}</p><Button onClick={() => download(session.exportWorkspace(), 'pobr-v2-backup.json')}><Download />{w('Download backup', '下载备份', '下載備份')}</Button></div>}
        {preferencesFailed && <p className="v2-alert" role="alert">{w('Display preferences could not be saved on this device.', '显示偏好暂时无法保存到当前设备。', '顯示偏好暫時無法儲存至目前裝置。')}</p>}
        {session.syncNotice && <div className="v2-notice" role="status" data-sync={session.syncNotice.kind}><p>{syncMessage(session.syncNotice.kind, w)}</p><Button variant="ghost" onClick={session.dismissSyncNotice}>{w('Dismiss', '知道了', '知道了')}</Button></div>}
        {reviewStage?.scope === stageKey && <div className="v2-notice" role="note"><p>{t(lang, 'config.importReview')}</p><Button variant="outline" onClick={() => { setReviewStage(null); navigate('config'); }}>{t(lang, 'config.review')}</Button><Button variant="ghost" onClick={() => setReviewStage(null)}>{t(lang, 'config.reviewDismiss')}</Button></div>}
        {Object.keys(session.editorDrafts).length > 0 && <div className="v2-notice" role="status"><p>{t(lang, 'editor.draftHint')}</p><Button variant="outline" onClick={() => navigate('items')}>{t(lang, 'editor.reviewItems')}</Button><Button variant="outline" onClick={() => navigate('tree')}>{t(lang, 'editor.reviewJewels')}</Button></div>}
        {session.calcError && <div className="v2-alert" role="alert"><strong>{w('Calculation unavailable', '当前计算不可用', '目前計算無法使用')}</strong><p>{session.calcError}</p></div>}
        {session.operationError && <div className="v2-alert" role="alert">{buildImportErrorMessage(session.operationError, lang)}</div>}
        <div className="v2-page-content">
          {route === 'overview' && <Overview session={session} view={view} lang={lang} navigate={navigate} focusStat={inspectStat} />}
          {route === 'build' && <BuildPage session={session} lang={lang} onImported={() => { setReviewStage({ scope: null }); navigate('overview'); }} />}
          {route === 'items' && <ItemsPanel session={session} lang={lang} onUpgrade={slot => { setUpgradeFocus({ slot, nonce: Date.now() }); navigate('trade'); }} />}
          {route === 'config' && <ConfigPanel session={session} lang={lang} />}
          <Suspense fallback={<div role="status" className="v2-panel-loading"><Skeleton className="h-12 w-2/3" /><Skeleton className="h-64 w-full" /><p>{t(lang, 'ui.loadingPanel')}</p></div>}>
            {route === 'calcs' && <CalcsPanel session={session} lang={lang} focus={stageChanged ? null : calcsFocus} onFocusConsumed={() => setCalcsFocus(null)} />}
            {route === 'compare' && <ComparePanel session={session} lang={lang} />}
            {retained('tree') && <div hidden={route !== 'tree'}><PanelActivity active={route === 'tree'}><TreePanel session={session} lang={lang} active={route === 'tree'} focusPlanner={stageChanged ? undefined : treeFocus} onJewelSearch={socket => { setUpgradeFocus({ slot: `Jewel@${socket}`, nonce: Date.now(), jewelType: 'radius' }); navigate('trade'); }} /></PanelActivity></div>}
            {retained('skills') && <div hidden={route !== 'skills'}><PanelActivity active={route === 'skills'}><SkillsPanel session={session} lang={lang} active={route === 'skills'} focusOptimizer={stageChanged ? undefined : skillFocus} /></PanelActivity></div>}
            {retained('trade') && <div hidden={route !== 'trade'}><PanelActivity active={route === 'trade'}><TradePanel session={session} lang={lang} active={route === 'trade'} focus={stageChanged ? undefined : upgradeFocus} onSkills={group => { setSkillFocus({ group, nonce: Date.now() }); navigate('skills'); }} onTree={() => { setTreeFocus({ nonce: Date.now() }); navigate('tree'); }} /></PanelActivity></div>}
          </Suspense>
        </div>
      </main>
      <ChangeBar lang={lang} />
      {docked && inspector && <aside className="v2-inspector" aria-label={w('Why this number', '这个数从哪来', '這個數從哪來')}>{inspector}</aside>}
    </div>
    <Sheet open={!docked && inspect !== null} onOpenChange={open => { if (!open) setInspect(null); }}><SheetContent className="v2-inspector-sheet" showCloseButton={false}><SheetHeader className="sr-only"><SheetTitle>{w('Why this number', '这个数从哪来', '這個數從哪來')}</SheetTitle><SheetDescription>{w('The calculation behind the selected value.', '所选数值背后的计算。', '所選數值背後的計算。')}</SheetDescription></SheetHeader>{inspector}</SheetContent></Sheet>
    <CommandPalette open={paletteOpen} onOpenChange={setPaletteOpen} session={session} view={view} lang={lang} pageLabel={tabLabel} onNavigate={navigate} onInspect={inspectStat} onSettings={() => setSettingsOpen(true)}
      onToggleTheme={() => onPreferences({ ...preferences, theme: document.documentElement.classList.contains('dark') ? 'light' : 'dark' })} themeLabel={document.documentElement.classList.contains('dark') ? w('Switch to the light theme', '切换到浅色主题', '切換到淺色主題') : w('Switch to the dark theme', '切换到深色主题', '切換到深色主題')} />
    <ShortcutsDialog open={helpOpen} onOpenChange={setHelpOpen} lang={lang} pageLabel={tabLabel} />
    <GlobalPaste session={session} lang={lang} onImported={() => { setReviewStage({ scope: null }); navigate('overview'); }} />
    <Sheet open={menuOpen} onOpenChange={setMenuOpen}><SheetContent side="left" className="v2-mobile-navigation"><SheetHeader className="sr-only"><SheetTitle>{t(lang, 'a11y.mainNavigation')}</SheetTitle><SheetDescription>{w('Choose a workspace page', '选择工作台页面', '選擇工作台頁面')}</SheetDescription></SheetHeader>
      {brand}{tabs(true)}
      <div className="v2-nav-footer"><Button variant="ghost" onClick={() => { setMenuOpen(false); setSettingsOpen(true); }}><Settings2 />{w('Appearance & language', '外观与语言', '外觀與語言')}</Button></div>
    </SheetContent></Sheet>
    <Sheet open={statsOpen} onOpenChange={setStatsOpen}><SheetContent className="v2-stat-sheet"><SheetHeader><SheetTitle>{w('Character stats', '角色属性', '角色屬性')}</SheetTitle><SheetDescription>{w('Select a value to inspect its calculation.', '点击数值查看计算明细。', '點擊數值查看計算明細。')}</SheetDescription></SheetHeader>{rail}</SheetContent></Sheet>
    <Settings open={settingsOpen} onOpenChange={setSettingsOpen} preferences={preferences} onChange={onPreferences} version={`v${__POBR_APP_VERSION__} · ${session.dataVersion}`} />
  </div></V2NavigateContext.Provider></PreviewProvider></AppDialogsProvider>;
}
