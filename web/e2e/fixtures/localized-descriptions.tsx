import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import { getBackend } from '../../src/api/backend';
import type { ItemLineJson } from '../../src/api/types';
import type { Lang } from '../../src/lib/i18n';
import type { TradeCatalog } from '../../src/lib/tradeOptimizer';
import { ItemDisplay } from '../../src/components/items/ItemDisplay';
import { SupportDescriptions } from '../../src/components/skills/SupportDescriptions';
import { UiProvider } from '../../src/components/shared/UiContext';
import current from '../../../data/CURRENT?raw';
import '../../src/app/styles.css';
import '../../src/components/skills/skills.css';

// Real presentation components and snapshot prose; deterministic item classification, no engine calculation.
const catalogs = import.meta.glob('../../../data/*/overlay/trade_catalog.json');
const catalog = (await catalogs[`../../../data/${current.trim()}/overlay/trade_catalog.json`]() as { default: TradeCatalog }).default;
const ids = ['SupportRakiatasFlowPlayer', 'SupportGarukhansResolvePlayer', 'SupportElementalArmamentPlayerTwo', 'SupportElementalFocusPlayer', 'SupportInevitableCriticalsPlayer'];
const names = ['拉琪塔之流', '格鲁坎的决心', '元素军械 II', '元素集中', '命定绝杀 I'];
const gems = ids.map((id, i) => ({ ...catalog.gems!.find(gem => gem.skill_id === id)!, name: names[i] }));
gems.push({ ...gems[0], skill_id: 'future-support', name: 'Fallback support', description: 'Untranslated source.' });
const lines: ItemLineJson[] = [
  { kind: 'name', text: '苍空 风唤' }, { kind: 'base', text: '弱兵长靴' },
  ...['Item Level: 82', 'LevelReq: 70', 'Dex: 93', 'Quality: +20%', 'Sockets: S', 'Evasion Rating: 431', 'Armour: 123', 'Energy Shield: 45', 'Future Defence: 9'].map(text => ({ kind: 'struct' as const, text })),
  { kind: 'explicit', text: '36% increased Evasion Rating', affix: 'prefix' },
];
const raw = 'Rarity: RARE\n' + lines.map(line => line.text).join('\n');
const backend = await getBackend();
backend.classifyItemLines = async () => lines;
backend.translateLines = async (source, lang) => source.map(line => lang === 'en-US' ? line : line === '36% increased Evasion Rating' ? (lang === 'zh-CN' ? '闪避值提高 36%' : '閃避值提高 36%') : line);

function Fixture() {
  const [lang, setLang] = useState<Lang>('zh-CN');
  return <UiProvider lang={lang} displayMode="grouped"><main style={{ padding: '1.5rem', maxWidth: '70rem', margin: 'auto' }}>
    <nav aria-label="Language">{(['zh-CN', 'en-US', 'zh-TW'] as const).map(value => <button key={value} onClick={() => setLang(value)}>{value}</button>)}</nav>
    <SupportDescriptions gems={gems} lang={lang} />
    <ItemDisplay text={raw} lang={lang} />
  </main></UiProvider>;
}
createRoot(document.getElementById('root')!).render(<Fixture />);
