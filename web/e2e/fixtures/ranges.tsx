import { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { getBackend } from '../../src/api/backend';
import { UniqueCatalog } from '../../src/components/items/UniqueCatalog';
import { ReplacementAffixEditor } from '../../src/components/trade/ReplacementAffixEditor';
import { AppButton } from '../../src/components/shared/controls';
import { UiProvider } from '../../src/components/shared/UiContext';
import type { BuildSession } from '../../src/hooks/useBuildSession';
import { loadTradeCatalog } from '../../src/lib/tradeOptimizer';
import '../../src/styles/global.css';

await import('../../src/app/styles.css');
const backend = await getBackend();
await backend.init();
const catalog = await loadTradeCatalog();
const text = 'Rarity: RARE\nRange Fixture\nSapphire Ring\nItem Level: 80\nImplicits: 1\n+20% to Cold Resistance\n+80 to maximum Life\n+33% to Fire Resistance';
const report = { text, base: catalog.bases.find(base => base.name === 'Sapphire Ring')!, lines: await backend.classifyItemLines(text) };

// Keep the real WASM draft request pending after recording its input and result,
// so the catalogue's own preparing state can be exercised without the workbench.
const draft = backend.uniqueItemDraft;
backend.uniqueItemDraft = async selection => {
  document.getElementById('unique-selection')!.textContent = JSON.stringify(selection);
  document.getElementById('unique-preview')!.textContent = (await draft(selection)).text;
  return new Promise(() => {});
};

function Fixture() {
  const [dark, setDark] = useState(true);
  const [busy, setBusy] = useState(false);
  const [compared, setCompared] = useState('');
  const [dirty, setDirty] = useState(false);
  useEffect(() => { document.documentElement.classList.toggle('dark', dark); }, [dark]);
  return <UiProvider lang="en-US"><main className="shell-root" style={{ display: 'block', height: 'auto', overflow: 'visible', padding: 24, maxWidth: 900 }}>
    <AppButton onClick={() => setDark(value => !value)}>Toggle theme</AppButton>
    <AppButton onClick={() => setBusy(value => !value)}>Toggle busy</AppButton>
    <UniqueCatalog session={{ busy } as BuildSession} lang="en-US" />
    <ReplacementAffixEditor report={report} catalog={catalog} characterLevel={100} lang="en-US" disabled={busy}
      onCompare={setCompared} onDirtyChange={setDirty} />
    <output aria-label="Compared item">{compared}</output>
    <output aria-label="Dirty">{String(dirty)}</output>
    <output id="unique-selection" aria-label="Unique selection" />
    <output id="unique-preview" aria-label="Unique preview" />
  </main></UiProvider>;
}
createRoot(document.getElementById('root')!).render(<Fixture />);
