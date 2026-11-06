/** Strip `[A|B]` / `[A]` display markup from game text. */
export function cleanLineText(text: string): string {
  return text.replace(/\[([^\]|]*)\|([^\]]*)\]/g, '$2').replace(/\[([^\]]*)\]/g, '$1');
}
