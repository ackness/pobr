import { useState } from 'react';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import './import.css';

interface Props {
  session: BuildSession;
  lang: Lang;
  onImported: () => void;
}

/** Build 页（PoB2 语义）：角色身份编辑 + 新建 + 一键导入 build code。 */
export function BuildPanel({ session, lang, onImported }: Props) {
  const tt = bindT(lang);
  const [code, setCode] = useState('');
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
      <h2 id="build-heading">{tt('build.character')}</h2>
      <div className="character-form">
        <label>
          {tt('build.class')}
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
          {tt('build.ascendancy')}
          <select
            value={character.ascendancy_name}
            disabled={session.busy || ascendancies.length === 0}
            onChange={(e) => session.setCharacter({ ascendancy_name: e.target.value })}
          >
            <option value="">{tt('build.none')}</option>
            {ascendancies.map((a) => (
              <option key={a.id} value={a.name}>
                {a.name}
              </option>
            ))}
          </select>
        </label>
        <label>
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
      <p className="import-hint">{tt('build.newHint')}</p>

      <h2>{tt('build.import')}</h2>
      <textarea
        className="import-code"
        rows={6}
        placeholder={tt('build.importPlaceholder')}
        value={code}
        onChange={(e) => setCode(e.target.value)}
        spellCheck={false}
        aria-label="Build code"
      />
      <div className="import-actions">
        <button className="import-submit" onClick={doImport} disabled={session.busy || !code.trim()}>
          {session.busy ? tt('build.calculating') : tt('build.importButton')}
        </button>
      </div>
      {session.build && (
        <p className="import-summary">
          {tt('build.imported')}
          Lv{session.build.character.level}{' '}
          {session.build.character.ascendancy_name || session.build.character.class_name} ·{' '}
          {session.build.tree.allocated_nodes.length} {tt('build.passives')} ·{' '}
          {session.build.items.equipped.length} {tt('build.itemsCount')}
        </p>
      )}
      {session.calc && session.calc.unsupported_modifiers.length > 0 && (
        <details className="unsupported-block">
          <summary>
            {tt('build.unsupported')}（{session.calc.unsupported_modifiers.length}）
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
