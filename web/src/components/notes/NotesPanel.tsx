import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import { hasPobColorCodes, parsePobColorText } from '../../lib/pobColors';
import './notes.css';

interface Props {
  session: BuildSession;
  lang: Lang;
}

/** Notes 页：自由文本笔记——本地持久化（localStorage），导入 build 时带入其 `<Notes>`。
 *  含 PoB 颜色码（`^N` / `^xRRGGBB`）时在编辑器下方附着色预览。 */
export function NotesPanel({ session, lang }: Props) {
  const tt = bindT(lang);
  const colored = hasPobColorCodes(session.notes);
  return (
    <section className="notes-panel" aria-labelledby="notes-heading">
      <h2 id="notes-heading" className="panel-heading">
        {tt('tab.notes')}
      </h2>
      <p className="notes-hint">{tt('notes.hint')}</p>
      <textarea
        className="notes-editor"
        value={session.notes}
        placeholder={tt('notes.placeholder2')}
        spellCheck={false}
        aria-label={tt('tab.notes')}
        onChange={(e) => session.setNotes(e.target.value)}
      />
      {colored && (
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
    </section>
  );
}
