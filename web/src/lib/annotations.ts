/**
 * 局部注释（装备/技能组/珠宝旁的「为什么选它」说明）与 PoB2 `<Notes>` 的互转。
 *
 * PoB2 code 格式没有逐部件注释的位置，因此把注释序列化成 Notes 末尾的一个
 * 标记段随 code 往返：PoB2 本体把它当普通笔记文本展示（可读），PoBR 导入时
 * 解析回结构化 map。标记段之前的部分是自由笔记（总览）。
 *
 * 注释 key 约定（也是导出文本里的段头）：
 *   `item:<slot>`（装备槽）/ `flask:<slot>`（药剂/护符槽）/
 *   `jewel:<socket_node>`（树插槽珠宝）/ `skill:<index>`（技能组序号，与
 *   socket_groups 数组序一致，encode/decode 保序所以可往返）。
 */

export type Annotations = Record<string, string>;

const MARKER = '===== PoBR Annotations =====';
const HEADER_RE = /^\[((?:item|flask|jewel|skill):[^\]]+)\]$/;

/** 总览笔记 + 注释 map → `<Notes>` 全文（无注释时原样返回总览）。 */
export function composeNotes(overview: string, annotations: Annotations): string {
  const entries = Object.entries(annotations).filter(([, text]) => text.trim());
  if (entries.length === 0) return overview;
  const blocks = entries.map(([key, text]) => `[${key}]\n${text.trim()}`);
  const head = overview.trimEnd();
  return `${head ? `${head}\n\n` : ''}${MARKER}\n${blocks.join('\n\n')}`;
}

/** `<Notes>` 全文 → 总览笔记 + 注释 map（无标记段时注释为空）。 */
export function splitNotes(notes: string): { overview: string; annotations: Annotations } {
  const lines = notes.split('\n');
  const markerIdx = lines.findIndex((l) => l.trim() === MARKER);
  if (markerIdx < 0) return { overview: notes, annotations: {} };

  const annotations: Annotations = {};
  let key: string | null = null;
  let buf: string[] = [];
  const flush = () => {
    if (key) annotations[key] = buf.join('\n').trim();
    buf = [];
  };
  for (const line of lines.slice(markerIdx + 1)) {
    const m = line.match(HEADER_RE);
    if (m) {
      flush();
      key = m[1];
    } else if (key) {
      buf.push(line);
    }
  }
  flush();
  return { overview: lines.slice(0, markerIdx).join('\n').trimEnd(), annotations };
}
