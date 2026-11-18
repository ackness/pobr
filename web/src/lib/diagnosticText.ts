import { bindT, lookup, type Lang } from './i18n';

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
  const tt = bindT(lang);
  const tree = /^Tree:(\d+): ([\s\S]*)$/.exec(raw);
  if (tree) return { source: tt('diagnostics.passive', { node: tree[1] }), text: stripReferenceMarkup(tree[2]) };
  const config = /^Config: ([\s\S]*)$/.exec(raw);
  if (config) return { source: tt('diagnostics.setup'), text: stripReferenceMarkup(config[1]) };
  return { source: null, text: stripReferenceMarkup(raw) };
}

export function unsupportedLineLabel(raw: string, lang: Lang): string {
  const { source, text } = unsupportedLine(raw, lang);
  return source ? `${source}: ${text}` : text;
}

/** Short label for a stat-rejection `reason` code, or null when the code is not one the engine is known to emit. */
export function ledgerReasonLabel(reason: string, lang: Lang): string | null {
  const entry = /^([a-z_-]+)(?::[\s\S]*)?(?: \(part of the entry\))?$/.exec(reason);
  const label = entry && lookup(lang, 'rejectionReason', entry[1]);
  if (!label) return null;
  return reason.endsWith('(part of the entry)') ? bindT(lang)('diagnostics.wholeEntrySkipped', { label }) : label;
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
  const tt = bindT(lang);
  const exact = lookup(lang, 'modifierReason', reason);
  if (exact) return exact;
  const condition = /^(?:Condition (\w+) not met|needs (\w+))$/.exec(reason);
  if (condition) {
    const id = condition[1] ?? condition[2];
    const known: Record<string, string> = {
      LowLife: tt('diagnostics.lowLife'), Combat: tt('condition.combat'),
      Buffs: tt('condition.buffs'), Effective: tt('diagnostics.enemyConditions'),
      EnemyPoisoned: tt('condition.enemyPoisoned'), EnemyChilled: tt('diagnostics.enemyChilled'),
      AncestralBoost: tt('condition.ancestralBoost'), Attack: tt('diagnostics.attackSkill'),
    };
    const label = known[id] ?? conditions[id];
    return label ? tt('diagnostics.unmetCondition', { condition: label }) : null;
  }
  if (/^SkillType \w+(?: · SkillType \w+)*$/.test(reason)) return tt('diagnostics.skillType');
  if (/^another OVERRIDE of \S+ wins$/.test(reason)) return tt('diagnostics.override');
  if (/^only the strongest \S+ counts, and another is stronger$/.test(reason)) return tt('diagnostics.strongest');
  if (/^no query counted it: the quer(?:y|ies) of \S+ ask in another context than the main skill's$/.test(reason)) return tt('diagnostics.otherSkill');
  if (/^\S+ is read as [A-Z, ]+ only, not as [A-Z]+$/.test(reason)) return tt('diagnostics.stacking');
  if (/^given to the minions; no minion's calculation reads .+$/.test(reason)) return tt('diagnostics.unusedByMinions');
  if (/^rule \S+ recognises it and stores nothing$/.test(reason) || reason === 'a rule recognises it and stores nothing') return tt('diagnostics.unstored');
  if (/^registered as (?:having no effect on any output|changing nothing a calculation reports \(.+\))$/.test(reason)) return tt('diagnostics.noEffect');
  return ledgerReasonLabel(reason, lang);
}
