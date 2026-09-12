import { PageHeader } from '../shared/PageHeader';
import { formatApiError } from '../../api/error';
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

/** Build 页（PoB2 语义）：角色身份 + 总览笔记 + 导入/分享/存档，卡片式布局。 */
export function BuildPanel({ session, lang, onImported }: Props) {
  const tt = bindT(lang);
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

  const exportFile = () => {
    const blob = new Blob([session.exportSession()], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'pobr-build.json';
    a.click();
    URL.revokeObjectURL(url);
  };

  const importFile = async (file: File) => {
    setFileError(null);
    setImporting(true);
    try {
      const text = await file.text();
      try {
        session.importSession(text);
        onImported();
      } catch {
        if (await session.importCode(text.trim())) onImported();
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
    setImporting(true);
    try {
      if (await session.importCode(code.trim())) onImported();
    } finally {
      setImporting(false);
    }
  };

  const notesColored = hasPobColorCodes(session.notes);

  return (
    <section className="ui-page build-page" aria-labelledby="build-heading">
      <PageHeader id="build-heading" title={tt('ui.buildTitle')} description={tt('ui.buildHint')} />
      <div className="build-grid">
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

        <article className="build-card build-card--import">
          <h3>{tt('build.import')}</h3>
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
              {importing ? tt('build.importing') : tt('build.importButton')}
            </button>
          </div>
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

        <article className="build-card">
          <h3>{tt('share.title')}</h3>
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

          <h3 className="build-card-divide">{tt('save.title')}</h3>
          <p className="build-card-hint">{tt('save.hint')}</p>
          <div className="build-card-actions">
            <button onClick={exportFile} disabled={session.busy || importing}>{tt('save.export')}</button>
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
          {fileError && <div className="calc-error" role="alert">{fileError}</div>}
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
