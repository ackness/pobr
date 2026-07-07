import { useState } from 'react';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { Lang } from '../../lib/statDisplay';
import './import.css';

interface Props {
  session: BuildSession;
  lang: Lang;
  onImported: () => void;
}

export function ImportPanel({ session, lang, onImported }: Props) {
  const [code, setCode] = useState('');
  const zh = lang === 'zh-TW';

  const doImport = async () => {
    if (!code.trim()) return;
    await session.importCode(code.trim());
    onImported();
  };

  return (
    <section className="import-panel" aria-labelledby="import-heading">
      <h2 id="import-heading">{zh ? '匯入 Build Code' : 'Import Build Code'}</h2>
      <p className="import-hint">
        {zh
          ? '貼上 PoB2 分享代碼（Path of Building → Import/Export Build → Generate）。'
          : 'Paste a PoB2 share code (Path of Building → Import/Export Build → Generate).'}
      </p>
      <textarea
        className="import-code"
        rows={8}
        placeholder={zh ? '在此貼上 Build Code…' : 'Paste build code here…'}
        value={code}
        onChange={(e) => setCode(e.target.value)}
        spellCheck={false}
        aria-label="Build code"
      />
      <div className="import-actions">
        <button className="import-submit" onClick={doImport} disabled={session.busy || !code.trim()}>
          {session.busy ? (zh ? '計算中…' : 'Calculating…') : zh ? '匯入' : 'Import'}
        </button>
      </div>
      {session.build && (
        <p className="import-summary">
          {zh ? '已匯入：' : 'Imported: '}
          Lv{session.build.character.level}{' '}
          {session.build.character.ascendancy_name || session.build.character.class_name} ·{' '}
          {session.build.tree.allocated_nodes.length} {zh ? '天賦點' : 'passives'} ·{' '}
          {session.build.items.equipped.length} {zh ? '件裝備' : 'items'}
        </p>
      )}
      {session.calc && session.calc.unsupported_modifiers.length > 0 && (
        <details className="unsupported-block">
          <summary>
            {zh
              ? `未支援詞條（${session.calc.unsupported_modifiers.length}）`
              : `Unsupported modifiers (${session.calc.unsupported_modifiers.length})`}
          </summary>
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
