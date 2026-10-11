import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { extractLuaTemplates, prepareReference, SOURCE_COMMIT, validatePobWebStats, verifyReference } from './benchmark-pob-web.mjs';

function fixture(t) {
  const directory = mkdtempSync(join(tmpdir(), 'pobr Lua benchmark '));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const write = manifest => writeFileSync(join(directory, 'provenance.json'), JSON.stringify(manifest));
  return { directory, write };
}

const literalSources = {
  worker: 'const HEADLESS_WRAPPER = `print("Lua only")\n`;',
  shims: 'export const FIRST = `one\\n`;\nexport const SECOND = `two`;\nexport const ALL_SHIMS = SECOND + FIRST;',
};

test('extracts original template contents and upstream concatenation order without evaluation', () => {
  assert.deepEqual(extractLuaTemplates(literalSources.worker, literalSources.shims), {
    wrapper: 'print("Lua only")\n', shims: 'twoone\n',
  });
  assert.deepEqual(extractLuaTemplates('const HEADLESS_WRAPPER = `a\\`b\\\\c\\x41\\u0042`;', literalSources.shims), {
    wrapper: 'a`b\\cAB', shims: 'twoone\n',
  });
});

test('shim identifiers permit digits after the first character, as in upstream UTF8_SHIM', () => {
  const shims = 'export const UTF8_SHIM = `utf8`;\nexport const ALL_SHIMS = UTF8_SHIM;';
  assert.equal(extractLuaTemplates(literalSources.worker, shims).shims, 'utf8');
  assert.throws(() => extractLuaTemplates(literalSources.worker,
    'export const ALL_SHIMS = 8_SHIM;'), /Unsafe ALL_SHIMS/);
});

test('rejects interpolation, executable concatenation, missing and duplicate literal declarations', () => {
  const workers = [
    'const HEADLESS_WRAPPER = `${globalThis.sideEffect = true}`;',
    'const HEADLESS_WRAPPER = `safe` + execute();',
    'const HEADLESS_WRAPPER = execute();',
    'const HEADLESS_WRAPPER = `one`;\nconst HEADLESS_WRAPPER = `two`;',
    'const HEADLESS_WRAPPER = `unterminated',
    'const HEADLESS_WRAPPER = `\\123`;',
  ];
  for (const worker of workers) assert.throws(() => extractLuaTemplates(worker, literalSources.shims));
  const shims = [
    'export const ALL_SHIMS = execute();',
    'export const FIRST = `${execute()}`;\nexport const ALL_SHIMS = FIRST;',
    'export const FIRST = `x`;\nexport const ALL_SHIMS = FIRST + FIRST;',
    'export const ALL_SHIMS = MISSING;',
    `${literalSources.shims}\nexport const ALL_SHIMS = FIRST;`,
  ];
  for (const source of shims) assert.throws(() => extractLuaTemplates(literalSources.worker, source));
});

function manifest() {
  const source = {
    'calc-worker.ts': ['packages/web/src/worker/calc-worker.ts', 'c7622d26dfa997355db80f56da91ea4c0372932a3c456b6f45de96f523a338bd'],
    'lua-shims.ts': ['packages/web/src/worker/lua-shims.ts', '270e27f9520e1f8199e336f7652114681053575f93dbd60df1c7c560cc733d9b'],
    LICENSE: ['LICENSE', 'a6dff6522ae480e4130fe19cd7a7bbd8472825345d75cc8aef613211c628be78'],
  };
  return {
    version: 1, acquiredAt: '2026-10-11T00:00:00.000Z',
    source: { repository: 'https://github.com/krauthaufen/pob-web', commit: SOURCE_COMMIT },
    deployed: { baseUrl: 'https://pob.awx.at/', revision: null },
    files: [
      ...Object.entries({ 'lua.mjs': 'wasm/lua.mjs', 'lua.wasm': 'wasm/lua.wasm', 'pob-lua.json': 'data/pob-lua.json' })
        .map(([name, path]) => ({ name, bytes: 8, sha256: createHash('sha256').update('original').digest('hex'), url: `https://pob.awx.at/${path}` })),
      ...Object.entries(source).map(([name, [path, sha256]]) => ({ name, bytes: 1, sha256,
        url: `https://raw.githubusercontent.com/krauthaufen/pob-web/${SOURCE_COMMIT}/${path}` })),
    ],
  };
}

test('rejects tampered revision claims and unsafe/incomplete provenance before loading code', t => {
  const { directory, write } = fixture(t);
  const changes = [
    value => { value.source.commit = 'f'.repeat(40); },
    value => { value.deployed.revision = SOURCE_COMMIT; },
    value => { value.files[0].name = '../lua.mjs'; },
    value => { value.files.pop(); },
    value => { value.files[0].url = 'https://untrusted.invalid/lua.mjs'; },
    value => { value.files[3].sha256 = '0'.repeat(64); },
    value => { value.files[0].bytes = -1; },
  ];
  for (const change of changes) {
    const value = manifest();
    change(value);
    write(value);
    assert.throws(() => verifyReference({ directory }), /provenance|Incomplete|Invalid/);
  }
});

test('rejects altered artifact bytes instead of importing deployed JavaScript', t => {
  const { directory, write } = fixture(t);
  write(manifest());
  writeFileSync(join(directory, 'lua.mjs'), 'tampered');
  assert.throws(() => verifyReference({ directory }), /hash\/size mismatch: lua.mjs/);
});

test('existing partial downloads fail preparation without fetching or repairing them', async t => {
  const { directory } = fixture(t);
  await assert.rejects(prepareReference({ directory }), /ENOENT/);
});

test('stats validation rejects bridge errors and positive-Life partial calculations', () => {
  const complete = { Life: 100, Mana: 50, EnergyShield: 0, Armour: 0, Evasion: 0,
    FireResist: -60, ColdResist: -60, LightningResist: -60, _allocatedNodes: [] };
  assert.deepEqual(validatePobWebStats(JSON.stringify(complete)), complete);
  for (const raw of ['not json', 'null', '[]', '{"error":"bridge failed"}', '{"Life":100}', JSON.stringify({ ...complete, Life: 0 }), JSON.stringify({ ...complete, Mana: null })]) {
    assert.throws(() => validatePobWebStats(raw));
  }
});

test('prepare CLI rejects invalid arguments with nonzero exit and no network acquisition', () => {
  const script = fileURLToPath(new URL('./benchmark-pob-web.mjs', import.meta.url));
  for (const args of [['--unknown'], ['--prepare', '--directory'], ['--prepare', '--prepare']]) {
    assert.throws(() => execFileSync(process.execPath, [script, ...args], { stdio: 'pipe' }), error => error.status !== 0);
  }
});
