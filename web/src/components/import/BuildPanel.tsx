import { useState } from 'react';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { Lang } from '../../lib/statDisplay';
import './import.css';

interface Props {
  session: BuildSession;
  lang: Lang;
  onImported: () => void;
}

/** Build 页（PoB2 语义）：角色身份编辑 + 新建 + 一键导入 build code。 */
export function BuildPanel({ session, lang, onImported }: Props) {
  const [code, setCode] = useState('');
  const zh = lang === 'zh-TW';
  const character = session.character!;
  const classes = session.treeMeta?.classes ?? [];
  const currentClass = classes.find((c) => c.name === character.class_name);
  const ascendancies = currentClass?.ascendancies ?? [];

  const doImport = async () => {
    if (!code.trim()) return;
    await session.importCode(code.trim());
    onImported();
  };

  return (
    <section className="import-panel" aria-labelledby="build-heading">
      <h2 id="build-heading">{zh ? '角色' : 'Character'}</h2>
      <div className="character-form">
        <label>
          {zh ? '職業' : 'Class'}
          <select
            value={character.class_name}
            disabled={session.busy}
            onChange={(e) => session.newBuild(e.target.value, '')}
          >
            {classes.map((c) => (
              <option key={c.name} value={c.name}>
                {c.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          {zh ? '升華' : 'Ascendancy'}
          <select
            value={character.ascendancy_name}
            disabled={session.busy || ascendancies.length === 0}
            onChange={(e) => session.setCharacter({ ascendancy_name: e.target.value })}
          >
            <option value="">{zh ? '（無）' : '(none)'}</option>
            {ascendancies.map((a) => (
              <option key={a.id} value={a.name}>
                {a.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          {zh ? '等級' : 'Level'}
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
      <p className="import-hint">
        {zh
          ? '切換職業會開一個全新空 build（樹加點在 Tree 頁點選）；下方可一鍵導入 PoB2 code 替換全部內容。'
          : 'Switching class starts a fresh empty build (allocate passives on the Tree tab). Import a PoB2 code below to replace everything.'}
      </p>

      <h2>{zh ? '匯入 Build Code' : 'Import Build Code'}</h2>
      <textarea
        className="import-code"
        rows={6}
        placeholder={zh ? '在此貼上 PoB2 Build Code…' : 'Paste a PoB2 build code here…'}
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
