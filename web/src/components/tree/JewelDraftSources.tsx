import { lazy, Suspense, useLayoutEffect, useState } from 'react';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import { AppButton } from '../shared/controls';

const UniqueCatalog = lazy(() => import('../items/UniqueCatalog').then(module => ({ default: module.UniqueCatalog })));
const ItemCrafter = lazy(() => import('../items/ItemCrafter').then(module => ({ default: module.ItemCrafter })));

export function JewelDraftSources({ session, lang, onDraft, onDraftReady }: {
  session: BuildSession; lang: Lang; onDraft: (text: string) => void; onDraftReady: () => void;
}) {
  const tt = bindT(lang);
  const [source, setSource] = useState<'unique' | 'craft' | 'staged' | null>(null);
  const choose = (text: string) => { onDraft(text); setSource('staged'); };
  useLayoutEffect(() => {
    if (source === 'staged') onDraftReady();
  }, [source, onDraftReady]);
  return <>
    <div className="item-actions">
      <AppButton aria-expanded={source === 'unique'} onClick={() => setSource(source === 'unique' ? null : 'unique')}>{tt('tree.chooseUniqueJewel')}</AppButton>
      <AppButton aria-expanded={source === 'craft'} onClick={() => setSource(source === 'craft' ? null : 'craft')}>{tt('tree.craftJewel')}</AppButton>
    </div>
    <Suspense fallback={<p role="status">{tt('common.loading')}</p>}>
      {source === 'unique' && <UniqueCatalog session={session} lang={lang} onJewelDraft={choose} />}
      {source === 'craft' && <ItemCrafter session={session} lang={lang} onJewelDraft={choose} />}
    </Suspense>
  </>;
}
