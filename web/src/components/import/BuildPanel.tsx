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
  const text = workspaceText(lang);
  const [destination, setDestination] = useState<'newBuild' | 'currentStage'>('newBuild');
  const activeBuild = session.workspace?.builds.find(entry => entry.id === session.workspace?.activeBuild);
  const activeStage = activeBuild?.stages.find(entry => entry.id === activeBuild.activeStage);
  const zhName = (map: Record<string, string>, name: string) =>
    lang !== 'en-US' ? (map[name] ?? name) : name;
  const [code, setCode] = useState('');
  const [fileError, setFileError] = useState<string | null>(null);
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

  const confirmReplacement = () => destination !== 'currentStage' || window.confirm(text.replaceConfirm);

  const importFile = async (file: File) => {
    setFileError(null);
    setImporting(true);
    try {
      const content = await file.text();
      const kind = buildFileKind(content);
      if (kind === 'workspace' && !window.confirm(text.backupConfirm)) return;
      const isSharedBuild = kind === 'session' && JSON.parse(content).format === 'pobr-build';
      if (kind !== 'workspace' && !isSharedBuild && !confirmReplacement()) return;
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
    if (!code.trim() || importing || session.busy || !confirmReplacement()) return;
    setImporting(true);
    try {
      if (await session.importCode(code.trim(), { destination })) onImported();
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
              <select
                value={character.class_name}
                disabled={session.busy}
                onChange={(e) => {
                  if (session.hasBuildContent && !window.confirm(tt('build.confirmClassChange'))) return;
                  session.newBuild(e.target.value, '');
                }}
              >
                {classes.map((c) => (
                  <option key={c.name} value={c.name}>
                    {zhName(session.classNames.classes, c.name)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {tt('build.ascendancy')}
              <select
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
              </select>
            </label>
            <label className="character-level">
              {tt('build.level')}
              <input
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
          <textarea
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
            <select value={destination} disabled={importing || session.busy} onChange={event => setDestination(event.target.value as 'newBuild' | 'currentStage')}>
              <option value="newBuild">{text.importNew}</option>
              <option value="currentStage">{text.importCurrent} · {activeStage?.name || text.initialStage}</option>
            </select>
          </label>
          <p className="build-card-hint" id="import-hint">{tt('build.importHint')}</p>
          <textarea
            className="import-code"
            rows={5}
            placeholder={tt('build.importPlaceholder')}
            value={code}
            onChange={(e) => setCode(e.target.value)}
            spellCheck={false}
            aria-label={tt('build.code')}
            aria-describedby="import-hint"
            disabled={importing}
          />
          <div className="build-card-actions">
            <button
              className="import-submit"
              onClick={doImport}
              disabled={session.busy || importing || !code.trim()}
            >
              {importing ? tt('build.importing') : destination === 'newBuild' ? tt('build.importButton') : text.importCurrent}
            </button>
            <button onClick={() => fileRef.current?.click()} disabled={session.busy || importing}>
              {tt('save.import')}
            </button>
            <input
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
            <button disabled={session.busy} onClick={() => exportFile(session.exportLocalBuild(), 'pobr-build-stages.json')}>{text.share}</button>
          </div>
          <h4 className="build-export-heading">{text.currentStageShare}</h4>
          <p className="build-card-hint">{tt('share.hint')}</p>
          <div className="build-card-actions">
            <button onClick={generateCode} disabled={session.busy || generating}>
              {tt(generating ? 'share.generating' : 'share.generate')}
            </button>
            {shareCode && <CopyButton text={shareCode} lang={lang} />}
          </div>
          {shareError && <div className="calc-error" role="alert">{shareError}</div>}
          {share && !shareCode && <p className="build-card-hint" role="status">{tt('share.stale')}</p>}
          {shareCode && (
            <textarea
              className="import-code"
              rows={3}
              readOnly
              value={shareCode}
              spellCheck={false}
              aria-label={tt('share.title')}
              onFocus={(e) => e.target.select()}
            />
          )}

          <h4 className="build-export-heading">{text.backup}</h4>
          <p className="build-card-hint">{text.backupHint}</p>
          <div className="build-card-actions">
            <button onClick={() => exportFile(session.exportWorkspace(), 'pobr-workspace.json')} disabled={session.busy || importing}>{tt('save.export')}</button>

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
