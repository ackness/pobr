import { useRef, useState } from 'react';
import { Check, FileUp, Plus } from 'lucide-react';
import { buildFileKind } from '../api/import';
import { formatApiError } from '../api/error';
import type { BuildPlannerExport } from '../api/types';
import { Button } from '../components/ui/button';
import { AppInput, AppSelectControl, AppTextarea } from '../components/shared/controls';
import { useAppDialogs } from '../components/shared/AppDialogs';
import { CopyButton } from '../components/shared/CopyButton';
import { useSessionAction } from '../components/shared/useSessionAction';
import type { BuildSession } from '../hooks/useBuildSession';
import { chineseScript } from '../lib/chinese';
import { bindT, type Lang } from '../lib/i18n';
import { hasPobColorCodes, parsePobColorText } from '../lib/pobColors';
import { workspaceText } from '../lib/workspaceText';
import { words } from './text';

type Destination = 'newBuild' | 'currentStage';
type Naming = 'newBuild' | 'copyStage' | 'renameBuild' | 'renameStage';

function download(content: string, filename: string) {
  const url = URL.createObjectURL(new Blob([content], { type: 'application/json' }));
  const link = document.createElement('a'); link.href = url; link.download = filename; link.click();
  URL.revokeObjectURL(url);
}

/**
 * Where a build comes from and which one you are working on. Importing is the first thing anyone
 * does here, so it leads; everything else is arranged by how often it is needed afterwards.
 */
export function BuildPage({ session, lang, onImported }: { session: BuildSession; lang: Lang; onImported: () => void }) {
  const tt = bindT(lang);
  const text = workspaceText(lang);
  const w = (en: string, cn: string, tw?: string) => words(lang, en, cn, tw);
  const dialogs = useAppDialogs(lang);
  const captureAction = useSessionAction(session);
  const workspace = session.workspace;
  const build = workspace?.builds.find(entry => entry.id === workspace.activeBuild);
  const stage = build?.stages.find(entry => entry.id === build.activeStage);
  const character = session.character!;
  const classes = session.treeMeta?.classes ?? [];
  const ascendancies = classes.find(entry => entry.name === character.class_name)?.ascendancies ?? [];
  const zhName = (map: Record<string, string>, name: string) => lang !== 'en-US' ? chineseScript(map[name] ?? name, lang) : name;
  const className = (raw: string) => zhName({ ...session.classNames.classes, ...session.classNames.ascendancies }, raw);

  // Import
  const [destination, setDestination] = useState<Destination>('newBuild');
  const [code, setCode] = useState('');
  const [importing, setImporting] = useState(false);
  const [recovery, setRecovery] = useState<'file' | 'source' | null>(null);
  const [fileError, setFileError] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement | null>(null);
  const confirmReplacement = async () => destination !== 'currentStage' || await dialogs.confirm(text.replaceConfirm);
  const doImport = async () => {
    if (!code.trim() || importing || session.busy) return;
    const current = captureAction();
    setImporting(true); setRecovery(null);
    try {
      if (!await confirmReplacement() || !current()) return;
      if (await session.importCode(code.trim(), { destination })) onImported();
      else setRecovery(buildFileKind(code) === 'external' ? 'source' : 'file');
    } finally { setImporting(false); }
  };
  const importFile = async (file: File) => {
    const current = captureAction();
    setFileError(null); setRecovery(null); setImporting(true);
    try {
      const content = await file.text();
      if (!current()) return;
      const kind = buildFileKind(content);
      if (kind === 'workspace' && !await dialogs.confirm(text.backupConfirm)) return;
      const isSharedBuild = kind === 'session' && JSON.parse(content).format === 'pobr-build';
      if (kind !== 'workspace' && !isSharedBuild && !await confirmReplacement()) return;
      if (!current()) return;
      if (kind === 'external') { if (await session.importCode(content.trim(), { destination, name: file.name.replace(/\.[^.]+$/, '') })) onImported(); }
      else { session.importSession(content, destination); onImported(); }
    } catch (error) { setFileError(formatApiError(error)); }
    finally { setImporting(false); }
  };

  // Naming a build or stage
  const [naming, setNaming] = useState<Naming | null>(null);
  const [name, setName] = useState('');
  const beginNaming = (kind: Naming) => { setNaming(kind); setName(kind === 'renameBuild' ? build?.name ?? '' : kind === 'renameStage' ? stage?.name ?? '' : ''); };
  const submitName = (event: React.FormEvent) => {
    event.preventDefault();
    if (!naming || !name.trim() || session.busy) return;
    if (naming === 'newBuild') session.createLocalBuild(name);
    else if (naming === 'copyStage') session.duplicateStage(name);
    else session.renameWorkspace(naming === 'renameBuild' ? 'build' : 'stage', name);
    setNaming(null);
  };
  const nameForm = (kinds: Naming[]) => naming && kinds.includes(naming) && <form className="bd-name-form" onSubmit={submitName}>
    <label>{text[naming]}<AppInput autoFocus aria-label={text.name} value={name} maxLength={100} onChange={event => setName(event.target.value)}
      placeholder={naming === 'copyStage' ? `${text.start} / ${text.progress} / ${text.endgame}` : undefined} /></label>
    <Button type="submit" disabled={!name.trim() || session.busy}>{text.save}</Button>
    <Button type="button" variant="ghost" onClick={() => setNaming(null)}>{text.cancel}</Button>
  </form>;

  // Loadouts that came with an imported PoB code
  const confirmLoadout = async () => !session.isDirty || await dialogs.confirm(tt('loadout.confirmDiscard'));
  const manageLoadout = async (op: 'duplicate' | 'rename' | 'remove') => {
    const valid = captureAction();
    if (!await confirmLoadout() || !valid()) return;
    if (op === 'remove') { if (await dialogs.confirm(tt('loadout.confirmRemove')) && valid()) void session.manageLoadout(op); return; }
    const current = session.loadouts[session.activeLoadout ?? 0]?.name ?? '';
    const next = await dialogs.prompt(tt('loadout.namePrompt'), op === 'rename' ? current : '');
    if (next?.trim() && valid()) void session.manageLoadout(op, next.trim());
  };

  // Export
  const [generating, setGenerating] = useState(false);
  const [shareError, setShareError] = useState<string | null>(null);
  const [share, setShare] = useState<{ code: string; source: BuildSession['exportCode'] } | null>(null);
  const shareCode = share?.source === session.exportCode ? share.code : null;
  const [planner, setPlanner] = useState<{ value: BuildPlannerExport; source: BuildSession['exportBuildFiles'] } | null>(null);
  const plannerFiles = planner?.source === session.exportBuildFiles ? planner.value : null;
  const generateCode = async () => {
    setShareError(null); setGenerating(true);
    try { setShare({ code: await session.exportCode(), source: session.exportCode }); } catch (error) { setShareError(formatApiError(error)); } finally { setGenerating(false); }
  };
  const exportPlanner = async (all: boolean) => {
    setGenerating(true); setShareError(null); setPlanner(null);
    try {
      const result = await session.exportBuildFiles(build?.name || text.initialBuild, all);
      setPlanner({ value: result, source: session.exportBuildFiles });
      if (!all && result.files.length === 1) download(result.files[0].content, result.files[0].filename);
    } catch (error) { setShareError(formatApiError(error)); } finally { setGenerating(false); }
  };

  const destinations: [Destination, string][] = [['newBuild', text.importNew], ['currentStage', `${text.importCurrent} · ${stage?.name || text.initialStage}`]];
  return <section className="bd" aria-labelledby="build-heading">
    <header className="bd-head"><h1 id="build-heading">{w('My builds', '我的 BD', '我的 BD')}</h1><p>{tt('ui.buildHint')}</p></header>

    <section className="bd-sec bd-import" aria-labelledby="bd-import-title">
      <h2 id="bd-import-title">{w('Import a build', '导入 BD', '匯入 BD')}</h2>
      <AppTextarea className="import-code" rows={4} spellCheck={false} disabled={importing} value={code} placeholder={tt('build.importPlaceholder')} aria-label={tt('build.code')}
        aria-describedby={recovery ? 'bd-import-hint bd-import-recovery' : 'bd-import-hint'} onChange={event => { setCode(event.target.value); setRecovery(null); }} />
      <div className="bd-import-actions">
        <Button className="import-submit" onClick={doImport} disabled={session.busy || importing || !code.trim()}>
          {importing ? tt('build.importing') : destination === 'newBuild' ? tt('build.importButton') : text.importCurrent}</Button>
        <Button variant="outline" onClick={() => fileRef.current?.click()} disabled={session.busy || importing}><FileUp />{tt('save.import')}</Button>
        <AppInput ref={fileRef} type="file" accept=".json,.build,.txt,application/json,text/plain" hidden aria-label={tt('save.import')}
          onChange={event => { const file = event.target.files?.[0]; if (file) void importFile(file); event.target.value = ''; }} />
        <div className="bd-segment" role="group" aria-label={text.importDestination}>
          {destinations.map(([value, label]) => <button key={value} type="button" aria-pressed={destination === value} disabled={importing || session.busy} onClick={() => setDestination(value)}>{label}</button>)}
        </div>
      </div>
      <p className="bd-hint" id="bd-import-hint">{tt('build.importHint')} {text.importFileHint}</p>
      {recovery && <p className="bd-hint" id="bd-import-recovery" role="status">{tt(recovery === 'file' ? 'build.importRecoveryFile' : 'build.importRecoverySource')}</p>}
      {fileError && <p className="bd-error" role="alert">{fileError}</p>}
    </section>

    <section className="bd-sec" aria-labelledby="bd-library-title">
      <div className="bd-sec-head"><h2 id="bd-library-title">{text.libraryTitle}</h2>
        <div className="bd-actions"><Button variant="ghost" size="sm" disabled={session.busy} onClick={() => beginNaming('newBuild')}><Plus />{text.newBuild}</Button>
          <Button variant="ghost" size="sm" disabled={session.busy} onClick={() => beginNaming('renameBuild')}>{text.renameBuild}</Button></div></div>
      {nameForm(['newBuild', 'renameBuild'])}
      <ul className="bd-list" role="group" aria-label={text.libraryTitle}>
        {workspace?.builds.map(entry => {
          const active = entry.stages.find(candidate => candidate.id === entry.activeStage)!;
          const state = active.saved.state.character;
          const current = entry.id === build?.id;
          return <li key={entry.id}><button type="button" aria-pressed={current} disabled={session.busy} onClick={() => session.selectStage(entry.id)}>
            <span className="bd-row-mark">{current && <Check />}</span>
            <strong>{entry.name || text.initialBuild}</strong>
            <span>Lv{state.level} {className(state.ascendancy_name || state.class_name)}</span>
            <small>{entry.stages.length} {text.stages}</small></button></li>;
        })}
      </ul>
    </section>

    <section className="bd-sec" aria-labelledby="bd-stage-title">
      <div className="bd-sec-head"><div><h2 id="bd-stage-title">{text.stagesTitle}</h2><p className="bd-hint">{text.stageScope} <strong>{build?.name || text.initialBuild} / {stage?.name || text.initialStage}</strong></p></div>
        <div className="bd-actions"><Button variant="ghost" size="sm" disabled={session.busy} onClick={() => beginNaming('copyStage')}>{text.copyStage}</Button>
          <Button variant="ghost" size="sm" disabled={session.busy} onClick={() => beginNaming('renameStage')}>{text.renameStage}</Button></div></div>
      {nameForm(['copyStage', 'renameStage'])}
      <ul className="bd-chips" role="group" aria-label={text.stagesTitle}>
        {build?.stages.map(entry => <li key={entry.id}><button type="button" aria-pressed={entry.id === stage?.id} disabled={session.busy} onClick={() => session.selectStage(build.id, entry.id)}>
          {entry.name || text.initialStage}<small>Lv{entry.saved.state.character.level} · {entry.saved.state.allocatedNodes.length} {tt('build.passives')}</small></button></li>)}
      </ul>
      {session.build && session.loadouts.length > 0 && <details className="bd-details"><summary>{text.importedLoadouts}</summary>
        <p className="bd-hint">{text.importedLoadoutsHint}</p>
        <div className="bd-inline">
          <label>{tt('loadout.switch')}<AppSelectControl aria-label={tt('loadout.switch')} value={session.activeLoadout ?? ''} disabled={session.busy} onChange={async event => {
            const valid = captureAction(); const loadout = session.loadouts[Number(event.target.value)];
            if (loadout && await confirmLoadout() && valid()) void session.switchLoadout({ tree: loadout.tree, item: loadout.item, skill: loadout.skill, config: loadout.config });
          }}>{session.activeLoadout === null && <option value="">—</option>}{session.loadouts.map((loadout, index) => <option key={index} value={index}>{loadout.name}</option>)}</AppSelectControl></label>
          <Button variant="ghost" size="sm" disabled={session.busy} onClick={() => manageLoadout('duplicate')}>{tt('loadout.new')}</Button>
          <Button variant="ghost" size="sm" disabled={session.busy} onClick={() => manageLoadout('rename')}>{tt('loadout.rename')}</Button>
          {session.loadouts.length > 1 && <Button variant="ghost" size="sm" disabled={session.busy} onClick={() => manageLoadout('remove')}>{tt('loadout.remove')}</Button>}
        </div></details>}

      <h3 className="bd-subtitle">{tt('build.character')}</h3>
      <div className="bd-character">
        <label>{tt('build.class')}<AppSelectControl value={character.class_name} disabled={session.busy} onChange={async event => {
          const next = event.target.value; const current = captureAction();
          if (!character.class_name) { session.setCharacter({ class_name: next, ascendancy_name: '' }); return; }
          if (session.hasBuildContent && !await dialogs.confirm(tt('build.confirmClassChange'))) return;
          if (current()) session.newBuild(next, '');
        }}>{!character.class_name && <option value="">{tt('build.none')}</option>}{classes.map(entry => <option key={entry.name} value={entry.name}>{zhName(session.classNames.classes, entry.name)}</option>)}</AppSelectControl></label>
        <label>{tt('build.ascendancy')}<AppSelectControl value={character.ascendancy_name} disabled={session.busy || ascendancies.length === 0} onChange={event => session.setCharacter({ ascendancy_name: event.target.value })}>
          <option value="">{tt('build.none')}</option>{ascendancies.map(entry => <option key={entry.id} value={entry.name}>{zhName(session.classNames.ascendancies, entry.name)}</option>)}</AppSelectControl></label>
        <label className="bd-level">{tt('build.level')}<AppInput type="number" min={1} max={100} value={character.level} disabled={session.busy}
          onChange={event => { const level = Number(event.target.value); if (Number.isInteger(level) && level >= 1 && level <= 100) session.setCharacter({ level }); }} /></label>
      </div>
      <p className="bd-hint">{tt('build.newHint')}</p>

      <h3 className="bd-subtitle">{tt('tab.notes')}</h3>
      <AppTextarea className="notes-editor" rows={4} spellCheck={false} value={session.notes} placeholder={tt('notes.placeholder2')} aria-label={tt('tab.notes')} onChange={event => session.setNotes(event.target.value)} />
      {hasPobColorCodes(session.notes) && <div className="notes-preview" aria-label={tt('notes.preview')}><pre className="notes-preview-body">{parsePobColorText(session.notes).map((segment, index) => <span key={index} style={segment.color ? { color: segment.color } : undefined}>{segment.text}</span>)}</pre></div>}
    </section>

    <details className="bd-sec bd-export"><summary><h2>{text.exportTitle}</h2></summary>
      <p className="bd-hint">{text.shareHint}</p>
      <div className="bd-actions bd-actions-start"><Button variant="outline" disabled={session.busy} onClick={() => download(session.exportLocalBuild(), 'pobr-build-stages.json')}>{text.share}</Button></div>

      <h3 className="bd-subtitle">{text.currentStageShare}</h3>
      <p className="bd-hint">{tt('share.hint')}</p>
      <div className="bd-actions bd-actions-start"><Button variant="outline" onClick={generateCode} disabled={session.busy || generating}>{tt(generating ? 'share.generating' : 'share.generate')}</Button>{shareCode && <CopyButton text={shareCode} lang={lang} />}</div>
      {shareError && <p className="bd-error" role="alert">{shareError}</p>}
      {share && !shareCode && <p className="bd-hint" role="status">{tt('share.stale')}</p>}
      {shareCode && <AppTextarea className="import-code" rows={3} readOnly value={shareCode} spellCheck={false} aria-label={tt('share.title')} onFocus={event => event.target.select()} />}

      <h3 className="bd-subtitle">{tt('plannerFile.title')}</h3>
      <p className="bd-hint">{tt('plannerFile.hint')}</p>
      <div className="bd-actions bd-actions-start"><Button variant="outline" disabled={session.busy || generating} onClick={() => exportPlanner(false)}>{tt('plannerFile.current')}</Button>
        <Button variant="outline" disabled={session.busy || generating} onClick={() => exportPlanner(true)}>{tt('plannerFile.all')}</Button></div>
      {plannerFiles && <div aria-live="polite" className="bd-actions bd-actions-start">{plannerFiles.files.map(file => <Button key={file.filename} variant="ghost" onClick={() => download(file.content, file.filename)}>{file.filename}</Button>)}
        {plannerFiles.warnings.map(warning => <p className="bd-hint" key={warning}>{warning}</p>)}</div>}

      <h3 className="bd-subtitle">{text.backup}</h3>
      <p className="bd-hint">{text.backupHint}</p>
      <div className="bd-actions bd-actions-start"><Button variant="outline" onClick={() => download(session.exportWorkspace(), 'pobr-workspace.json')} disabled={session.busy || importing}>{tt('save.export')}</Button></div>
    </details>
  </section>;
}
