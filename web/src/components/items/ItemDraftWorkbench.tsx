import { useEffect, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { ItemLineJson } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { Lang } from '../../lib/i18n';
import { craftT } from '../../lib/itemCraftingText';
import type { TradeBase, TradeCatalog } from '../../lib/tradeOptimizer';
import { ItemReplacementCheck } from '../trade/ItemReplacementCheck';
import { ReplacementAffixEditor } from '../trade/ReplacementAffixEditor';
import '../trade/trade.css';

const objective = { stat: 'TotalDPS', secondaryStat: 'TotalEHP', constraints: [] };

/** Canonical drafts remain local until the existing preview explicitly applies a variant. */
export function ItemDraftWorkbench({ initialText, base, catalog, session, lang, editable = true }: {
  initialText: string; base: TradeBase; catalog: TradeCatalog; session: BuildSession; lang: Lang; editable?: boolean;
}) {
  const [draft, setDraft] = useState<{ text: string; lines: ItemLineJson[] } | null>(null);
  const [input, setInput] = useState({ text: initialText, revision: 0 });
  const [dirty, setDirty] = useState(false);
  const [error, setError] = useState(false);
  const [jewelSockets, setJewelSockets] = useState<number[] | null>(null);
  useEffect(() => {
    let cancelled = false;
    setDraft(null); setError(false);
    getBackend().then(async backend => {
      const text = await backend.normalizeItemDraft(input.text);
      const lines = await backend.classifyItemLines(text);
      if (!cancelled) { setDraft({ text, lines }); setDirty(false); }
    }).catch(() => { if (!cancelled) setError(true); });
    return () => { cancelled = true; };
  }, [input]);
  useEffect(() => {
    let cancelled = false;
    setJewelSockets(null);
    getBackend().then(backend => backend.loadPassiveTree(session.treeVersion)).then(nodes => {
      if (!cancelled) setJewelSockets(nodes.filter(node => node.kind === 'jewel_socket').map(node => node.skill));
    }).catch(() => { if (!cancelled) setJewelSockets([]); });
    return () => { cancelled = true; };
  }, [session.treeVersion]);
  if (error) return <p role="alert">{craftT(lang, 'error')}</p>;
  if (!draft || !jewelSockets) return <p role="status">{craftT(lang, 'loading')}</p>;
  return <div className="item-draft-workbench">
    {editable && !/^Rarity: NORMAL/m.test(draft.text) && <ReplacementAffixEditor key={draft.text}
      report={{ ...draft, base }} catalog={catalog} characterLevel={100} lang={lang}
      disabled={session.busy} onCompare={text => setInput(previous => ({ text, revision: previous.revision + 1 }))} onDirtyChange={setDirty} />}
    {editable && /^Rarity: NORMAL/m.test(draft.text) && <p>{craftT(lang, 'normalHint')}</p>}
    {!dirty && <ItemReplacementCheck key={draft.text} session={session} lang={lang} active={true}
      objective={objective} catalog={catalog} jewelSockets={jewelSockets} goalLabel="" onEditGoal={() => {}}
      initialItemText={draft.text} candidate={{ kind: base.category === 'jewel' ? 'jewel' : 'item' }} />}
  </div>;
}
