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

/** Exact reasons and whole engine templates. Unknown reasons remain available in Details. */
export function plainModifierReason(reason: string, lang: Lang, conditions: Readonly<Record<string, string>> = {}): string | null {
  const w = (en: string, cn: string, tw = cn) => words(lang, en, cn, tw);
  const exact: Record<string, readonly [string, string, string]> = {
    'the build creates no minion to give it to': ['This build has no minions', '此 BD 没有召唤物', '此 BD 沒有召喚物'],
    'its value is 0': ['Its value is zero', '数值为零', '數值為零'],
    'it is stored as false': ['This effect is disabled', '此效果未开启', '此效果未啟用'],
    'no rule matched this line': ['This modifier could not be interpreted', '未能识别此词缀', '未能辨識此詞綴'],
    'no query asks for this name': ['This modifier is not used by the calculated skill', '计算技能未使用此词缀', '計算技能未使用此詞綴'],
    'no totem skill is selected': ['No totem skill is selected', '尚未选择图腾技能', '尚未選擇圖騰技能'],
    'flask is not active': ['This flask is not active', '此药剂尚未生效', '此藥劑尚未生效'],
    'a config option nothing in this build asks for': ['This build does not use this setting', '此 BD 未使用此配置', '此 BD 未使用此設定'],
    'config option nobody asks for': ['This build does not use this setting', '此 BD 未使用此配置', '此 BD 未使用此設定'],
    'event frequency is not modelled': ['This calculation does not simulate how often this event happens', '本次计算未模拟此事件的发生频率', '本次計算未模擬此事件的發生頻率'],
    'seeded notable replacements are not modelled': ['Replaced jewel passives are not supported yet', '尚不支持珠宝替换的天赋', '尚不支援珠寶替換的天賦'],
    'given to the minions, whose calculation reads it': ['Included in minion calculations', '已计入召唤物计算', '已計入召喚物計算'],
  };
  if (exact[reason]) return words(lang, ...exact[reason]);
  const condition = /^(?:Condition (\w+) not met|needs (\w+))$/.exec(reason);
  if (condition) {
    const id = condition[1] ?? condition[2];
    const known: Record<string, string> = {
      LowLife: w('Low Life', '低血', '貧血'), Combat: w('Combat', '战斗状态', '戰鬥狀態'),
      Buffs: w('Buffs', '增益效果', '增益效果'), Effective: w('Enemy conditions enabled', '已启用敌人条件', '已啟用敵人條件'),
      EnemyPoisoned: w('Enemy poisoned', '敌人中毒', '敵人中毒'), EnemyChilled: w('Enemy chilled', '敌人冰缓', '敵人冰緩'),
      AncestralBoost: w('Ancestral Boost', '先祖增幅', '先祖增幅'), Attack: w('Attack skill', '攻击技能', '攻擊技能'),
    };
    const label = known[id] ?? conditions[id];
    return label ? w(`Condition not met: ${label}`, `未满足条件：${label}`, `未滿足條件：${label}`) : null;
  }
  if (/^SkillType \w+(?: · SkillType \w+)*$/.test(reason)) return w('The calculated skill does not meet the skill type requirement', '计算技能不满足技能类型要求', '計算技能不滿足技能類型要求');
  if (/^another OVERRIDE of \S+ wins$/.test(reason)) return w('Another modifier sets this value instead', '此数值已由另一个词缀设定', '此數值已由另一個詞綴設定');
  if (/^only the strongest \S+ counts, and another is stronger$/.test(reason)) return w('Only the strongest effect counts; another effect is stronger', '仅计入最强效果，已有更强的效果', '僅計入最強效果，已有更強的效果');
  if (/^no query counted it: the quer(?:y|ies) of \S+ ask in another context than the main skill's$/.test(reason)) return w('This modifier does not apply to the calculated skill', '此词缀不适用于计算技能', '此詞綴不適用於計算技能');
  if (/^\S+ is read as [A-Z, ]+ only, not as [A-Z]+$/.test(reason)) return w('This stacking type is not used for this value', '此数值未使用这种叠加类型', '此數值未使用這種疊加類型');
  if (/^given to the minions; no minion's calculation reads .+$/.test(reason)) return w('The selected minions do not use this effect', '所选召唤物未使用此效果', '所選召喚物未使用此效果');
  if (/^rule \S+ recognises it and stores nothing$/.test(reason) || reason === 'a rule recognises it and stores nothing') return w('Recognised, but this effect is not calculated yet', '已识别，但尚未计算此效果', '已辨識，但尚未計算此效果');
  if (/^registered as (?:having no effect on any output|changing nothing a calculation reports \(.+\))$/.test(reason)) return w('This effect does not change a calculated value', '此效果不影响计算数值', '此效果不影響計算數值');
  return ledgerReasonLabel(reason, lang);
}
