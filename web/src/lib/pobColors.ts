/**
 * PoB 文本颜色码解析（`^N` 单数字 / `^xRRGGBB` 十六进制）。
 *
 * Notes 页着色预览用；调色板与 SimpleGraphic 引擎的 `^0`–`^9` 内置色一致。
 */

/** SimpleGraphic 内置 `^0`–`^9` 十色。 */
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
  /** CSS 颜色；null = 未指定（用默认前景色）。 */
  color: string | null;
  text: string;
}

/** 文本是否含颜色码（决定 Notes 页要不要显示着色预览）。 */
export function hasPobColorCodes(text: string): boolean {
  CODE_RE.lastIndex = 0;
  return CODE_RE.test(text);
}

/** 把带颜色码的文本切成着色段（颜色码自身被吃掉，后续文本继承该色直到下一个码）。 */
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
