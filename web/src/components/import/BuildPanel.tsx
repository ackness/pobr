import { chineseScript } from '../../lib/chinese';
import { useAppDialogs } from '../shared/AppDialogs';
import { useSessionAction } from '../shared/useSessionAction';
import { AppButton, AppInput, AppTextarea, AppSelectControl } from '../shared/controls';
import { BuildWorkspace } from './BuildWorkspace';
import { workspaceText } from '../../lib/workspaceText';
import { PageHeader } from '../shared/PageHeader';
import { formatApiError } from '../../api/error';
import { buildFileKind } from '../../api/import';
import { useRef, useState } from 'react';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import { hasPobColorCodes, parsePobColorText } from '../../lib/pobColors';
import { CopyButton } from '../shared/CopyButton';
import './import.css';

interface Props {
  session: BuildSession;
  lang: Lang;
  onImported: () => void;
}

/** Build library, current-stage editing, import and scoped sharing. */
export function BuildPanel({ session, lang, onImported }: Props) {
  const tt = bindT(lang);
  const dialogs = useAppDialogs(lang);
  const captureAction = useSessionAction(session);
  const text = workspaceText(lang);
  const [destination, setDestination] = useState<'newBuild' | 'currentStage'>('newBuild');
  const activeBuild = session.workspace?.builds.find(entry => entry.id === session.workspace?.activeBuild);
  const activeStage = activeBuild?.stages.find(entry => entry.id === activeBuild.activeStage);
  const zhName = (map: Record<string, string>, name: string) =>
    lang !== 'en-US' ? chineseScript(map[name] ?? name, lang) : name;
  const [code, setCode] = useState('');
  const [fileError, setFileError] = useState<string | null>(null);
  const [importRecovery, setImportRecovery] = useState<'file' | 'source' | null>(null);
  const fileRef = useRef<HTMLInputElement | null>(null);
  const [share, setShare] = useState<{ code: string; source: BuildSession['exportCode'] } | null>(null);
  const shareCode = share?.source === session.exportCode ? share.code : null;
  const [shareError, setShareError] = useState<string | null>(null);
  const [generating, setGenerating] = useState(false);
  const [importing, setImporting] = useState(false);

  const generateCode = async () => {
    setShareError(null);
    setGenerating(true);
    try {
      setShare({ code: await session.exportCode(), source: session.exportCode });
    } catch (err) {
      setShareError(formatApiError(err));
    } finally {
      setGenerating(false);
    }
  };

  const exportFile = (content: string, filename: string) => {
    const url = URL.createObjectURL(new Blob([content], { type: 'application/json' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = filename;
    link.click();
    URL.revokeObjectURL(url);
  };

  const [plannerResult, setPlannerResult] = useState<{ value: import('../../api/types').BuildPlannerExport; source: BuildSession['exportBuildFiles'] } | null>(null);
  const plannerFiles = plannerResult?.source === session.exportBuildFiles ? plannerResult.value : null;
  const exportPlanner = async (all: boolean) => {
    setGenerating(true);
    setShareError(null);
    setPlannerResult(null);
    try {
      const result = await session.exportBuildFiles(activeBuild?.name || text.initialBuild, all);
      setPlannerResult({ value: result, source: session.exportBuildFiles });
      if (!all && result.files.length === 1) exportFile(result.files[0].content, result.files[0].filename);
    } catch (err) { setShareError(formatApiError(err)); }
    finally { setGenerating(false); }
  };

  const confirmReplacement = async () => destination !== 'currentStage' || await dialogs.confirm(text.replaceConfirm);

  const importFile = async (file: File) => {
    const current = captureAction();
    setFileError(null);
    setImportRecovery(null);
    setImporting(true);
    try {
      const content = await file.text();
      if (!current()) return;
      const kind = buildFileKind(content);
      if (kind === 'workspace' && !await dialogs.confirm(text.backupConfirm)) return;
      const isSharedBuild = kind === 'session' && JSON.parse(content).format === 'pobr-build';
      if (kind !== 'workspace' && !isSharedBuild && !await confirmReplacement()) return;
      if (!current()) return;
      if (kind === 'external') {
        if (await session.importCode(content.trim(), { destination, name: file.name.replace(/\.[^.]+$/, '') })) onImported();
      } else {
        session.importSession(content, destination);
        onImported();
      }
    } catch (err) {
      setFileError(formatApiError(err));
    } finally {
      setImporting(false);
    }
  };
  const character = session.character!;
  const classes = session.treeMeta?.classes ?? [];
  const currentClass = classes.find((c) => c.name === character.class_name);
  const ascendancies = currentClass?.ascendancies ?? [];

  const doImport = async () => {
    if (!code.trim() || importing || session.busy) return;
    const current = captureAction();
    setImporting(true);
    setImportRecovery(null);
    try {
      if (!await confirmReplacement() || !current()) return;
      if (await session.importCode(code.trim(), { destination })) onImported();
      else setImportRecovery(buildFileKind(code) === 'external' ? 'source' : 'file');
    } finally {
      setImporting(false);
    }
  };

  const notesColored = hasPobColorCodes(session.notes);

  return (
    <section className="ui-page build-page" aria-labelledby="build-heading">
      <PageHeader id="build-heading" title={tt('ui.buildTitle')} description={tt('ui.buildHint')} />
      <div className="build-grid">
        <BuildWorkspace session={session} lang={lang} />
        <p className="build-stage-scope" role="note">{text.stageScope} <strong>{activeBuild?.name || text.initialBuild} / {activeStage?.name || text.initialStage}</strong></p>
        <article className="build-card">
          <h3>{tt('build.character')}</h3>
          <div className="character-form">
            <label>
              {tt('build.class')}
              <AppSelectControl
                value={character.class_name}
                disabled={session.busy}
                onChange={async (e) => {
                  const nextClass = e.target.value;
                  const current = captureAction();
                  if (!character.class_name) { session.setCharacter({ class_name: e.target.value, ascendancy_name: '' }); return; }
                  if (session.hasBuildContent && !await dialogs.confirm(tt('build.confirmClassChange'))) return;
                  if (current()) session.newBuild(nextClass, '');
                }}
              >
                {!character.class_name && <option value="">{tt('build.none')}</option>}
                {classes.map((c) => (
                  <option key={c.name} value={c.name}>
                    {zhName(session.classNames.classes, c.name)}
                  </option>
                ))}
              </AppSelectControl>
            </label>
            <label>
              {tt('build.ascendancy')}
              <AppSelectControl
                value={character.ascendancy_name}
                disabled={session.busy || ascendancies.length === 0}
                onChange={(e) => session.setCharacter({ ascendancy_name: e.target.value })}
              >
                <option value="">{tt('build.none')}</option>
                {ascendancies.map((a) => (
                  <option key={a.id} value={a.name}>
                    {zhName(session.classNames.ascendancies, a.name)}
                  </option>
                ))}
              </AppSelectControl>
            </label>
            <label className="character-level">
              {tt('build.level')}
              <AppInput
                type="number"
                min={1}
                max={100}
                value={character.level}
                disabled={session.busy}
                onChange={(e) => {
                  const level = Number(e.target.value);
                  if (Number.isInteger(level) && level >= 1 && level <= 100) {
                    session.setCharacter({ level });
                  }
                }}
              />
            </label>
          </div>
          <p className="build-card-hint">{tt('build.newHint')}</p>
          {session.build && (
            <p className="import-summary">
              {tt('build.imported')}
              Lv{session.build.character.level}{' '}
              {zhName(
                session.build.character.ascendancy_name
                  ? session.classNames.ascendancies
                  : session.classNames.classes,
                session.build.character.ascendancy_name || session.build.character.class_name,
              )}{' '}
              ·{' '}
              {session.build.tree.allocated_nodes.length} {tt('build.passives')} ·{' '}
              {session.build.items.equipped.length} {tt('build.itemsCount')}
            </p>
          )}
        </article>

        <article className="build-card build-card--notes">
          <h3>{tt('tab.notes')}</h3>
          <p className="build-card-hint">{tt('notes.hint')}</p>
          <AppTextarea
            className="notes-editor"
            value={session.notes}
            placeholder={tt('notes.placeholder2')}
            spellCheck={false}
            aria-label={tt('tab.notes')}
            onChange={(e) => session.setNotes(e.target.value)}
          />
          {notesColored && (
            <div className="notes-preview" aria-label={tt('notes.preview')}>
              <span className="notes-preview-title">{tt('notes.preview')}</span>
              <pre className="notes-preview-body">
                {parsePobColorText(session.notes).map((seg, i) => (
                  <span key={i} style={seg.color ? { color: seg.color } : undefined}>
                    {seg.text}
                  </span>
                ))}
              </pre>
            </div>
          )}
        </article>

        <article className="build-card build-card--import">
          <h3>{tt('build.import')}</h3>
          <p className="build-card-hint">{text.importHint}</p>
          <label className="import-destination">{text.importDestination}
            <AppSelectControl value={destination} disabled={importing || session.busy} onChange={event => setDestination(event.target.value as 'newBuild' | 'currentStage')}>
              <option value="newBuild">{text.importNew}</option>
              <option value="currentStage">{text.importCurrent} · {activeStage?.name || text.initialStage}</option>
            </AppSelectControl>
          </label>
          <p className="build-card-hint" id="import-hint">{tt('build.importHint')}</p>
          <AppTextarea
            className="import-code"
            rows={5}
            placeholder={tt('build.importPlaceholder')}
            value={code}
            onChange={(e) => { setCode(e.target.value); setImportRecovery(null); }}
            spellCheck={false}
            aria-label={tt('build.code')}
            aria-describedby={importRecovery ? 'import-hint import-recovery' : 'import-hint'}
            disabled={importing}
          />
          {importRecovery && (
            <p id="import-recovery" className="build-card-hint" role="status">
              {tt(importRecovery === 'file' ? 'build.importRecoveryFile' : 'build.importRecoverySource')}
            </p>
          )}
          <div className="build-card-actions">
            <AppButton
              className="import-submit"
              onClick={doImport}
              disabled={session.busy || importing || !code.trim()}
            >
              {importing ? tt('build.importing') : destination === 'newBuild' ? tt('build.importButton') : text.importCurrent}
            </AppButton>
            <AppButton onClick={() => fileRef.current?.click()} disabled={session.busy || importing}>
              {tt('save.import')}
            </AppButton>
            <AppInput
              ref={fileRef}
              type="file"
              accept=".json,.build,.txt,application/json,text/plain"
              hidden
              aria-label={tt('save.import')}
              onChange={(e) => {
                const file = e.target.files?.[0];
                if (file) importFile(file);
                e.target.value = '';
              }}
            />
          </div>
          <p className="build-card-hint import-file-hint">{text.importFileHint}</p>
          {fileError && <div className="calc-error" role="alert">{fileError}</div>}
        </article>


        <article className="build-card">
          <h3>{text.exportTitle}</h3>
          <p className="build-card-hint">{text.shareHint}</p>
          <div className="build-card-actions">
            <AppButton disabled={session.busy} onClick={() => exportFile(session.exportLocalBuild(), 'pobr-build-stages.json')}>{text.share}</AppButton>
          </div>
          <h4 className="build-export-heading">{text.currentStageShare}</h4>
          <p className="build-card-hint">{tt('share.hint')}</p>
          <div className="build-card-actions">
            <AppButton onClick={generateCode} disabled={session.busy || generating}>
              {tt(generating ? 'share.generating' : 'share.generate')}
            </AppButton>
            {shareCode && <CopyButton text={shareCode} lang={lang} />}
          </div>
          {shareError && <div className="calc-error" role="alert">{shareError}</div>}
          {share && !shareCode && <p className="build-card-hint" role="status">{tt('share.stale')}</p>}
          {shareCode && (
            <AppTextarea
              className="import-code"
              rows={3}
              readOnly
              value={shareCode}
              spellCheck={false}
              aria-label={tt('share.title')}
              onFocus={(e) => e.target.select()}
            />
          )}

          <h4 className="build-export-heading">{tt('plannerFile.title')}</h4>
          <p className="build-card-hint">{tt('plannerFile.hint')}</p>
          <div className="build-card-actions">
            <AppButton disabled={session.busy || generating} onClick={() => exportPlanner(false)}>{tt('plannerFile.current')}</AppButton>
            <AppButton disabled={session.busy || generating} onClick={() => exportPlanner(true)}>{tt('plannerFile.all')}</AppButton>
          </div>
          {plannerFiles && <div aria-live="polite">
            {plannerFiles.files.map(file => <AppButton key={file.filename} onClick={() => exportFile(file.content, file.filename)}>{file.filename}</AppButton>)}
            {plannerFiles.warnings.map(warning => <p className="build-card-hint" key={warning}>{warning}</p>)}
          </div>}
          <h4 className="build-export-heading">{text.backup}</h4>
          <p className="build-card-hint">{text.backupHint}</p>
          <div className="build-card-actions">
            <AppButton onClick={() => exportFile(session.exportWorkspace(), 'pobr-workspace.json')} disabled={session.busy || importing}>{tt('save.export')}</AppButton>

          </div>
        </article>


      </div>

      {session.calc && session.calc.item_errors.length > 0 && (
        <div className="calc-error">
          {session.calc.item_errors.map((e) => (
            <div key={e.slot}>
              [{e.slot}] {tt('build.itemError')}: {e.message}
            </div>
          ))}
        </div>
      )}
      {session.calc && session.calc.unsupported_modifiers.length > 0 && (
        <details className="unsupported-block">
          <summary>
            {tt('build.unsupported')}（{session.calc.unsupported_modifiers.length}）
          </summary>
          <p className="hint">{tt('build.unsupportedHint')}</p>
          <ul>
            {session.calc.unsupported_modifiers.map((text, i) => (
              <li key={i}>{text}</li>
            ))}
          </ul>
        </details>
      )}
    </section>
  );
}
