// Opt-in browser measurement: the built site with its per-file data snapshot against the
// same site serving the compact runtime release. Never part of a test gate.
//
//   mise run web build-wasm && mise run web sync-data && pnpm --dir web exec vite build
//   python3 -m pipeline.runtime.runtime_pack build --data data --out .cache/runtime-pack
//   node web/scripts/bench-web-load.mjs --pack .cache/runtime-pack/player [--iterations 5]
//
// Each run launches a fresh headless Chromium (no HTTP or code cache), opens /build with no
// saved build and waits for the default character's first calculation. The local server sends
// precompressed (brotli) bodies with the host's cache headers, and optionally shapes the link
// (one shared FIFO link with a fixed bandwidth and a per-response delay). CPU throttling is
// DevTools' rate, applied to the page and to its calculation Worker. Memory is the summed PSS
// of the renderer processes (page and Worker, JS heaps and WASM memory), read from /proc (Linux).
// Results: docs/wasm-package.md#web-data-load-comparison.
import { chromium } from '@playwright/test';
import { createServer } from 'node:http';
import { brotliCompressSync, constants as zlib } from 'node:zlib';
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { cpus, totalmem, release } from 'node:os';
import { dirname, extname, join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { syncRelease } from './sync-data.mjs';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const options = { iterations: 5, dist: join(repo, 'web/dist'), out: join(repo, '.cache/web-load-performance.json'), profiles: 'all' };
for (let i = 2; i < process.argv.length; i += 2) {
  const [flag, value] = [process.argv[i], process.argv[i + 1]];
  if (!['--pack', '--dist', '--iterations', '--out', '--profiles'].includes(flag) || value === undefined) throw new Error(`Unknown or incomplete option: ${flag}`);
  options[flag.slice(2)] = flag === '--iterations' ? Number(value) : flag === '--profiles' ? value : resolve(value);
}
if (!options.pack) throw new Error('--pack <runtime_pack build output>/player is required');
if (!Number.isInteger(options.iterations) || options.iterations < 1) throw new Error('--iterations must be a positive integer');

// Network profiles shape one shared link; cpu is the DevTools throttling rate.
const PROFILES = {
  host: { cpu: 1, net: null },
  cpu4x: { cpu: 4, net: null },
  '4g': { cpu: 1, net: { mbps: 9, latencyMs: 150 } },
  'cpu4x+4g': { cpu: 4, net: { mbps: 9, latencyMs: 150 } },
};
const profiles = options.profiles === 'all' ? Object.keys(PROFILES) : options.profiles.split(',');
for (const name of profiles) if (!PROFILES[name]) throw new Error(`Unknown profile ${name}`);

// The compact variant: the same site, with /data/ replaced by the verified release's blocks.
const compactData = join(repo, '.cache/web-load/compact-data');
rmSync(compactData, { recursive: true, force: true });
mkdirSync(dirname(compactData), { recursive: true });
const release_ = syncRelease({ packRoot: options.pack, destRoot: compactData });
const variants = { 'per-file': join(options.dist, 'data'), compact: compactData };

const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.json': 'application/json',
  '.wasm': 'application/wasm', '.svg': 'image/svg+xml', '.png': 'image/png', '.webp': 'image/webp', '.woff2': 'font/woff2', '.txt': 'text/plain' };
const compressed = new Map();
function body(path) {
  let entry = compressed.get(path);
  if (!entry) {
    const raw = readFileSync(path);
    const type = TYPES[extname(path)] ?? 'application/octet-stream';
    const compressible = /^(text|application\/(json|javascript|wasm)|image\/svg)/.test(type);
    // Brotli quality 5: about what a CDN applies on the fly to uncached responses.
    const encoded = compressible ? brotliCompressSync(raw, { params: { [zlib.BROTLI_PARAM_QUALITY]: 5, [zlib.BROTLI_PARAM_SIZE_HINT]: raw.length } }) : raw;
    entry = { raw: raw.length, encoded, type, compressible };
    compressed.set(path, entry);
  }
  return entry;
}

const state = { variant: 'per-file', net: null, linkFree: 0, bytes: { data: 0, wasm: 0, other: 0 }, raw: { data: 0, wasm: 0, other: 0 }, requests: 0 };
const sleep = ms => new Promise(done => setTimeout(done, ms));
async function send(res, buffer) {
  if (!state.net) { res.end(buffer); return; }
  await sleep(state.net.latencyMs);
  const rate = state.net.mbps * 1e6 / 8 / 1000; // bytes per ms
  for (let offset = 0; offset < buffer.length; offset += 16384) {
    const chunk = buffer.subarray(offset, offset + 16384);
    const now = performance.now();
    const start = Math.max(now, state.linkFree);
    state.linkFree = start + chunk.length / rate;
    if (state.linkFree > now) await sleep(state.linkFree - now);
    if (!res.write(chunk)) await new Promise(done => res.once('drain', done));
  }
  res.end();
}

const server = createServer(async (req, res) => {
  const url = new URL(req.url, 'http://localhost');
  let rel = decodeURIComponent(url.pathname);
  const dataRoot = variants[state.variant];
  let path = rel.startsWith('/data/') ? join(dataRoot, rel.slice('/data/'.length)) : join(options.dist, rel);
  const base = rel.startsWith('/data/') ? dataRoot : options.dist;
  if (!path.startsWith(base + sep) && path !== base) { res.writeHead(403).end(); return; }
  if (!existsSync(path) || statSync(path).isDirectory()) {
    // SPA fallback, as the host serves index.html for client routes.
    if (rel.startsWith('/data/') || rel.startsWith('/assets/')) { res.writeHead(404).end(); return; }
    path = join(options.dist, 'index.html');
    rel = '/index.html';
  }
  const entry = body(path);
  const kind = rel.startsWith('/data/') ? 'data' : extname(path) === '.wasm' ? 'wasm' : 'other';
  state.bytes[kind] += entry.encoded.length;
  state.raw[kind] += entry.raw;
  state.requests += 1;
  res.writeHead(200, {
    'Content-Type': entry.type, 'Content-Length': entry.encoded.length,
    ...(entry.compressible ? { 'Content-Encoding': 'br' } : {}),
    'Cache-Control': rel.startsWith('/assets/') || rel.startsWith('/data/h/') ? 'public, max-age=31536000, immutable' : 'no-cache',
  });
  await send(res, entry.encoded);
});
await new Promise(done => server.listen(0, '127.0.0.1', done));
const origin = `http://127.0.0.1:${server.address().port}`;

/** Chromium renderer processes started by this script, with their memory in bytes. */
function rendererMemory() {
  const parents = new Map();
  for (const pid of readdirSync('/proc').filter(name => /^\d+$/.test(name))) {
    try { parents.set(pid, readFileSync(`/proc/${pid}/stat`, 'utf8').split(') ')[1].split(' ')[1]); } catch { /* exited */ }
  }
  const mine = pid => { for (let p = pid; p && p !== '1'; p = parents.get(p)) if (p === String(process.pid)) return true; return false; };
  let rss = 0, pss = 0;
  for (const pid of parents.keys()) {
    if (pid === String(process.pid) || !mine(pid)) continue;
    try {
      if (!readFileSync(`/proc/${pid}/cmdline`, 'utf8').includes('--type=renderer')) continue;
      const rollup = readFileSync(`/proc/${pid}/smaps_rollup`, 'utf8');
      rss += Number(/^Rss:\s+(\d+)/m.exec(rollup)[1]) * 1024;
      pss += Number(/^Pss:\s+(\d+)/m.exec(rollup)[1]) * 1024;
    } catch { /* exited */ }
  }
  return { rss, pss };
}

// Records when the boot screen leaves and when each calculation time appears, in page time.
const OBSERVER = () => {
  localStorage.setItem('pobr-lang', 'en-US');
  const marks = { boot: null, calcs: [] };
  window.__pobrMarks = marks;
  let seenBoot = false;
  const check = () => {
    const boot = document.querySelector('.shell-boot[role=status]');
    if (boot) seenBoot = true;
    if (seenBoot && !boot && marks.boot === null) marks.boot = performance.now();
    const ms = [...document.querySelectorAll('.rail-foot span')].map(span => span.textContent).find(text => /^\d+ ms$/.test(text ?? ''));
    const last = marks.calcs.at(-1);
    if (ms && (!last || last.text !== ms || last.gone)) marks.calcs.push({ at: performance.now(), text: ms, ms: Number.parseInt(ms, 10) });
    else if (!ms && last) last.gone = true;
  };
  new MutationObserver(check).observe(document, { childList: true, subtree: true, characterData: true });
};

async function run(variant, profileName) {
  const profile = PROFILES[profileName];
  state.variant = variant;
  state.net = profile.net;
  state.linkFree = 0;
  state.bytes = { data: 0, wasm: 0, other: 0 };
  state.raw = { data: 0, wasm: 0, other: 0 };
  state.requests = 0;
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext();
    const page = await context.newPage();
    await page.addInitScript(OBSERVER);
    const cdp = await context.newCDPSession(page);
    await cdp.send('Performance.enable');
    // Worker targets get the same CPU rate through a non-flat child session.
    let message = 0;
    cdp.on('Target.attachedToTarget', ({ sessionId, targetInfo }) => {
      if (targetInfo.type !== 'worker') return;
      for (const command of [{ method: 'Emulation.setCPUThrottlingRate', params: { rate: profile.cpu } }, { method: 'Runtime.runIfWaitingForDebugger' }]) {
        cdp.send('Target.sendMessageToTarget', { sessionId, message: JSON.stringify({ id: ++message, ...command }) }).catch(() => {});
      }
    });
    await cdp.send('Target.setAutoAttach', { autoAttach: true, waitForDebuggerOnStart: true, flatten: false });
    await cdp.send('Emulation.setCPUThrottlingRate', { rate: profile.cpu });
    let peak = { rss: 0, pss: 0 };
    let sampling = true;
    const sampler = (async () => { while (sampling) { const now = rendererMemory(); if (now.rss > peak.rss) peak = now; await sleep(100); } })();
    await page.goto(`${origin}/build`);
    await page.waitForFunction(() => window.__pobrMarks?.calcs.length > 0, null, { timeout: 600_000, polling: 50 });
    const first = await page.evaluate(() => structuredClone(window.__pobrMarks));
    const navigation = await page.evaluate(() => {
      const entry = performance.getEntriesByType('navigation')[0];
      return { domContentLoaded: entry.domContentLoadedEventEnd };
    });
    await sleep(1500);
    sampling = false;
    await sampler;
    const settled = rendererMemory();
    const metrics = Object.fromEntries((await cdp.send('Performance.getMetrics')).metrics.map(({ name, value }) => [name, value]));
    // A second, ordinary edit: the warm calculation time with the same engine and data.
    const before = first.calcs.length;
    await page.getByLabel('Level', { exact: true }).fill('90');
    await page.waitForFunction(count => window.__pobrMarks.calcs.length > count, before, { timeout: 600_000, polling: 50 });
    const edit = await page.evaluate(() => window.__pobrMarks.calcs.at(-1).ms);
    await context.close();
    return {
      variant, profile: profileName,
      bootMs: first.boot, firstResultMs: first.calcs[0].at, firstCalcReportedMs: first.calcs[0].ms, editCalcMs: edit,
      domContentLoadedMs: navigation.domContentLoaded,
      transfer: { ...state.bytes, total: state.bytes.data + state.bytes.wasm + state.bytes.other }, raw: { ...state.raw }, requests: state.requests,
      memory: { peakRendererRss: peak.rss, peakRendererPss: peak.pss, settledRendererRss: settled.rss, settledRendererPss: settled.pss,
        mainJsHeapUsed: metrics.JSHeapUsedSize },
    };
  } finally { await browser.close(); }
}

const median = values => { const sorted = [...values].sort((a, b) => a - b); const mid = sorted.length >> 1; return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2; };
const samples = [];
for (const profile of profiles) {
  // One discarded warm-up per variant fills the server's compression cache.
  for (const variant of Object.keys(variants)) if (!samples.some(sample => sample.variant === variant)) await run(variant, 'host');
  for (let i = 0; i < options.iterations; i++) {
    // Alternate the order so drift on the host does not favour one variant.
    for (const variant of i % 2 ? ['compact', 'per-file'] : ['per-file', 'compact']) {
      const sample = await run(variant, profile);
      samples.push(sample);
      console.error(`${profile} ${variant} #${i + 1}: first result ${Math.round(sample.firstResultMs)} ms, transfer ${(sample.transfer.total / 1e6).toFixed(2)} MB, peak PSS ${(sample.memory.peakRendererPss / 1e6).toFixed(0)} MB`);
    }
  }
}
server.close();

const summary = [];
for (const profile of profiles) for (const variant of Object.keys(variants)) {
  const group = samples.filter(sample => sample.profile === profile && sample.variant === variant);
  const pick = read => median(group.map(read));
  summary.push({ profile, variant, n: group.length,
    bootMs: pick(s => s.bootMs), firstResultMs: pick(s => s.firstResultMs), firstCalcAfterBootMs: pick(s => s.firstResultMs - s.bootMs),
    firstCalcReportedMs: pick(s => s.firstCalcReportedMs), editCalcMs: pick(s => s.editCalcMs),
    transferMB: pick(s => s.transfer.total) / 1e6, dataTransferMB: pick(s => s.transfer.data) / 1e6, dataRawMB: pick(s => s.raw.data) / 1e6,
    requests: pick(s => s.requests), peakPssMB: pick(s => s.memory.peakRendererPss) / 1e6, settledPssMB: pick(s => s.memory.settledRendererPss) / 1e6,
    mainHeapMB: pick(s => s.memory.mainJsHeapUsed) / 1e6 });
}
const report = {
  schema: 'pobr-web-load-performance/v1', measuredAt: new Date().toISOString(),
  host: { cpus: cpus().length, cpuModel: cpus()[0]?.model, memoryBytes: totalmem(), kernel: release(), chromium: chromium.executablePath() },
  release: { release_id: release_.release_id, runtime_identity: release_.runtime_identity, locale: release_.locale },
  profiles: Object.fromEntries(profiles.map(name => [name, PROFILES[name]])), iterations: options.iterations, summary, samples,
};
mkdirSync(dirname(options.out), { recursive: true });
writeFileSync(options.out, JSON.stringify(report, null, 2) + '\n');
console.table(summary.map(row => Object.fromEntries(Object.entries(row).map(([key, value]) => [key, typeof value === 'number' ? Math.round(value * 100) / 100 : value]))));
console.error(`wrote ${options.out}`);
