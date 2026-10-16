import { useRef, useState } from 'react';
import { getBackend } from '../../api/backend';
import { formatApiError } from '../../api/error';
import type { BuildJson, TreeConversionResponse } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { Lang } from '../../lib/i18n';

export function TreeConversionControls({ session, lang }: { session: BuildSession; lang: Lang }) {
  const en = lang === 'en-US';
  const traditional = lang === 'zh-TW';
  const label = (english: string, chinese: string, taiwan = chinese) => en ? english : traditional ? taiwan : chinese;
  const [all, setAll] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<{ result: TreeConversionResponse; decoded: BuildJson; version: number } | null>(null);
  const generation = useRef(0);
  const latest = useRef(session);
  latest.current = session;
  const staleText = label('The build changed. Create a new preview.', '构筑已变更，请重新预览。', '構築已變更，請重新預覽。');
  const stale = preview && preview.version !== session.stateVersion;
  const cancel = () => { generation.current++; setPending(false); setPreview(null); setError(null); };
  const createPreview = async () => {
    const epoch = ++generation.current;
    const version = session.stateVersion;
    setPending(true); setError(null); setPreview(null);
    try {
      const code = await session.exportCode();
      const backend = await getBackend();
      const result = await backend.previewTreeConversion(code, all);
      const decoded = await backend.decodeBuild(result.code);
      if (epoch !== generation.current) return;
      if (latest.current.stateVersion !== version) { setError(staleText); return; }
      setPreview({ result, decoded, version });
    } catch (err) {
      if (epoch === generation.current) setError(formatApiError(err));
    } finally {
      if (epoch === generation.current) setPending(false);
    }
  };
  if (!session.character) return null;
  return <section className="tree-conversion" aria-label={label('Tree conversion', '天赋树转换', '天賦樹轉換')}>
    <div className="tree-conversion-actions">
      <label><input type="checkbox" checked={all} disabled={pending} onChange={event => { setAll(event.target.checked); setPreview(null); }} />
        {label('All passive specs', '全部天赋方案', '全部天賦方案')}</label>
      <button disabled={pending || session.busy} onClick={() => void createPreview()}>
        {pending ? label('Preparing preview…', '正在准备预览…', '正在準備預覽…') : label('Convert to current tree', '转换为当前天赋树', '轉換為目前天賦樹')}
      </button>
      <button disabled={!session.canUndoTree || session.busy || pending} onClick={() => { cancel(); session.undoTree(); }}>
        {label('Undo tree edit', '撤销天赋编辑', '復原天賦編輯')}</button>
      {(pending || preview) && <button onClick={cancel}>{label('Cancel', '取消')}</button>}
    </div>
    {error && <p role="alert">{error}</p>}
    {preview && <div className="tree-conversion-preview" role="region" aria-label={label('Conversion preview', '转换预览', '轉換預覽')}>
      <p>{label('Review the changes before confirming. Disconnected nodes stay allocated; their paths may need repair.', '请先检查变更再确认。断路节点会保留加点，可能需要修复路径。', '請先檢查變更再確認。斷路節點會保留加點，可能需要修復路徑。')}</p>
      {preview.result.specs.map(spec => <article key={spec.index}>
        <h3>{spec.title} · {spec.from_version} → {spec.to_version}</h3>
        <p>{label('Kept', '保留')} {spec.kept.length} · {label('Dropped', '移除')} {spec.dropped.length} · {label('Disconnected', '断路', '斷路')} {spec.disconnected.length}</p>
        <details><summary>{label('Kept node IDs', '保留节点编号', '保留節點編號')}</summary><p>{spec.kept.join(', ') || '—'}</p></details>
        {([['dropped', label('Dropped nodes', '移除节点', '移除節點')], ['disconnected', label('Disconnected nodes (kept)', '断路节点（保留）', '斷路節點（保留）')]] as const).map(([key, title]) => spec[key].length > 0 && <div key={key}>
          <h4>{title}</h4>
          <ul>{spec[key].map(node => <li key={node.id}>
            <strong>{node.name} · #{node.id}</strong>
            {key === 'dropped' && <span> — {node.reason === 'missing' ? label('Removed from tree', '当前树已移除', '目前樹已移除') : node.reason === 'socket_missing_or_changed' ? label('Socket removed or changed', '插槽已移除或变更', '插槽已移除或變更') : label('Node identity changed', '节点身份已变更', '節點身份已變更')}</span>}
            {node.jewel_item_id !== null && <span> · {label('Jewel', '珠宝', '珠寶')} #{node.jewel_item_id}{key === 'dropped' && label(' stays in the item pool', '保留在物品池', '保留在物品池')}</span>}
            {key === 'disconnected' && <span> · {spec.disconnected_by_set.map((ids, i) => ids.includes(node.id) ? i === 0 ? label('Shared', '共用') : `${label('Weapon set', '武器组', '武器組')} ${i}` : '').filter(Boolean).join(', ')}</span>}
            {node.stats.map((stat, i) => <div key={i}>{stat}</div>)}
          </li>)}</ul>
        </div>)}
      </article>)}
      {stale && <p role="alert">{staleText}</p>}
      <button disabled={!!stale || session.busy} onClick={() => {
        if (!session.applyTreeConversion(preview.decoded, preview.result.code, preview.version)) { setError(staleText); return; }
        setPreview(null);
      }}>{label('Confirm conversion', '确认转换', '確認轉換')}</button>
    </div>}
  </section>;
}
