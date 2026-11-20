/**
 * Round-trip conversion between per-entity annotations ("why this choice" notes next to items/skills/jewels)
 * and PoB2 `<Notes>`.
 *
 * Because the PoB2 export format has no dedicated per-part note fields, annotations are serialized into
 * a marked section at the end of `<Notes>`: PoB2 displays it as readable plain text, and PoBR parses it
 * back into a structured map on import. Everything before the marker remains free-form overview notes.
 *
 * Annotation key convention (also used as section headers in exported text):
 *   `item:<slot>` (equipment slot) / `flask:<slot>` (flask/charm slot) /
 *   `jewel:<socket_node>` (passive tree jewel socket) / `skill:<index>` (socket group index, matching
 *   the `socket_groups` array order which encode/decode preserves for round-tripping).
 */

export type Annotations = Record<string, string>;

const MARKER = '===== PoBR Annotations =====';
const HEADER_RE = /^\[((?:item|flask|jewel|skill):[^\]]+)\]$/;

/** Overview notes + annotations map -> full `<Notes>` text (returns overview unchanged when empty). */
export function composeNotes(overview: string, annotations: Annotations): string {
  const entries = Object.entries(annotations).filter(([, text]) => text.trim());
  if (entries.length === 0) return overview;
  const blocks = entries.map(([key, text]) => `[${key}]\n${text.trim()}`);
  const head = overview.trimEnd();
  return `${head ? `${head}\n\n` : ''}${MARKER}\n${blocks.join('\n\n')}`;
}

/** Full `<Notes>` text -> overview notes + annotations map (annotations empty when marker is absent). */
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
