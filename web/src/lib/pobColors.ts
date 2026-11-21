/**
 * PoB text color code parser (`^N` single digit / `^xRRGGBB` hex).
 *
 * Used for Notes preview highlighting; the palette matches SimpleGraphic's built-in `^0`–`^9` colors.
 */

/** SimpleGraphic built-in `^0`–`^9` palette. */
const DIGIT_PALETTE: Record<string, string> = {
  '0': '#000000',
  '1': '#ff0000',
  '2': '#00ff00',
  '3': '#0000ff',
  '4': '#ffff00',
  '5': '#ff00ff',
  '6': '#00ffff',
  '7': '#ffffff',
  '8': '#b3b3b3',
  '9': '#666666',
};

const CODE_RE = /\^x([0-9A-Fa-f]{6})|\^([0-9])/g;

export interface ColorSegment {
  /** CSS color; null = unspecified (inherit default foreground color). */
  color: string | null;
  text: string;
}

/** Whether the text contains color codes (gates whether Notes shows colored preview). */
export function hasPobColorCodes(text: string): boolean {
  CODE_RE.lastIndex = 0;
  return CODE_RE.test(text);
}

/** Split text with color codes into styled segments (codes are stripped and apply to following text until the next code). */
export function parsePobColorText(text: string): ColorSegment[] {
  const segments: ColorSegment[] = [];
  let color: string | null = null;
  let last = 0;
  CODE_RE.lastIndex = 0;
  for (let m = CODE_RE.exec(text); m; m = CODE_RE.exec(text)) {
    if (m.index > last) {
      segments.push({ color, text: text.slice(last, m.index) });
    }
    color = m[1] ? `#${m[1].toLowerCase()}` : DIGIT_PALETTE[m[2]];
    last = m.index + m[0].length;
  }
  if (last < text.length) {
    segments.push({ color, text: text.slice(last) });
  }
  return segments;
}
