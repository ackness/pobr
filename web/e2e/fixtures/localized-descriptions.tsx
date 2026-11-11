import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import { getBackend } from '../../src/api/backend';
import type { ItemLineJson } from '../../src/api/types';
import type { Lang } from '../../src/lib/i18n';
import { ItemDisplay } from '../../src/components/items/ItemDisplay';
import { GemDescriptionPanel } from '../../src/components/skills/GemDescriptionPanel';
import { UiProvider } from '../../src/components/shared/UiContext';
import '../../src/app/styles.css';
import '../../src/components/skills/skills.css';

// Real presentation components and snapshot prose; deterministic item classification, no engine calculation.
const ids = ['SupportRakiatasFlowPlayer', 'SupportGarukhansResolvePlayer', 'SupportElementalArmamentPlayerTwo', 'SupportElementalFocusPlayer', 'SupportInevitableCriticalsPlayer'];
const lines: ItemLineJson[] = [
  { kind: 'name', text: '苍空 风唤' }, { kind: 'base', text: '弱兵长靴' },
  ...['Item Level: 82', 'LevelReq: 70', 'Dex: 93', 'Quality: +20%', 'Sockets: S', 'Evasion Rating: 431', 'Armour: 123', 'Energy Shield: 45', 'Future Defence: 9'].map(text => ({ kind: 'struct' as const, text })),
  { kind: 'explicit', text: '36% increased Evasion Rating', affix: 'prefix' },
];
const raw = 'Rarity: RARE\n' + lines.map(line => line.text).join('\n');
const backend = await getBackend();
await backend.init();
const [catalog, descriptions] = await Promise.all([backend.gemCatalog(), backend.gemDescriptions()]);
const gems = catalog.filter(gem => ids.includes(gem.skill_id));
backend.classifyItemLines = async () => lines;
backend.translateLines = async (source, lang) => source.map(line => lang === 'en-US' ? line : line === '36% increased Evasion Rating' ? (lang === 'zh-CN' ? '闪避值提高 36%' : '閃避值提高 36%') : line);

function Fixture() {
  const [lang, setLang] = useState<Lang>('zh-CN');
  return <UiProvider lang={lang} displayMode="grouped"><main style={{ padding: '1.5rem', maxWidth: '70rem', margin: 'auto' }}>
    <nav aria-label="Language">{(['zh-CN', 'en-US', 'zh-TW'] as const).map(value => <button key={value} onClick={() => setLang(value)}>{value}</button>)}</nav>
    {descriptions && gems.map(gem => <GemDescriptionPanel key={gem.skill_id} gem={gem} index={descriptions} lang={lang} />)}
    <ItemDisplay text={raw} lang={lang} />
  </main></UiProvider>;
}
createRoot(document.getElementById('root')!).render(<Fixture />);
