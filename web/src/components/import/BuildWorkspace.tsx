import { useState } from 'react';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import { workspaceText } from '../../lib/workspaceText';
import './workspace.css';

export function WorkspaceSwitcher({ session, lang, onManage }: { session: BuildSession; lang: Lang; onManage: () => void }) {
  const text = workspaceText(lang);
  const workspace = session.workspace;
  if (!workspace) return null;
  const build = workspace.builds.find(entry => entry.id === workspace.activeBuild)!;
  const stage = build.stages.find(entry => entry.id === build.activeStage)!;
  return <div className="workspace-switcher" aria-label={text.currentBuild}>
    <button onClick={onManage}>{text.manage}</button>
    <strong className="workspace-current-name">{build.name || text.initialBuild}</strong>
    {build.stages.length > 1 ? <label>{text.stages}<select aria-label={text.stages} value={build.activeStage} disabled={session.busy} onChange={event => session.selectStage(build.id, event.target.value)}>
      {build.stages.map(entry => <option key={entry.id} value={entry.id}>{entry.name || text.initialStage}</option>)}
    </select></label> : <span className="workspace-current-stage">{stage.name || text.initialStage}</span>}
    <span className={session.storageFailed ? 'workspace-save-error' : 'workspace-saved'} role="status">{session.storageFailed ? text.failed : text.saved}</span>
  </div>;
}

export function BuildWorkspace({ session, lang }: { session: BuildSession; lang: Lang }) {
  const text = workspaceText(lang);
  const tt = bindT(lang);
  const [action, setAction] = useState<'newBuild' | 'copyStage' | 'renameBuild' | 'renameStage' | null>(null);
  const [name, setName] = useState('');
  const workspace = session.workspace;
  if (!workspace) return null;
  const build = workspace.builds.find(entry => entry.id === workspace.activeBuild)!;
  const stage = build.stages.find(entry => entry.id === build.activeStage)!;
  const begin = (kind: NonNullable<typeof action>) => {
    setAction(kind);
    setName(kind === 'renameBuild' ? build.name : kind === 'renameStage' ? stage.name : '');
  };
  const nameForm = (scope: 'build' | 'stage') => action && (scope === 'build' ? action === 'newBuild' || action === 'renameBuild' : action === 'copyStage' || action === 'renameStage') && <form className="workspace-form" onSubmit={event => {
    event.preventDefault();
    if (!name.trim() || session.busy) return;
    if (action === 'newBuild') session.createLocalBuild(name);
    else if (action === 'copyStage') session.duplicateStage(name);
    else session.renameWorkspace(action === 'renameBuild' ? 'build' : 'stage', name);
    setAction(null);
  }}>
    <label>{text[action]}<input autoFocus aria-label={text.name} value={name} maxLength={100} placeholder={action === 'copyStage' ? `${text.start} / ${text.progress} / ${text.endgame}` : undefined} onChange={event => setName(event.target.value)} /></label>
    <button type="submit" disabled={!name.trim() || session.busy}>{text.save}</button>
    <button type="button" onClick={() => setAction(null)}>{text.cancel}</button>
  </form>;
  const confirmLoadout = () => !session.isDirty || window.confirm(tt('loadout.confirmDiscard'));
  const manageLoadout = (op: 'duplicate' | 'rename' | 'remove') => {
    if (!confirmLoadout()) return;
    if (op === 'remove') {
      if (window.confirm(tt('loadout.confirmRemove'))) void session.manageLoadout(op);
      return;
    }
    const current = session.loadouts[session.activeLoadout ?? 0]?.name ?? '';
    const nextName = window.prompt(tt('loadout.namePrompt'), op === 'rename' ? current : '');
    if (nextName?.trim()) void session.manageLoadout(op, nextName.trim());
  };
  const className = (raw: string) => lang === 'en-US' ? raw : session.classNames.ascendancies[raw] ?? session.classNames.classes[raw] ?? raw;
  return <div className="workspace-management">
    <article className="build-card workspace-library" aria-labelledby="workspace-library-heading">
      <h3 id="workspace-library-heading">{text.libraryTitle}</h3>
      <p className="build-card-hint">{text.libraryHint}</p>
      <div className="workspace-build-list" role="group" aria-label={text.libraryTitle}>
        {workspace.builds.map(entry => {
          const active = entry.stages.find(candidate => candidate.id === entry.activeStage)!;
          const character = active.saved.state.character;
          return <button key={entry.id} aria-pressed={entry.id === build.id} disabled={session.busy} onClick={() => session.selectStage(entry.id)}>
            <strong>{entry.name || text.initialBuild}</strong>
            <small>Lv{character.level} {className(character.ascendancy_name || character.class_name)} · {entry.stages.length} {text.stages}</small>
          </button>;
        })}
      </div>
      <div className="build-card-actions">
        <button disabled={session.busy} onClick={() => begin('newBuild')}>{text.newBuild}</button>
        <button disabled={session.busy} onClick={() => begin('renameBuild')}>{text.renameBuild}</button>
      </div>
      {nameForm('build')}
    </article>
    <article className="build-card workspace-stages" aria-labelledby="workspace-stages-heading">
      <h3 id="workspace-stages-heading">{text.stagesTitle} · {build.name || text.initialBuild}</h3>
      <p className="build-card-hint">{text.stagesHint}</p>
      <div className="workspace-stage-list" role="group" aria-label={text.stagesTitle}>
        {build.stages.map(entry => <button key={entry.id} aria-pressed={entry.id === stage.id} disabled={session.busy}
          onClick={() => session.selectStage(build.id, entry.id)}>{entry.name || text.initialStage}<small>Lv{entry.saved.state.character.level} · {entry.saved.state.allocatedNodes.length} {tt('build.passives')}</small></button>)}
      </div>
      <div className="build-card-actions">
        <button disabled={session.busy} onClick={() => begin('copyStage')}>{text.copyStage}</button>
        <button disabled={session.busy} onClick={() => begin('renameStage')}>{text.renameStage}</button>
      </div>
      {nameForm('stage')}
      {session.build && session.loadouts.length > 0 && <details className="workspace-loadouts">
        <summary>{text.importedLoadouts}</summary>
        <p className="build-card-hint">{text.importedLoadoutsHint}</p>
        <label>{tt('loadout.switch')}<select aria-label={tt('loadout.switch')} value={session.activeLoadout ?? ''} disabled={session.busy} onChange={event => {
          const loadout = session.loadouts[Number(event.target.value)];
          if (loadout && confirmLoadout()) void session.switchLoadout({ tree: loadout.tree, item: loadout.item, skill: loadout.skill });
        }}>
          {session.activeLoadout === null && <option value="">—</option>}
          {session.loadouts.map((loadout, index) => <option key={index} value={index}>{loadout.name}</option>)}
        </select></label>
        <div className="build-card-actions">
          <button disabled={session.busy} onClick={() => manageLoadout('duplicate')}>{tt('loadout.new')}</button>
          <button disabled={session.busy} onClick={() => manageLoadout('rename')}>{tt('loadout.rename')}</button>
          {session.loadouts.length > 1 && <button disabled={session.busy} onClick={() => manageLoadout('remove')}>{tt('loadout.remove')}</button>}
        </div>
      </details>}
    </article>
  </div>;
}
