import { mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';

// Reuse the reviewed public parity corpus; never read a player's local saves.
export function syncBuildReferences() {
  const source = fileURLToPath(new URL('../../examples/demo-bd-test/builds/', import.meta.url));
  const destination = fileURLToPath(new URL('../public/build-references/', import.meta.url));
  const references = readdirSync(source).sort().flatMap(id => {
    const dir = join(source, id);
    // Some legacy Python fixtures contain non-finite numeric stats. Preserve all
    // quoted strings and normalize only the non-JSON numeric tokens.
    const meta = JSON.parse(readFileSync(join(dir, 'meta.json'), 'utf8')
      .replace(/"(?:[^"\\]|\\.)*"|-?Infinity|\bNaN\b/g, token => token.startsWith('"') ? token : 'null'));
    if (!meta.source.url || !meta.source.fetched_at) return [];
    const url = new URL(meta.source.url);
    if (url.origin !== 'https://poe.ninja' || !url.pathname.startsWith('/poe2/builds/')) throw new Error(`Invalid reference: ${id}`);
    return [{ id, source: { url: url.href, league: meta.source.league,
      gameVersion: meta.source.game_version, fetchedAt: meta.source.fetched_at },
      code: readFileSync(join(dir, 'code.txt'), 'utf8').trim() }];
  });
  mkdirSync(destination, { recursive: true });
  writeFileSync(join(destination, 'index.json'), JSON.stringify({ version: 1, references }));
  console.log(`synced ${references.length} dated poe.ninja build references`);
}
