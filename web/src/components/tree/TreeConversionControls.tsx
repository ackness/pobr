import { colon } from '../../app/text';
import { AppButton, AppInput } from '../shared/controls';
import { useEffect, useMemo, useRef, useState } from 'react';
import { getBackend } from '../../api/backend';
import { formatApiError } from '../../api/error';
import type { PassiveNode, BuildJson, TreeConversionNode, TreeConversionResponse } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { cleanLine, useLocalizedLines, useLocalizedNames } from '../../hooks/useLocalizedLines';
import { shownLines } from '../../lib/continuedLines';
import { bindT, type Lang } from '../../lib/i18n';
import { passiveNameRequest } from './passiveNodeNames';

/**
 * The conversion request and its preview. The state lives with the page, not with the canvas it is drawn
 * on: the canvas is rebuilt when the weapon set changes, and an open preview must survive that so that
 * it can say the build changed instead of vanishing. Leaving the page closes it.
 */
export function useTreeConversion(session: BuildSession, lang: Lang, active: boolean) {
  const tt = bindT(lang);
  const [all, setAll] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<{ result: TreeConversionResponse; decoded: BuildJson; version: number; nodes: PassiveNode[] } | null>(null);
  const generation = useRef(0);
  const latest = useRef(session);
  latest.current = session;
  const staleText = tt('tree.conversion.stale');
  const stale = preview && preview.version !== session.stateVersion;
  const cancel = () => { generation.current++; setPending(false); setPreview(null); setError(null); };
  useEffect(() => { if (!active) { cancel(); setAll(false); } }, [active]);
  const createPreview = async () => {
    const epoch = ++generation.current;
    const version = session.stateVersion;
    setPending(true); setError(null); setPreview(null);
    try {
      const code = await session.exportCode();
      const backend = await getBackend();
      const result = await backend.previewTreeConversion(code, all);
      const decoded = await backend.decodeBuild(result.code);
      const nodes = await backend.loadPassiveTree();
      if (epoch !== generation.current) return;
      if (latest.current.stateVersion !== version) { setError(staleText); return; }
      setPreview({ result, decoded, version, nodes });
    } catch (err) {
      if (epoch === generation.current) setError(formatApiError(err));
    } finally {
      if (epoch === generation.current) setPending(false);
    }
  };
  return { lang, text: tt, all, setAll, pending, error, setError, preview, setPreview, stale, staleText, cancel, createPreview };
}
export type TreeConversion = ReturnType<typeof useTreeConversion>;

export function TreeConversionControls({ session, conversion }: { session: BuildSession; conversion: TreeConversion }) {
  const { lang, text, all, setAll, pending, error, setError, preview, setPreview, stale, staleText, cancel, createPreview } = conversion;
  if (!session.character) return null;
  return <section className="tree-conversion" aria-label={text('tree.conversion.title')}>
    <div className="tree-conversion-actions">
      <details className="tree-conversion-options"><summary className="disclosure">{text('tree.conversionOptions')}</summary><label><AppInput type="checkbox" checked={all} disabled={pending} onChange={event => { setAll(event.target.checked); setPreview(null); }} />
        {text('tree.conversion.allSpecs')}</label></details>
      <AppButton disabled={pending || session.busy} onClick={() => void createPreview()}>
        {pending ? text('tree.conversion.preparing') : text(all ? 'tree.convertAll' : 'tree.convertCurrent')}
      </AppButton>
      <AppButton disabled={!session.canUndoTree || session.busy || pending} onClick={() => { cancel(); session.undoTree(); }}>
        {text('tree.conversion.undo')}</AppButton>
      {(pending || preview) && <AppButton onClick={cancel}>{text('common.cancel')}</AppButton>}
    </div>
    {error && <p role="alert">{error}</p>}
    {preview && <div className="tree-conversion-preview" role="region" aria-label={text('tree.conversion.preview')}>
      <div className="tree-conversion-body"><p>{text('tree.conversion.previewHint')}</p>
      {preview.result.specs.map(spec => <article key={spec.index}>
        <h3>{/^Spec(?:\s|$)/i.test(spec.title) || !spec.title ? text('tree.planNumber').replace('{number}', String(spec.index + 1)) : spec.title}</h3>
        <details><summary className="disclosure">{text('tree.details')}</summary><p>{spec.title} · {spec.from_version} → {spec.to_version}</p></details>
        <p>{text('tree.conversion.counts', { kept: spec.kept.length, dropped: spec.dropped.length, disconnected: spec.disconnected.length })}</p>
        <details><summary className="disclosure">{text('tree.conversion.keptIds')}</summary><p>{spec.kept.join(', ') || '—'}</p></details>
        {([['dropped', text('tree.conversion.droppedNodes')], ['disconnected', text('tree.conversion.disconnectedNodes')]] as const).map(([key, title]) => spec[key].length > 0 && <div key={key}>
          <h4>{title}</h4>
          <ul>{spec[key].map(node => <li key={node.id}>
            <ConversionNodeCard node={node} identity={preview.nodes.find(current => current.skill === node.id && current.name === node.name)?.id} lang={lang} />
            <details><summary className="disclosure">{text('tree.details')}</summary><p>#{node.id}{node.jewel_item_id !== null ? ` · ${text('trade.category.jewel')} #${node.jewel_item_id}` : ''}</p></details>
            {key === 'dropped' && <span>{colon(lang)}{node.reason === 'missing' ? text('tree.conversion.removed') : node.reason === 'socket_missing_or_changed' ? text('tree.conversion.socketChanged') : text('tree.conversion.identityChanged')}</span>}
            {node.jewel_item_id !== null && <span>{key === 'dropped' ? text('tree.conversion.retainedJewel', { jewel: text('trade.category.jewel') }) : ` · ${text('trade.category.jewel')}`}</span>}
            {key === 'disconnected' && <span> · {spec.disconnected_by_set.map((ids, i) => ids.includes(node.id) ? i === 0 ? text('tree.shared') : `${text('compare.weaponSet')} ${i}` : '').filter(Boolean).join(', ')}</span>}
          </li>)}</ul>
        </div>)}
      </article>)}
      </div><div className="tree-conversion-footer">
      {stale && <p role="alert">{staleText}</p>}
      <AppButton disabled={!!stale || session.busy} onClick={() => {
        if (!session.applyTreeConversion(preview.decoded, preview.result.code, preview.version)) { setError(staleText); return; }
        setPreview(null);
      }}>{text('tree.conversion.confirm')}</AppButton>
      <AppButton onClick={cancel}>{text('common.cancel')}</AppButton></div>
    </div>}
  </section>;
}

function ConversionNodeCard({ node, identity, lang }: { node: TreeConversionNode; identity?: string; lang: Lang }) {
  // Removed nodes may have no identity in the current tree. Resolve their original name by text.
  const nameRequests = useMemo(() => identity ? passiveNameRequest({ id: identity, name: node.name }) : [], [identity, node.name]);
  const names = useLocalizedNames(nameRequests, lang);
  const fallbackLines = useMemo(() => identity ? [] : [cleanLine(node.name)], [identity, node.name]);
  const fallbackName = useLocalizedLines(fallbackLines, lang);
  const lines = useMemo(() => node.stats.map(cleanLine), [node.stats]);
  const translated = useLocalizedLines(lines, lang);
  return <><strong>{names[0] ?? fallbackName[0] ?? node.name}</strong>{shownLines(lines, translated).map((line, index) => <div key={index}>{line}</div>)}</>;
}
