import { CopyButton } from '../shared/CopyButton';
import { bindT, type Lang } from '../../lib/i18n';

/** Small copy button beside a gem name; copies the name shown in the active UI language and hides when `skill_id` is unresolved. */
export function GemNameCopy({ skillId, name, lang }: { skillId: string; name: string; lang: Lang }) {
  if (!skillId || !name) return null;
  return <CopyButton compact text={name} lang={lang} label={bindT(lang)('skills.copyName')} />;
}
