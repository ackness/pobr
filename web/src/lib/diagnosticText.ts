import type { Lang } from './i18n';

/** Same as `app/text.words`; kept local so `lib` does not depend on `app`. */
const words = (lang: Lang, en: string, cn: string, tw: string = cn) => lang === 'en-US' ? en : lang === 'zh-TW' ? tw : cn;

/** `[Curse|Curses]` -> `Curses`, `[Curse]` -> `Curse`: the engine keeps the game's reference markup in diagnostics. */
export function stripReferenceMarkup(text: string): string {
  return text.replace(/\[([^\[\]|]*)(?:\|([^\[\]]*))?\]/g, (_, key: string, label?: string) => label ?? key);
}

export interface UnsupportedLine { source: string | null; text: string }

/**
 * Split an `unsupported_modifiers` entry into a readable source and the line.
 * Prefixes the engine emits: `Tree:<node>: ` (passive node) and `Config: `.
 * Anything else is shown as is, only without the reference markup. Callers keep `raw` for faithful diagnostics.
 */
export function unsupportedLine(raw: string, lang: Lang): UnsupportedLine {
  const w = (en: string, cn: string, tw = cn) => words(lang, en, cn, tw);
  const tree = /^Tree:(\d+): ([\s\S]*)$/.exec(raw);
  if (tree) return { source: w(`Passive #${tree[1]}`, `天赋 #${tree[1]}`, `天賦 #${tree[1]}`), text: stripReferenceMarkup(tree[2]) };
  const config = /^Config: ([\s\S]*)$/.exec(raw);
  if (config) return { source: w('Setup', '配置', '配置'), text: stripReferenceMarkup(config[1]) };
  return { source: null, text: stripReferenceMarkup(raw) };
}

export function unsupportedLineLabel(raw: string, lang: Lang): string {
  const { source, text } = unsupportedLine(raw, lang);
  return source ? `${source}: ${text}` : text;
}

const REASONS: Record<string, readonly [string, string, string]> = {
  unextractable: ['Could not be read from the skill data', '无法从技能数据读取', '無法從技能資料讀取'],
  scalar: ['Scalar multiplier is not modelled', '标量倍率未建模', '純量倍率未建模'],
  unknown_mod_name: ['Unknown modifier name', '未知的词缀名称', '未知的詞綴名稱'],
  missing_mod_type: ['Modifier type is missing', '缺少词缀类型', '缺少詞綴類型'],
  mod_type: ['Modifier type is not supported', '不支持的词缀类型', '不支援的詞綴類型'],
  tag: ['Condition tag is not supported', '不支持的条件标签', '不支援的條件標籤'],
  flags: ['Flags are not supported', '不支持的标记', '不支援的標記'],
  keyword_flags: ['Keywords are not supported', '不支持的关键字', '不支援的關鍵字'],
  skill_data_key: ['Skill data key is not supported', '不支持的技能数据项', '不支援的技能資料項'],
  skill_flag: ['Skill flag is not supported', '不支持的技能标记', '不支援的技能標記'],
  kind: ['Kind is not supported', '不支持的类别', '不支援的類別'],
  'quality-applicability': ['Quality bonus does not apply', '品质加成不适用', '品質加成不適用'],
};

/** Short label for a stat-rejection `reason` code, or null when the code is not one the engine is known to emit. */
export function ledgerReasonLabel(reason: string, lang: Lang): string | null {
  const entry = /^([a-z_-]+)(?::[\s\S]*)?(?: \(part of the entry\))?$/.exec(reason);
  const labels = entry && Object.hasOwn(REASONS, entry[1]) ? REASONS[entry[1]] : null;
  if (!labels) return null;
  return words(lang, ...labels) + (reason.endsWith('(part of the entry)') ? words(lang, ' (whole entry skipped)', '（整条被跳过）', '（整條被跳過）') : '');
}

const SKILL_OWNER = /^(?:gem|buff|curse|debuff|minion|support|skill|trigger)\.([A-Za-z0-9_]+)/;

/** The skill id inside a stat owner such as `curse.TemporalChainsPlayer` or `gem.X.q20`; null for other owners. */
export function ownerSkillId(owner: string): string | null {
  return SKILL_OWNER.exec(owner)?.[1] ?? null;
}

/** Owner as `Skill name`, or the raw owner when it names no skill. */
export function ownerLabel(owner: string, skillName: (id: string) => string): string {
  const id = ownerSkillId(owner);
  return id ? skillName(id) : owner;
}
