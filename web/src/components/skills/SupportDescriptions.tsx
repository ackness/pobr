import type { Lang } from '../../lib/i18n';
import { supportDescription } from '../../lib/supportDescriptions';
import { supportText } from '../../lib/supportI18n';
import type { TradeGem } from '../../lib/tradeOptimizer';

export function SupportDescriptions({ gems, lang }: {
  gems: Pick<TradeGem, 'skill_id' | 'name' | 'description'>[]; lang: Lang;
}) {
  if (!gems.length) return null;
  return <details className="support-diagnostics support-descriptions">
    <summary>{supportText(lang, 'descriptions')}</summary>
    <p className="skills-hint">{supportText(lang, 'descriptionsHint')}</p>
    <ul>{gems.map(gem => {
      const description = supportDescription(gem, lang);
      return <li key={gem.skill_id}>
        <strong>{gem.name}</strong>
        {lang !== 'en-US' && description.source === 'english' && <span className="skills-hint"> · {supportText(lang, 'descriptionEnglish')}</span>}
        <p lang={description.lang}>{description.text || supportText(lang, 'descriptionUnavailable')}</p>
      </li>;
    })}</ul>
  </details>;
}
