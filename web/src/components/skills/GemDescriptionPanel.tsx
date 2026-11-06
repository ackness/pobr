import { supportText } from '../../lib/supportI18n';
import type { GemCatalogEntry } from '../../api/types';
import { gemDescription, type GemDescriptionIndex } from '../../lib/gemDescriptions';
import { gemTagLabels } from '../../lib/gemTags';
import { bindT, type Lang } from '../../lib/i18n';
import { gemDisplayName, markUntranslated } from '../../lib/skillNames';
import { AppButton } from '../shared/controls';

/** Official qualitative gem text, shared by manual picking and recommendation plans. */
export function GemDescriptionPanel({ gem, index, lang, onAdd, descriptionId }: {
  gem: GemCatalogEntry; index: GemDescriptionIndex; lang: Lang; onAdd?: () => void; descriptionId?: string;
}) {
  const tt = bindT(lang);
  const name = gemDisplayName(gem, lang);
  const englishName = gemDisplayName(gem, 'en-US');
  const showEnglishName = name !== englishName && name !== markUntranslated(englishName, englishName, lang);
  const description = gemDescription(index, gem, lang);
  const attributes = index.requirements[gem.skill_id];
  const tags = [tt(gem.is_support ? 'picker.support' : 'picker.active'), ...gemTagLabels(gem.tags, lang, 3)];
  if (attributes?.length) tags.push(tt('picker.requires').replace('{attributes}',
    attributes.map(attr => tt(`tree.attr.${attr}`)).join(' / ')));
  return <section className="gem-description-panel" aria-label={tt('picker.description')}>
    <strong className="gem-description-name">{name}{showEnglishName && <> / <span lang="en">{englishName}</span></>}</strong>
    {description ? <div className="gem-description-copy">
      {description.english && lang !== 'en-US' && <small className="gem-description-original">{tt('picker.originalText')}</small>}
      <p id={descriptionId} className="gem-description-text" lang={description.lang}>{description.text}</p>
    </div> : <p className="skills-hint">{supportText(lang, 'descriptionUnavailable')}</p>}
    <p className="gem-description-tags">{tags.join(' · ')}</p>
    {onAdd && <AppButton type="button" aria-label={`${tt('common.add')} ${name}`}
      onMouseDown={event => event.preventDefault()} onClick={onAdd}>{tt('common.add')}</AppButton>}
  </section>;
}
