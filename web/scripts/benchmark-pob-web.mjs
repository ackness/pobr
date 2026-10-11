import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { runInNewContext } from 'node:vm';
import { isMainThread, parentPort, Worker, workerData } from 'node:worker_threads';

export const SOURCE_COMMIT = 'ee33ce63920f23e634cc30e37d8ad339244dbf34';
const SOURCE_REPOSITORY = 'https://github.com/krauthaufen/pob-web';
const SOURCE_FILES = {
  'calc-worker.ts': { path: 'packages/web/src/worker/calc-worker.ts', sha256: 'c7622d26dfa997355db80f56da91ea4c0372932a3c456b6f45de96f523a338bd' },
  'lua-shims.ts': { path: 'packages/web/src/worker/lua-shims.ts', sha256: '270e27f9520e1f8199e336f7652114681053575f93dbd60df1c7c560cc733d9b' },
  LICENSE: { path: 'LICENSE', sha256: 'a6dff6522ae480e4130fe19cd7a7bbd8472825345d75cc8aef613211c628be78' },
};
const DEPLOYED_FILES = { 'lua.mjs': 'wasm/lua.mjs', 'lua.wasm': 'wasm/lua.wasm', 'pob-lua.json': 'data/pob-lua.json' };
const FILE_NAMES = [...Object.keys(DEPLOYED_FILES), ...Object.keys(SOURCE_FILES)].sort();
export const DEFAULT_REFERENCE_DIRECTORY = fileURLToPath(new URL('../../.cache/pob-web-benchmark', import.meta.url));
const hash = bytes => createHash('sha256').update(bytes).digest('hex');

function safeRelativePath(path) {
  return typeof path === 'string' && path.length > 0 && !path.includes('\\') && !path.includes('\0')
    && path.split('/').every(part => part && part !== '.' && part !== '..') && !path.includes(':');
}

// Parse only literal TypeScript templates and a concatenation of their names.
// Never evaluate the downloaded TypeScript or permit ${...} to execute.
function template(source, name) {
  const pattern = new RegExp(`(?:^|\\n)(?:export )?const ${name} = \\x60`, 'g');
  const match = pattern.exec(source);
  if (!match || pattern.exec(source)) throw new Error(`Expected exactly one literal ${name}`);
  let value = '';
  let index = match.index + match[0].length;
  for (; index < source.length; index++) {
    const char = source[index];
    if (char === '$' && source[index + 1] === '{') throw new Error(`Unsafe interpolation in ${name}`);
    if (char === '`') {
      if (!/^\s*;/.test(source.slice(index + 1))) throw new Error(`Unsafe expression after ${name}`);
      return value;
    }
    if (char !== '\\') { value += char; continue; }
    const escape = source[++index];
    const escapes = { n: '\n', r: '\r', t: '\t', b: '\b', f: '\f', v: '\v', '0': '\0', '\\': '\\', '`': '`', '$': '$', '"': '"', "'": "'" };
    if (escape === '\n') continue;
    if (escape === '\r') { if (source[index + 1] === '\n') index++; continue; }
    if (escape === 'x' || escape === 'u') {
      const count = escape === 'x' ? 2 : 4;
      const digits = source.slice(index + 1, index + 1 + count);
      if (!new RegExp(`^[0-9a-fA-F]{${count}}$`).test(digits)) throw new Error(`Unsupported escape in ${name}`);
      value += String.fromCharCode(Number.parseInt(digits, 16));
      index += count;
    } else if (Object.hasOwn(escapes, escape)) {
      if (escape === '0' && /[0-9]/.test(source[index + 1] ?? '')) throw new Error(`Unsafe octal escape in ${name}`);
      value += escapes[escape];
    } else if (escape && !/[0-9]/.test(escape)) value += escape;
    else throw new Error(`Unsupported escape in ${name}`);
  }
  throw new Error(`Unterminated ${name}`);
}

export function extractLuaTemplates(workerSource, shimSource) {
  const wrapper = template(workerSource, 'HEADLESS_WRAPPER');
  const matches = [...shimSource.matchAll(/(?:^|\n)export const ALL_SHIMS = ([^;]+);/g)];
  if (matches.length !== 1 || !/^[A-Z_][A-Z_0-9]*(?:\s*\+\s*[A-Z_][A-Z_0-9]*)*$/.test(matches[0][1])) {
    throw new Error('Unsafe ALL_SHIMS expression');
  }
  const names = matches[0][1].split('+').map(name => name.trim());
  if (new Set(names).size !== names.length) throw new Error('Duplicate ALL_SHIMS component');
  return { wrapper, shims: names.map(name => template(shimSource, name)).join('') };
}

function readManifest(directory) {
  const manifest = JSON.parse(readFileSync(join(directory, 'provenance.json'), 'utf8'));
  if (manifest.version !== 1 || manifest.source?.commit !== SOURCE_COMMIT
      || manifest.source?.repository !== SOURCE_REPOSITORY || manifest.deployed?.revision !== null
      || !Number.isFinite(Date.parse(manifest.acquiredAt)) || !Array.isArray(manifest.files)) {
    throw new Error('Invalid pob-web provenance (source revision is pinned; deployed revision must be unknown)');
  }
  let base;
  try { base = new URL(manifest.deployed.baseUrl); } catch { throw new Error('Invalid deployed base URL'); }
  if (!['https:', 'http:'].includes(base.protocol) || base.username || base.password || base.search || base.hash) {
    throw new Error('Invalid deployed base URL');
  }
  const names = manifest.files.map(file => file.name).sort();
  if (JSON.stringify(names) !== JSON.stringify(FILE_NAMES)) throw new Error('Incomplete or unsafe pob-web provenance files');
  for (const file of manifest.files) {
    const source = SOURCE_FILES[file.name];
    const expectedUrl = source
      ? `https://raw.githubusercontent.com/krauthaufen/pob-web/${SOURCE_COMMIT}/${source.path}`
      : new URL(DEPLOYED_FILES[file.name], base).href;
    if (file.url !== expectedUrl || !Number.isSafeInteger(file.bytes) || file.bytes <= 0
        || !/^[0-9a-f]{64}$/.test(file.sha256) || (source && file.sha256 !== source.sha256)) {
      throw new Error(`Invalid provenance for ${file.name}`);
    }
  }
  return manifest;
}

// Verification is also performed inside the isolated worker before import/boot.
export function verifyReference({ directory = DEFAULT_REFERENCE_DIRECTORY } = {}) {
  const provenance = readManifest(directory);
  const files = new Map();
  for (const record of provenance.files) {
    const path = join(directory, record.name);
    if (!statSync(path).isFile()) throw new Error(`Reference artifact is not a file: ${record.name}`);
    const bytes = readFileSync(path);
    if (bytes.length !== record.bytes || hash(bytes) !== record.sha256) throw new Error(`Reference hash/size mismatch: ${record.name}`);
    files.set(record.name, bytes);
  }
  const bundle = JSON.parse(files.get('pob-lua.json').toString('utf8'));
  if (!bundle.files || typeof bundle.files !== 'object' || Array.isArray(bundle.files)
      || !Object.keys(bundle.files).length) throw new Error('Invalid pob-web Lua bundle');
  for (const [path, text] of Object.entries(bundle.files)) {
    if (!safeRelativePath(path) || typeof text !== 'string') throw new Error(`Unsafe Lua bundle path/content: ${path}`);
  }
  const templates = extractLuaTemplates(files.get('calc-worker.ts').toString('utf8'), files.get('lua-shims.ts').toString('utf8'));
  return { provenance, files, bundle, templates };
}

export async function prepareReference({ directory = DEFAULT_REFERENCE_DIRECTORY, sourceDirectory, baseUrl = 'https://pob.awx.at' } = {}) {
  directory = resolve(directory);
  const base = new URL(`${baseUrl.replace(/\/$/, '')}/`);
  if (!['https:', 'http:'].includes(base.protocol) || base.username || base.password || base.search || base.hash) throw new Error('Invalid base URL');
  if (existsSync(directory)) {
    const provenance = verifyReference({ directory }).provenance;
    if (provenance.deployed.baseUrl !== base.href) throw new Error('Existing reference uses a different base URL; use a separate directory');
    return provenance;
  }
  if (sourceDirectory) {
    const commit = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: sourceDirectory, encoding: 'utf8' }).trim();
    if (commit !== SOURCE_COMMIT) throw new Error(`Expected pob-web source commit ${SOURCE_COMMIT}, received ${commit}`);
  }
  mkdirSync(dirname(directory), { recursive: true });
  const staging = `${directory}.preparing-${process.pid}-${Date.now()}`;
  mkdirSync(staging);
  try {
    const records = [];
    for (const name of FILE_NAMES) {
      const source = SOURCE_FILES[name];
      const url = source ? `https://raw.githubusercontent.com/krauthaufen/pob-web/${SOURCE_COMMIT}/${source.path}` : new URL(DEPLOYED_FILES[name], base).href;
      let bytes;
      let response;
      if (source && sourceDirectory) {
        bytes = execFileSync('git', ['show', `${SOURCE_COMMIT}:${source.path}`], { cwd: sourceDirectory, maxBuffer: 16 * 1024 * 1024 });
      } else {
        response = await fetch(url, { signal: AbortSignal.timeout(120_000) });
        if (!response.ok) throw new Error(`Download failed: ${url} (${response.status})`);
        bytes = Buffer.from(await response.arrayBuffer());
      }
      if (!bytes.length || (source && hash(bytes) !== source.sha256)) throw new Error(`Empty or unpinned source artifact: ${name}`);
      writeFileSync(join(staging, name), bytes, { flag: 'wx' });
      records.push({ name, url, bytes: bytes.length, sha256: hash(bytes), acquiredFrom: source && sourceDirectory ? 'pinned local git object' : 'download',
        responseUrl: response?.url ?? null, etag: response?.headers.get('etag') ?? null, lastModified: response?.headers.get('last-modified') ?? null });
    }
    const provenance = { version: 1, acquiredAt: new Date().toISOString(),
      source: { repository: SOURCE_REPOSITORY, commit: SOURCE_COMMIT },
      deployed: { baseUrl: base.href, revision: null, note: 'Downloaded deployed artifacts; their build/source revision is unknown. The source commit pins only the wrapper, shims and loading patches, not these binaries or Lua data.' },
      adapter: { host: 'Node isolated worker; unchanged Emscripten ES module with wasmBinary supplied',
        patches: 'Original pinned loadPobFiles body, including dkjson and FrostShield patches',
        initializationTiming: 'Excludes worker startup, artifact reads/hash checks and JSON decoding; includes ES module import I/O, WASM instantiation, upstream MEMFS population and Lua boot',
        calculation: 'pobWebLoadBuild(XML) then pobWebGetStats; no skill auto-selection or numerical parity assertion' }, files: records };
    writeFileSync(join(staging, 'provenance.json'), `${JSON.stringify(provenance, null, 2)}\n`, { flag: 'wx' });
    verifyReference({ directory: staging });
    if (existsSync(directory)) throw new Error('Reference directory appeared during preparation; refusing to replace it');
    renameSync(staging, directory);
    return provenance;
  } finally { rmSync(staging, { recursive: true, force: true }); }
}

export function validatePobWebStats(raw) {
  const stats = JSON.parse(raw);
  if (!stats || typeof stats !== 'object' || Array.isArray(stats) || stats.error) throw new Error(`pob-web stats failure: ${stats?.error ?? raw}`);
  if (!Number.isFinite(stats.Life) || stats.Life <= 0) throw new Error('pob-web did not produce finite positive Life');
  for (const key of ['Mana', 'EnergyShield', 'Armour', 'Evasion', 'FireResist', 'ColdResist', 'LightningResist']) {
    if (!Number.isFinite(stats[key])) throw new Error(`Incomplete pob-web output: ${key}`);
  }
  if (!Array.isArray(stats._allocatedNodes)) throw new Error('Incomplete pob-web output: _allocatedNodes');
  return stats;
}

// This is the original upstream loader, not a second implementation of its Lua patches.
// Its sole TS-only syntax is removed after the entire source has been hash-verified.
function upstreamFileLoader(source, bundle) {
  const start = source.indexOf('async function loadPobFiles(FS: any): Promise<number> {');
  const end = source.indexOf('\n}\n', start);
  if (start < 0 || end < 0) throw new Error('Pinned upstream loadPobFiles declaration not found');
  const loader = source.slice(start, end + 2)
    .replace('loadPobFiles(FS: any): Promise<number>', 'loadPobFiles(FS)')
    .replace('content as string', 'content');
  const mkdirp = (FS, path) => {
    let current = '';
    for (const part of path.split('/').filter(Boolean)) {
      current += `/${part}`;
      if (!FS.analyzePath(current).exists) FS.mkdir(current);
    }
  };
  return runInNewContext(`(${loader})`, {
    log() {}, mkdirp,
    fetch: async url => {
      if (url !== '/data/pob-lua.json') throw new Error(`Unexpected upstream fetch: ${url}`);
      return { ok: true, json: async () => bundle };
    },
    writeFile: (FS, path, text) => {
      if (!path.startsWith('/pob/') || !safeRelativePath(path.slice(5))) throw new Error(`Unsafe MEMFS path: ${path}`);
      mkdirp(FS, path.slice(0, path.lastIndexOf('/')));
      FS.writeFile(path, text);
    },
  }, { timeout: 1000 });
}

async function workerEngine(directory) {
  const reference = verifyReference({ directory });
  const started = performance.now();
  const createModule = (await import(pathToFileURL(join(directory, 'lua.mjs')).href)).default;
  const logs = [];
  // bridge.c's Lua print override calls console.log directly, bypassing Module.print.
  // This worker owns its console; capture both routes so swallowed Lua failures cannot pass.
  for (const [method, stream] of [['log', 'stdout'], ['warn', 'stderr'], ['error', 'stderr']]) {
    console[method] = (...values) => logs.push({ stream, text: values.map(String).join(' ') });
  }
  const Module = await createModule({ wasmBinary: reference.files.get('lua.wasm'),
    print: text => logs.push({ stream: 'stdout', text: String(text) }),
    printErr: text => logs.push({ stream: 'stderr', text: String(text) }) });
  const init = Module.cwrap('bridge_init', 'number', []);
  const exec = Module.cwrap('bridge_exec', 'string', ['string']);
  const call = Module.cwrap('bridge_call_json', 'string', ['string', 'string']);
  if (init() !== 0) throw new Error('pob-web bridge_init failed');
  const checkedExec = code => { const error = exec(code); if (error) throw new Error(`pob-web bridge_exec: ${error}`); };
  checkedExec(reference.templates.shims);
  await upstreamFileLoader(reference.files.get('calc-worker.ts').toString('utf8'), reference.bundle)(Module.FS);
  checkedExec('package.path = "/pob/runtime/lua/?.lua;/pob/runtime/lua/?/init.lua;" .. package.path');
  Module.FS.chdir('/pob/src');
  checkedExec(reference.templates.wrapper);
  const initializationMs = performance.now() - started;
  const initializationLogs = logs.splice(0);
  const seenNonces = new Set();
  return { initializationMs, provenance: { ...reference.provenance, initializationLogs },
    calculate({ xml, nonce }) {
      if (typeof xml !== 'string' || !xml.trim() || !/<PathOfBuilding2(?:\s|>)/.test(xml)) throw new Error('Expected nonempty PathOfBuilding2 XML');
      if ((typeof nonce !== 'string' && typeof nonce !== 'number') || (typeof nonce === 'number' && !Number.isFinite(nonce))
          || !String(nonce).length || seenNonces.has(String(nonce))) throw new Error('Expected unique calculation nonce');
      seenNonces.add(String(nonce));
      logs.length = 0;
      const name = `benchmark-${nonce}`;
      const luaName = `"${name.replace(/[\x00-\x1f\x7f"\\]/g, char => `\\${char.charCodeAt(0).toString().padStart(3, '0')}`)}"`;
      const rawLoad = call('pobWebLoadBuild', JSON.stringify({ xml, name }));
      const loaded = JSON.parse(rawLoad);
      if (!loaded || loaded.error || loaded.success !== true || !Array.isArray(loaded.allocatedNodes)) throw new Error(`pob-web load failure: ${rawLoad}`);
      const raw = call('pobWebGetStats', '{}');
      const failures = logs.filter(entry => entry.stream === 'stderr' || /error|failed|warning|non-fatal/i.test(entry.text));
      if (failures.length) throw new Error(`pob-web swallowed load/calculation error: ${failures.map(entry => entry.text).join('\n')}`);
      validatePobWebStats(raw);
      // Read-only bridge observation: reject stale/aborted imports, even if their
      // previous output still looks valid. No Lua calculation mechanics change.
      checkedExec(`assert(build and build.buildName == ${luaName}, "stale imported build")\nassert(build.xmlSectionList and #build.xmlSectionList > 0 and build.skillsTab and build.itemsTab and build.configTab and build.spec and build.calcsTab.mainEnv, "incomplete imported build")`);
      return raw;
    },
    close() { Module._bridge_destroy(); },
  };
}

export async function loadPobWeb({ directory = DEFAULT_REFERENCE_DIRECTORY } = {}) {
  const worker = new Worker(new URL(import.meta.url), { workerData: { pobWebBenchmark: true, directory: resolve(directory) }, execArgv: [] });
  const pending = new Map();
  let nextId = 0;
  let closed = false;
  let readyResolve;
  let readyReject;
  const ready = new Promise((resolve, reject) => { readyResolve = resolve; readyReject = reject; });
  const fail = error => { readyReject(error); for (const request of pending.values()) request.reject(error); pending.clear(); };
  worker.on('error', fail);
  worker.on('exit', code => { if (!closed) fail(new Error(`pob-web worker exited unexpectedly (${code})`)); });
  worker.on('message', message => {
    if (message.type === 'ready') { readyResolve(message); return; }
    if (message.type === 'fatal') { fail(new Error(message.error)); return; }
    const request = pending.get(message.id);
    if (!request) return;
    pending.delete(message.id);
    if (message.error) request.reject(new Error(message.error)); else request.resolve(message.raw);
  });
  try {
    const info = await ready;
    return { name: 'pob-web', initializationMs: info.initializationMs, provenance: info.provenance,
      calculate(request) {
        if (closed) return Promise.reject(new Error('pob-web engine is closed'));
        const id = ++nextId;
        return new Promise((resolve, reject) => { pending.set(id, { resolve, reject }); worker.postMessage({ id, request }); });
      },
      close() {
        if (closed) return;
        closed = true;
        fail(new Error('pob-web engine closed'));
        return worker.terminate();
      },
    };
  } catch (error) { closed = true; await worker.terminate(); throw error; }
}

if (!isMainThread && workerData?.pobWebBenchmark) {
  try {
    const engine = await workerEngine(workerData.directory);
    parentPort.postMessage({ type: 'ready', initializationMs: engine.initializationMs, provenance: engine.provenance });
    parentPort.on('message', ({ id, request }) => {
      try { parentPort.postMessage({ id, raw: engine.calculate(request) }); }
      catch (error) { parentPort.postMessage({ id, error: error.stack ?? String(error) }); }
    });
    parentPort.on('close', () => engine.close());
  } catch (error) { parentPort.postMessage({ type: 'fatal', error: error.stack ?? String(error) }); parentPort.close(); }
}

const HELP = `Usage: node web/scripts/benchmark-pob-web.mjs --prepare [--directory PATH] [--source-dir PATH] [--base-url URL]

Downloads the deployed lua.mjs, lua.wasm and pob-lua.json, plus the pinned
upstream wrapper/shims and LICENSE. No local clone is required. --source-dir
uses git objects from the exact pinned commit instead of downloading sources.
An existing directory is reused offline only after all hashes are verified;
partial or altered directories are errors. Use a separate directory to acquire
new deployed artifacts. Deployment revision is unknown, not the wrapper commit.
`;

if (isMainThread && process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const args = process.argv.slice(2);
    if (args.length === 1 && ['--help', '-h'].includes(args[0])) process.stdout.write(HELP);
    else {
      const options = {};
      let prepare = false;
      for (let index = 0; index < args.length; index++) {
        const arg = args[index];
        if (arg === '--prepare' && !prepare) { prepare = true; continue; }
        const key = { '--directory': 'directory', '--source-dir': 'sourceDirectory', '--base-url': 'baseUrl' }[arg];
        if (!key || options[key] !== undefined || !args[index + 1] || args[index + 1].startsWith('--')) throw new Error(`Invalid argument: ${arg}`);
        options[key] = args[++index];
      }
      if (!prepare) throw new Error('Expected --prepare; use --help for usage');
      process.stdout.write(`${JSON.stringify(await prepareReference(options), null, 2)}\n`);
    }
  } catch (error) { console.error(error.stack ?? String(error)); process.exitCode = 1; }
}
