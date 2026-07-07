import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import './notes.css';

interface Props {
  session: BuildSession;
  lang: Lang;
}

/** Notes 页：自由文本笔记——本地持久化（localStorage），导入 build 时带入其 `<Notes>`。 */
export function NotesPanel({ session, lang }: Props) {
  const tt = bindT(lang);
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
    </section>
  );
}
