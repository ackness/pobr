import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { basename, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { inflateSync } from 'node:zlib';

const root = fileURLToPath(new URL('../../', import.meta.url));
const defaultDirectories = [
  resolve(root, 'examples/demo-bd-test/builds'),
  resolve(root, 'examples/wasm-benchmark/builds'),
];
// Historical comparison cohort retained after the user requested removal of
// failing/unstable old builds. Their shared parity fixtures remain untouched.
const historicalFixtures = new Set([
  'huntress-spirit-walker-twister',
  'mercenary-gemling-legionnaire-explosive-grenade',
  'ranger-deadeye-explosive-grenade',
  'ranger-pathfinder-ice-shot',
  'warrior-smith-of-kitava-shield-wall',
  'warrior-titan-shield-wall',
  'witch-abyssal-lich-detonate-dead',
]);
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const text = value => typeof value === 'string' && value.trim().length > 0;

function validateXml(xml) {
  // Actual engines validate XML grammar and the calculation contract, retaining
  // failures in the report. Discovery checks the decoded PoB2 envelope.
  if (!/^\s*(?:<\?xml[^?]*\?>\s*)?<PathOfBuilding2(?:\s[^>]*|)>[\s\S]*<\/PathOfBuilding2>\s*$/.test(xml)
      || !/<Build(?:\s|\/?>)/.test(xml)) throw new Error('Expected nonempty PathOfBuilding2 XML with a Build element');
}

async function readBuild(directory, fixture, corpus) {
  const path = resolve(directory, fixture);
  const raw = await readFile(resolve(path, 'meta.json'), 'utf8');
  // Historical Python metadata may contain non-JSON numeric tokens. Preserve
  // quoted strings, including source URLs and character names, verbatim.
  const meta = JSON.parse(raw.replace(/"(?:[^"\\]|\\.)*"|-?Infinity|\bNaN\b/g,
    token => token.startsWith('"') ? token : 'null'));
  if (!object(meta) || !text(meta.name) || !object(meta.source)
      || !object(meta.character) || !object(meta.pob)) throw new Error('Malformed fixture metadata');
  if (meta.source.site !== 'poe.ninja') throw new Error('Missing poe.ninja source metadata');
  // One committed legacy fixture has unknown source details. Preserve those nulls
  // rather than inventing a league/date or excluding a valid historical build.
  if (corpus === 'ninja-latest-season'
      && [meta.source.url, meta.source.league, meta.source.fetched_at].some(value => !text(value))) {
    throw new Error('Missing latest-season source metadata');
  }
  if (meta.source.url != null) {
    const url = new URL(meta.source.url);
    if (url.origin !== 'https://poe.ninja' || !url.pathname.startsWith('/poe2/builds/')) {
      throw new Error('Invalid poe.ninja source URL');
    }
  }
  if (!text(meta.character.class) || !text(meta.character.ascendancy)
      || !Number.isInteger(meta.character.level) || meta.character.level <= 0
      || !text(meta.character.main_skill)) throw new Error('Malformed character metadata');
  if (typeof meta.pob.code_sha256 !== 'string' || !/^[a-f0-9]{64}$/i.test(meta.pob.code_sha256)) {
    throw new Error('Missing or malformed pob.code_sha256');
  }
  const code = (await readFile(resolve(path, 'code.txt'), 'utf8')).trim();
  if (!code || !/^[A-Za-z0-9_-]+={0,2}$/.test(code) || code.replace(/=+$/, '').length % 4 === 1) {
    throw new Error('Empty or malformed base64url PoB code');
  }
  const hash = sha256(code);
  if (hash !== meta.pob.code_sha256.toLowerCase()) throw new Error('PoB code SHA-256 mismatch');
  const bytes = inflateSync(Buffer.from(code, 'base64url'));
  const xml = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  validateXml(xml);
  return { fixture, code, xml, sha256: hash, xmlSha256: sha256(bytes),
    character: meta.character, source: meta.source, corpus };
}

// All latest-season and explicitly selected historical fixtures are required.
// Selection never changes automatically in response to calculation failures.
export async function discoverBuilds(directories = defaultDirectories) {
  if (!Array.isArray(directories) || directories.length === 0
      || directories.some(directory => !text(directory))) throw new Error('Build directories must be nonempty paths');
  const builds = [];
  const failures = [];
  const seen = new Set();
  const latestLeagues = new Set();
  for (const directory of [...directories].map(directory => resolve(directory)).sort()) {
    const corpus = directory === defaultDirectories[0] ? 'historical'
      : directory === defaultDirectories[1] ? 'ninja-latest-season' : basename(resolve(directory, '..'));
    let entries;
    try {
      entries = await readdir(directory, { withFileTypes: true });
    } catch (error) {
      failures.push(`${directory}: Cannot read required build root: ${error.message}`);
      continue;
    }
    if (corpus === 'historical') {
      const present = new Set(entries.map(entry => entry.name));
      for (const fixture of historicalFixtures) {
        if (!present.has(fixture)) failures.push(`${directory}/${fixture}: Missing selected historical fixture`);
      }
      entries = entries.filter(entry => historicalFixtures.has(entry.name));
    }
    const candidates = entries.filter(entry => entry.isDirectory() || entry.isSymbolicLink())
      .sort((a, b) => a.name < b.name ? -1 : a.name > b.name ? 1 : 0);
    if (candidates.length === 0) failures.push(`${directory}: No fixture directories found`);
    for (const entry of candidates) {
      const identity = `${directory}/${entry.name}`;
      if (seen.has(entry.name)) {
        failures.push(`${identity}: Duplicate fixture identity: ${entry.name}`);
        continue;
      }
      seen.add(entry.name);
      try {
        if (entry.isSymbolicLink()) throw new Error('Fixture symlinks are not supported');
        const build = await readBuild(directory, entry.name, corpus);
        builds.push(build);
        if (corpus === 'ninja-latest-season') latestLeagues.add(build.source.league);
      } catch (error) {
        failures.push(`${identity}: ${error.message}`);
      }
    }
  }
  if (latestLeagues.size > 1) failures.push(`Latest-season fixtures disagree on league: ${[...latestLeagues].sort().join(', ')}`);
  if (failures.length) throw new Error(`Build discovery failed (${failures.length}):\n${failures.join('\n')}`);
  return builds.sort((a, b) => a.fixture < b.fixture ? -1 : a.fixture > b.fixture ? 1 : 0);
}
