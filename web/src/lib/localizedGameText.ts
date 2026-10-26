import { chineseScript, localizedName } from './chinese';
import type { Lang } from './i18n';

export interface GameDisplayName {
  name: string;
  name_zh_cn?: string | null;
  name_zh_tw?: string | null;
}

const escapeRegExp = (text: string): string => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** Uses the selected snapshot's names; conversion is only a fallback for missing
 * language data (notably TW stat-line templates in the current snapshot).
 * This index never modifies the original item, stat ID, or backend response. */
export class GameTextLocalizer {
  private readonly byEnglish = new Map<string, GameDisplayName>();
  private readonly byName = new Map<string, GameDisplayName>();
  private readonly mentionedNames: RegExp | null;

  constructor(names: GameDisplayName[]) {
    for (const entry of names) {
      const previous = this.byEnglish.get(entry.name);
      this.byEnglish.set(entry.name, {
        name: entry.name,
        name_zh_cn: entry.name_zh_cn || previous?.name_zh_cn,
        name_zh_tw: entry.name_zh_tw || previous?.name_zh_tw,
      });
    }
    const ambiguous = new Set<string>();
    for (const entry of this.byEnglish.values()) {
      for (const alias of [entry.name, entry.name_zh_cn, entry.name_zh_tw]) {
        if (!alias?.trim()) continue;
        const existing = this.byName.get(alias.trim());
        if (existing && existing.name !== entry.name) ambiguous.add(alias.trim());
        this.byName.set(alias.trim(), entry);
      }
    }
    for (const alias of ambiguous) this.byName.delete(alias);
    const alternatives = [...this.byEnglish.keys()].filter(Boolean).sort((a, b) => b.length - a.length).map(escapeRegExp);
    this.mentionedNames = alternatives.length
      ? new RegExp(`(?<![A-Za-z0-9])(?:${alternatives.join('|')})(?![A-Za-z0-9])`, 'g')
      : null;
  }

  localize(line: string, simplified: string, lang: Lang): string {
    if (lang === 'en-US') return line;
    const exact = this.byName.get(line.trim());
    if (exact) return localizedName(lang, exact.name, exact.name_zh_cn, exact.name_zh_tw);
    let display = chineseScript(simplified, lang);
    if (!this.mentionedNames) return display;
    const replacements = new Map<string, string>();
    // Only replace a name if the original line mentions it. Generic nouns that
    // also happen to be gem names must not rewrite unrelated translated prose.
    for (const name of line.match(this.mentionedNames) ?? []) {
      const entry = this.byEnglish.get(name)!;
      const target = localizedName(lang, entry.name, entry.name_zh_cn, entry.name_zh_tw);
      for (const alias of [entry.name, entry.name_zh_cn, entry.name_zh_tw]) {
        if (alias?.trim()) replacements.set(chineseScript(alias.trim(), lang), target);
      }
    }
    if (replacements.size) {
      const pattern = new RegExp([...replacements.keys()].sort((a, b) => b.length - a.length).map(escapeRegExp).join('|'), 'g');
      display = display.replace(pattern, name => replacements.get(name)!);
    }
    return display;
  }

  localizeLines(lines: readonly string[], simplified: readonly string[], lang: Lang): string[] {
    return lines.map((line, index) => this.localize(line, simplified[index] ?? line, lang));
  }
}
