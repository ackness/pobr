import { lazy, Suspense, useEffect, useRef, useState } from 'react';
import { LayoutDashboard, Library, Network, Gem, Swords, TrendingUp, Calculator, SlidersHorizontal, Columns2, Settings2, Menu, PanelRightOpen, Check, LoaderCircle, ChevronRight, ArrowLeft, Download } from 'lucide-react';
import { useBuildSession } from '../hooks/useBuildSession';
import { UpgradeGoalProvider } from '../hooks/useUpgradeGoal';
import { UiProvider } from '../components/shared/UiContext';
import { AppDialogsProvider } from '../components/shared/AppDialogs';
import { AppSelectControl } from '../components/shared/controls';
import { TooltipProvider } from '../components/ui/tooltip';
import { Button } from '../components/ui/button';
import { Badge } from '../components/ui/badge';
import { Skeleton } from '../components/ui/skeleton';
import { Sheet, SheetContent, SheetHeader, SheetTitle, SheetDescription } from '../components/ui/sheet';
import { StatSidebar } from '../components/sidebar/StatSidebar';
import { BuildPanel } from '../components/import/BuildPanel';
import { ItemsPanel } from '../components/items/ItemsPanel';
import { SkillsPanel } from '../components/skills/SkillsPanel';
import { ConfigPanel } from '../components/config/ConfigPanel';
import { bootMessageLabel, t, type Lang } from '../lib/i18n';
import { buildImportErrorMessage } from '../api/import';
import { V2_STORAGE } from '../lib/sessionStorage';
import { initializeV2Storage } from './storage';
import { DEFAULT_PREFERENCES, PREFERENCES_KEY, parsePreferences, type Preferences } from './preferences';
import { ROUTES, readRoute, routePath, type Route } from './routes';
import { words, pageNames } from './text';
import { Settings } from './Settings';
import { Overview } from './Overview';

const ComparePanel = lazy(() => import('../components/compare/ComparePanel').then(m => ({ default: m.ComparePanel })));
const TradePanel = lazy(() => import('../components/trade/TradePanel').then(m => ({ default: m.TradePanel })));
const CalcsPanel = lazy(() => import('../components/calcs/CalcsPanel').then(m => ({ default: m.CalcsPanel })));
const TreePanel = lazy(() => import('../components/tree/TreePanel').then(m => ({ default: m.TreePanel })));
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
  return <div className="v2-boot" role="alert"><div className="v2-brand-mark">P</div><h1>{words(lang, 'Your build could not be opened', '暂时无法打开构筑', '暫時無法開啟構築')}</h1><p>{words(lang, 'Your original save is preserved. Download it before retrying.', '原始存档已保留。可先下载备份，再尝试重新打开。', '原始存檔已保留。可先下載備份，再嘗試重新開啟。')}</p><pre>{error}</pre>
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
  const session = useBuildSession(route === 'config', V2_STORAGE);
  const activeBuild = session.workspace?.builds.find(build => build.id === session.workspace?.activeBuild);
  const stageKey = activeBuild ? `${activeBuild.id}:${activeBuild.activeStage}` : undefined;
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
    setStatsOpen(false);
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
    if (next !== route) history.pushState(null, '', routePath(next));
    setRoute(next); setMenuOpen(false); setStatsOpen(false);
  };
  const focusStat = (id: string) => { setCalcsFocus({ id }); navigate('calcs'); };
  const retained = (page: Route) => route === page || !stageChanged && visited.scope === stageKey && visited.pages.includes(page);
  if (session.bootError) return <Recovery lang={lang} error={session.bootError} />;
  if (session.bootMessage || !session.character) return <div className="v2-boot" role="status" aria-busy="true"><div className="v2-brand-mark">P</div><h1>PoBR</h1><p>{bootMessageLabel(lang, session.bootMessage ?? '初始化…')}</p><Skeleton className="h-2 w-56" /></div>;
  const navigation = <><a className="v2-brand" href="/v2" onClick={event => { event.preventDefault(); navigate('overview'); }}><span className="v2-brand-mark">P</span><span><strong>PoBR</strong><small>PATH OF EXILE II</small></span><Badge variant="outline">V2</Badge></a>
    <div className="v2-nav-caption">{w('WORKSPACE', '构筑工作台', '構築工作台')}</div><nav aria-label={t(lang, 'a11y.mainNavigation')}>{ROUTES.map(page => { const Icon = icons[page]; return <Button key={page} variant="ghost" className="v2-nav-link" asChild><a href={routePath(page)} aria-current={page === route ? 'page' : undefined} onClick={event => { if (event.button || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return; event.preventDefault(); navigate(page); }}><Icon /><span>{pageNames[lang][page]}</span>{page === route && <ChevronRight />}</a></Button>; })}</nav>
    <div className="v2-nav-footer"><Button variant="ghost" onClick={() => { setMenuOpen(false); setSettingsOpen(true); }}><Settings2 />{w('Appearance & language', '外观与语言', '外觀與語言')}</Button><a href="/" className="v2-legacy-link"><ArrowLeft />{w('Previous version', '返回旧版', '返回舊版')}</a><p>{w('Local saves · V2 independent copy', '本地存档 · V2 独立副本', '本地存檔 · V2 獨立副本')}</p><small>v{__POBR_APP_VERSION__} · {session.dataVersion}</small></div></>;
  const sidebar = <StatSidebar session={session} lang={lang} onStatClick={focusStat} />;
  return <AppDialogsProvider scope={stageKey}><div className="v2-root">
    <a href="#v2-main" className="v2-skip-link">{t(lang, 'a11y.skipToContent')}</a>
    <aside className="v2-navigation">{navigation}</aside>
    <div className="v2-workspace">
      <header className="v2-topbar">
        <Button className="v2-menu-button" size="icon" variant="ghost" aria-label={t(lang, 'a11y.openMenu')} onClick={() => setMenuOpen(true)}><Menu /></Button>
        <div className="v2-workspace-selectors">
          <label className="v2-workspace-picker"><span>{w('Build', '构筑', '構築')}</span><AppSelectControl aria-label={w('Current build', '当前构筑', '目前構築')} value={activeBuild?.id ?? ''} onChange={e => session.selectStage(e.target.value)}>{session.workspace?.builds.map((build, index) => <option key={build.id} value={build.id}>{build.name || `${w('Build', '构筑', '構築')} ${index + 1}`}</option>)}</AppSelectControl></label>
          <ChevronRight className="v2-picker-divider" />
          <label className="v2-workspace-picker"><span>{w('Stage', '阶段', '階段')}</span><AppSelectControl aria-label={w('Current stage', '当前阶段', '目前階段')} value={activeBuild?.activeStage ?? ''} onChange={e => { if (activeBuild) session.selectStage(activeBuild.id, e.target.value); }}>{activeBuild?.stages.map((stage, index) => <option key={stage.id} value={stage.id}>{stage.name || `${w('Stage', '阶段', '階段')} ${index + 1}`}</option>)}</AppSelectControl></label>
        </div>
        <div className="v2-topbar-actions"><span className={`v2-save-state${session.storageFailed ? ' is-error' : ''}`} role="status">{session.busy ? <LoaderCircle className="animate-spin" /> : <Check />}{session.storageFailed ? w('Save failed', '保存失败', '儲存失敗') : session.busy ? w('Calculating', '计算中', '計算中') : w('Saved locally', '已保存到本地', '已儲存至本地')}</span>
          <Button variant="outline" size="icon" aria-label={w('Character stats', '角色属性', '角色屬性')} onClick={() => setStatsOpen(true)}><PanelRightOpen /></Button>
          <Button variant="ghost" size="icon" aria-label={t(lang, 'common.settings')} onClick={() => setSettingsOpen(true)}><Settings2 /></Button>
        </div>
      </header>
      <main id="v2-main" tabIndex={-1} className={`v2-main v2-page-${route}`} ref={main} key={stageKey}>
        {session.storageFailed && <div className="v2-alert" role="alert"><p>{w('Browser storage is full or unavailable. Download a backup before leaving.', '浏览器存储已满或不可用，请先下载完整备份。', '瀏覽器儲存空間已滿或無法使用，請先下載完整備份。')}</p><Button onClick={() => download(session.exportWorkspace(), 'pobr-v2-backup.json')}><Download />{w('Download backup', '下载备份', '下載備份')}</Button></div>}
        {preferencesFailed && <p className="v2-alert" role="alert">{w('Display preferences could not be saved on this device.', '显示偏好暂时无法保存到当前设备。', '顯示偏好暫時無法儲存至目前裝置。')}</p>}
        {reviewStage?.scope === stageKey && <div className="v2-notice" role="note"><p>{t(lang, 'config.importReview')}</p><Button variant="outline" onClick={() => { setReviewStage(null); navigate('config'); }}>{t(lang, 'config.review')}</Button><Button variant="ghost" onClick={() => setReviewStage(null)}>{t(lang, 'config.reviewDismiss')}</Button></div>}
        {Object.keys(session.editorDrafts).length > 0 && <div className="v2-notice" role="status"><p>{t(lang, 'editor.draftHint')}</p><Button variant="outline" onClick={() => navigate('items')}>{t(lang, 'editor.reviewItems')}</Button><Button variant="outline" onClick={() => navigate('tree')}>{t(lang, 'editor.reviewJewels')}</Button></div>}
        {session.calcError && <div className="v2-alert" role="alert"><strong>{w('Calculation unavailable', '当前计算不可用', '目前計算無法使用')}</strong><p>{session.calcError}</p></div>}
        {session.operationError && <div className="v2-alert" role="alert">{buildImportErrorMessage(session.operationError, lang)}</div>}
        <div className="v2-page-content">
          {route === 'overview' && <Overview session={session} lang={lang} navigate={navigate} focusStat={focusStat} />}
          {route === 'build' && <BuildPanel session={session} lang={lang} onImported={() => { setReviewStage({ scope: null }); navigate('items'); }} />}
          {route === 'items' && <ItemsPanel session={session} lang={lang} onUpgrade={slot => { setUpgradeFocus({ slot, nonce: Date.now() }); navigate('trade'); }} />}
          {route === 'config' && <ConfigPanel session={session} lang={lang} />}
          <Suspense fallback={<div role="status" className="v2-panel-loading"><Skeleton className="h-12 w-2/3" /><Skeleton className="h-64 w-full" /><p>{t(lang, 'ui.loadingPanel')}</p></div>}>
            {route === 'calcs' && <CalcsPanel session={session} lang={lang} focus={stageChanged ? null : calcsFocus} onFocusConsumed={() => setCalcsFocus(null)} />}
            {route === 'compare' && <ComparePanel session={session} lang={lang} />}
            {retained('tree') && <div hidden={route !== 'tree'}><TreePanel session={session} lang={lang} active={route === 'tree'} focusPlanner={stageChanged ? undefined : treeFocus} onJewelSearch={socket => { setUpgradeFocus({ slot: `Jewel@${socket}`, nonce: Date.now(), jewelType: 'radius' }); navigate('trade'); }} /></div>}
            {retained('skills') && <div hidden={route !== 'skills'}><SkillsPanel session={session} lang={lang} active={route === 'skills'} focusOptimizer={stageChanged ? undefined : skillFocus} /></div>}
            {retained('trade') && <div hidden={route !== 'trade'}><TradePanel session={session} lang={lang} active={route === 'trade'} focus={stageChanged ? undefined : upgradeFocus} onSkills={group => { setSkillFocus({ group, nonce: Date.now() }); navigate('skills'); }} onTree={() => { setTreeFocus({ nonce: Date.now() }); navigate('tree'); }} /></div>}
          </Suspense>
        </div>
      </main>
    </div>
    <Sheet open={menuOpen} onOpenChange={setMenuOpen}><SheetContent side="left" className="v2-mobile-navigation"><SheetHeader className="sr-only"><SheetTitle>{t(lang, 'a11y.mainNavigation')}</SheetTitle><SheetDescription>{w('Choose a workspace page', '选择工作台页面', '選擇工作台頁面')}</SheetDescription></SheetHeader>{navigation}</SheetContent></Sheet>
    <Sheet open={statsOpen} onOpenChange={setStatsOpen}><SheetContent className="v2-stat-sheet"><SheetHeader><SheetTitle>{w('Character stats', '角色属性', '角色屬性')}</SheetTitle><SheetDescription>{w('Select a value to inspect its calculation.', '点击数值查看计算明细。', '點擊數值查看計算明細。')}</SheetDescription></SheetHeader>{sidebar}</SheetContent></Sheet>
    <Settings open={settingsOpen} onOpenChange={setSettingsOpen} preferences={preferences} onChange={onPreferences} />
  </div></AppDialogsProvider>;
}
