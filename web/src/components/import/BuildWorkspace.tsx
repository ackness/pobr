import { chineseScript } from '../../lib/chinese';
import { useAppDialogs } from '../shared/AppDialogs';
import { useSessionAction } from '../shared/useSessionAction';
import { AppButton, AppInput, AppSelectControl } from '../shared/controls';
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
    <AppButton onClick={onManage}>{text.manage}</AppButton>
    <strong className="workspace-current-name">{build.name || text.initialBuild}</strong>
    {build.stages.length > 1 ? <label>{text.stages}<AppSelectControl aria-label={text.stages} value={build.activeStage} disabled={session.busy} onChange={event => session.selectStage(build.id, event.target.value)}>
      {build.stages.map(entry => <option key={entry.id} value={entry.id}>{entry.name || text.initialStage}</option>)}
    </AppSelectControl></label> : <span className="workspace-current-stage">{stage.name || text.initialStage}</span>}
    <span className={session.storageFailed ? 'workspace-save-error' : 'workspace-saved'} role="status">{session.storageFailed ? text.failed : text.saved}</span>
  </div>;
}

export function BuildWorkspace({ session, lang }: { session: BuildSession; lang: Lang }) {
  const text = workspaceText(lang);
  const tt = bindT(lang);
  const dialogs = useAppDialogs(lang);
  const captureAction = useSessionAction(session);
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
    <label>{text[action]}<AppInput autoFocus aria-label={text.name} value={name} maxLength={100} placeholder={action === 'copyStage' ? `${text.start} / ${text.progress} / ${text.endgame}` : undefined} onChange={event => setName(event.target.value)} /></label>
    <AppButton type="submit" disabled={!name.trim() || session.busy}>{text.save}</AppButton>
    <AppButton type="button" onClick={() => setAction(null)}>{text.cancel}</AppButton>
  </form>;
  const confirmLoadout = async () => !session.isDirty || await dialogs.confirm(tt('loadout.confirmDiscard'));
  const manageLoadout = async (op: 'duplicate' | 'rename' | 'remove') => {
    const valid = captureAction();
    if (!await confirmLoadout() || !valid()) return;
    if (op === 'remove') {
      if (await dialogs.confirm(tt('loadout.confirmRemove')) && valid()) void session.manageLoadout(op);
      return;
    }
    const current = session.loadouts[session.activeLoadout ?? 0]?.name ?? '';
    const nextName = await dialogs.prompt(tt('loadout.namePrompt'), op === 'rename' ? current : '');
    if (nextName?.trim() && valid()) void session.manageLoadout(op, nextName.trim());
  };
  const className = (raw: string) => lang === 'en-US' ? raw : chineseScript(session.classNames.ascendancies[raw] ?? session.classNames.classes[raw] ?? raw, lang);
  return <div className="workspace-management">
    <article className="build-card workspace-library" aria-labelledby="workspace-library-heading">
      <h3 id="workspace-library-heading">{text.libraryTitle}</h3>
      <p className="build-card-hint">{text.libraryHint}</p>
      <div className="workspace-build-list" role="group" aria-label={text.libraryTitle}>
        {workspace.builds.map(entry => {
          const active = entry.stages.find(candidate => candidate.id === entry.activeStage)!;
          const character = active.saved.state.character;
          return <AppButton key={entry.id} aria-pressed={entry.id === build.id} disabled={session.busy} onClick={() => session.selectStage(entry.id)}>
            <strong>{entry.name || text.initialBuild}</strong>
            <small>Lv{character.level} {className(character.ascendancy_name || character.class_name)} · {entry.stages.length} {text.stages}</small>
          </AppButton>;
        })}
      </div>
      <div className="build-card-actions">
        <AppButton disabled={session.busy} onClick={() => begin('newBuild')}>{text.newBuild}</AppButton>
        <AppButton disabled={session.busy} onClick={() => begin('renameBuild')}>{text.renameBuild}</AppButton>
      </div>
      {nameForm('build')}
    </article>
    <article className="build-card workspace-stages" aria-labelledby="workspace-stages-heading">
      <h3 id="workspace-stages-heading">{text.stagesTitle} · {build.name || text.initialBuild}</h3>
      <p className="build-card-hint">{text.stagesHint}</p>
      <div className="workspace-stage-list" role="group" aria-label={text.stagesTitle}>
        {build.stages.map(entry => <AppButton key={entry.id} aria-pressed={entry.id === stage.id} disabled={session.busy}
          onClick={() => session.selectStage(build.id, entry.id)}>{entry.name || text.initialStage}<small>Lv{entry.saved.state.character.level} · {entry.saved.state.allocatedNodes.length} {tt('build.passives')}</small></AppButton>)}
      </div>
      <div className="build-card-actions">
        <AppButton disabled={session.busy} onClick={() => begin('copyStage')}>{text.copyStage}</AppButton>
        <AppButton disabled={session.busy} onClick={() => begin('renameStage')}>{text.renameStage}</AppButton>
      </div>
      {nameForm('stage')}
      {session.build && session.loadouts.length > 0 && <details className="workspace-loadouts">
        <summary>{text.importedLoadouts}</summary>
        <p className="build-card-hint">{text.importedLoadoutsHint}</p>
        <label>{tt('loadout.switch')}<AppSelectControl aria-label={tt('loadout.switch')} value={session.activeLoadout ?? ''} disabled={session.busy} onChange={async event => {
          const valid = captureAction();
          const loadout = session.loadouts[Number(event.target.value)];
          if (loadout && await confirmLoadout() && valid()) void session.switchLoadout({ tree: loadout.tree, item: loadout.item, skill: loadout.skill, config: loadout.config });
        }}>
          {session.activeLoadout === null && <option value="">—</option>}
          {session.loadouts.map((loadout, index) => <option key={index} value={index}>{loadout.name}</option>)}
        </AppSelectControl></label>
        <div className="build-card-actions">
          <AppButton disabled={session.busy} onClick={() => manageLoadout('duplicate')}>{tt('loadout.new')}</AppButton>
          <AppButton disabled={session.busy} onClick={() => manageLoadout('rename')}>{tt('loadout.rename')}</AppButton>
          {session.loadouts.length > 1 && <AppButton disabled={session.busy} onClick={() => manageLoadout('remove')}>{tt('loadout.remove')}</AppButton>}
        </div>
      </details>}
    </article>
  </div>;
}
