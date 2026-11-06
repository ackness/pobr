/**
 * `translateLines` gives one output per input. The game writes some stat descriptions on several
 * lines (Mind Over Matter's two); when those lines are adjacent in a batch they translate as one.
 * Each line gets its own translated line when the line counts agree; otherwise the first line gets
 * the whole translation and the others come back `""`: their text is in the line before them.
 * Every consumer goes through these helpers so a covered line never shows as a blank row.
 */

/** Whether `translated` is the `""` of a non-blank `source` line covered by the line before it. */
export function isContinuation(source: string, translated: string | undefined): boolean {
  return translated === '' && source.trim() !== '';
}

/** A list's displayed lines: each translation (its source while missing), covered lines dropped. */
export function shownLines(sources: readonly string[], translated: readonly (string | undefined)[]): string[] {
  return sources.flatMap((source, index) => isContinuation(source, translated[index]) ? [] : [translated[index] ?? source]);
}

/** One line's text where its row cannot be dropped (a ledger row, a weighted stat, a search alias):
 * a covered line keeps its source text. */
export function lineText(source: string, translated: string | undefined): string {
  return translated === undefined || isContinuation(source, translated) ? source : translated;
}

/** PoB wraps some rune descriptions mid-sentence. Keep positions for classified
 * rows, but send the complete effect to the language pack before translating. */
export function mergeModifierContinuations(lines: readonly string[]): string[] {
  const merged = [...lines];
  for (let index = 0; index < merged.length; index++) {
    if (!/^Bonded:\s*/.test(merged[index])) continue;
    let next = index + 1;
    while (next < merged.length && /\b(?:a|an|the|to|of|and|or|with|gain)\s*$/i.test(merged[index])
      && /^[a-z]/.test(merged[next]) && !/^[a-z]+:/.test(merged[next])) {
      merged[index] += ` ${merged[next].trim()}`;
      merged[next++] = '';
    }
  }
  return merged;
}
