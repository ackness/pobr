import { useState } from 'react';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { Lang } from '../../lib/i18n';
import { workspaceText } from '../../lib/workspaceText';
import './workspace.css';

export function WorkspaceSwitcher({ session, lang, onManage }: { session: BuildSession; lang: Lang; onManage: () => void }) {
  const text = workspaceText(lang);
  const workspace = session.workspace;
  if (!workspace) return null;
  const build = workspace.builds.find(entry => entry.id === workspace.activeBuild)!;
  return <div className="workspace-switcher" aria-label={text.title}>
    <label>{text.builds}<select aria-label={text.builds} value={build.id} disabled={session.busy} onChange={event => session.selectStage(event.target.value)}>
      {workspace.builds.map((entry, index) => <option key={entry.id} value={entry.id}>{entry.name || `${text.initialBuild} ${index + 1}`}</option>)}
    </select></label>
    <span aria-hidden="true">/</span>
    <label>{text.stages}<select aria-label={text.stages} value={build.activeStage} disabled={session.busy} onChange={event => session.selectStage(build.id, event.target.value)}>
      {build.stages.map(stage => <option key={stage.id} value={stage.id}>{stage.name || text.initialStage}</option>)}
    </select></label>
    <button onClick={onManage}>{text.manage}</button>
    <span className={session.storageFailed ? 'workspace-save-error' : 'workspace-saved'} role="status">{session.storageFailed ? text.failed : text.saved}</span>
  </div>;
}

export function BuildWorkspace({ session, lang }: { session: BuildSession; lang: Lang }) {
  const text = workspaceText(lang);
  const [action, setAction] = useState<'newBuild' | 'copyStage' | 'renameBuild' | 'renameStage' | null>(null);
  const [name, setName] = useState('');
  const workspace = session.workspace;
  if (!workspace) return null;
  const build = workspace.builds.find(entry => entry.id === workspace.activeBuild)!;
  const stage = build.stages.find(entry => entry.id === build.activeStage)!;
  const download = (content: string, filename: string) => {
    const url = URL.createObjectURL(new Blob([content], { type: 'application/json' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = filename;
    link.click();
    URL.revokeObjectURL(url);
  };
  return <article className="build-card workspace-card">
    <h3>{text.title}</h3>
    <p className="build-card-hint">{text.hint}</p>
    <div className="workspace-stage-list" role="group" aria-label={text.title}>
      {build.stages.map(entry => <button key={entry.id} aria-pressed={entry.id === stage.id} disabled={session.busy}
        onClick={() => session.selectStage(build.id, entry.id)}>{entry.name || text.initialStage}<small>Lv{entry.saved.state.character.level} · {entry.saved.state.allocatedNodes.length} {lang === 'en-US' ? 'passives' : lang === 'zh-TW' ? '天賦' : '天赋'}</small></button>)}
    </div>
    <div className="build-card-actions">
      {(['newBuild', 'copyStage', 'renameBuild', 'renameStage'] as const).map(kind => <button key={kind} disabled={session.busy} onClick={() => {
        setAction(kind); setName(kind === 'renameBuild' ? build.name : kind === 'renameStage' ? stage.name : '');
      }}>{text[kind]}</button>)}
    </div>
    {action && <form className="workspace-form" onSubmit={event => {
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
    </form>}
    <p className="build-card-hint workspace-share-hint">{text.shareHint}</p>
    <div className="build-card-actions">
      <button disabled={session.busy} onClick={() => download(session.exportLocalBuild(), 'pobr-build-stages.json')}>{text.share}</button>
      <button disabled={session.busy} onClick={() => download(session.exportWorkspace(), 'pobr-workspace.json')}>{text.backup}</button>
    </div>
  </article>;
}
