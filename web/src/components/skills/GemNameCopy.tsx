import { CopyButton } from '../shared/CopyButton';
import { bindT, type Lang } from '../../lib/i18n';

/** 宝石名旁的小型复制按钮；复制当前界面语言下显示的名称，未解析宝石（skill_id 为空）不显示。 */
export function GemNameCopy({ skillId, name, lang }: { skillId: string; name: string; lang: Lang }) {
  if (!skillId || !name) return null;
  return <CopyButton compact text={name} lang={lang} label={bindT(lang)('skills.copyName')} />;
}
