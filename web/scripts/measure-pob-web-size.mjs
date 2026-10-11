import { createHash } from 'node:crypto';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';
import { verifyReference, DEFAULT_REFERENCE_DIRECTORY } from './benchmark-pob-web.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');

export function measureReferenceArtifacts({ wasm, js, data }) {
  if (![wasm, js, data].every(bytes => Buffer.isBuffer(bytes) && bytes.length > 0)) {
    throw new Error('Expected nonempty WASM, JavaScript and uncompressed Lua bundle bytes');
  }
  const files = [['lua.wasm', wasm], ['lua.mjs', js]].map(([name, bytes]) => ({
    name, bytes: bytes.length, gzipBytes: gzipSync(bytes, { level: 9 }).length, sha256: sha256(bytes),
  }));
  const compressedData = gzipSync(data, { level: 9 });
  files.push({ name: 'pob-lua.json.gz', bytes: compressedData.length, gzipBytes: compressedData.length,
    uncompressedBytes: data.length, sha256: sha256(compressedData), uncompressedSha256: sha256(data) });
  return files;
}

export function measureReferenceSize({ directory = DEFAULT_REFERENCE_DIRECTORY } = {}) {
  const { provenance, files } = verifyReference({ directory });
  return {
    project: 'krauthaufen/pob-web', source: provenance.source.repository,
    measuredAt: new Date().toISOString(), acquiredAt: provenance.acquiredAt,
    artifactSource: provenance.deployed.baseUrl, deployedRevision: null,
    wrapperSourceCommit: provenance.source.commit,
    compression: { algorithm: 'gzip', level: 9, node: process.version, zlib: process.versions.zlib },
    provenance: 'Measured from hash-verified deployed Lua WASM, JS and Lua/data bundle. Deployment revision unknown; wrapper source revision does not identify the binary/data. Data is gzip level 9; runtime transfer estimates also gzip WASM/JS. Different datasets and supported mechanics; not a feature-parity or runtime-memory claim.',
    files: measureReferenceArtifacts({ wasm: files.get('lua.wasm'), js: files.get('lua.mjs'), data: files.get('pob-lua.json') }),
  };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const options = { directory: DEFAULT_REFERENCE_DIRECTORY, output: resolve(root, '.cache/pob-web-size-reference.json') };
    const seen = new Set();
    for (let index = 2; index < process.argv.length; index++) {
      const flag = process.argv[index];
      if (flag === '--help' && process.argv.length === 3) {
        console.log('Usage: node web/scripts/measure-pob-web-size.mjs [--reference-dir PATH] [--output FILE]');
        process.exit(0);
      }
      const key = { '--reference-dir': 'directory', '--output': 'output' }[flag];
      const value = process.argv[++index];
      if (!key || seen.has(key) || !value?.trim() || value.startsWith('--')) throw new Error(`Invalid option: ${flag}`);
      seen.add(key);
      options[key] = resolve(value);
    }
    const report = measureReferenceSize(options);
    mkdirSync(dirname(options.output), { recursive: true });
    writeFileSync(options.output, JSON.stringify(report, null, 2) + '\n');
    console.log(JSON.stringify(report, null, 2));
    console.log(`Report: ${options.output}`);
  } catch (error) {
    console.error(error.stack ?? String(error));
    process.exitCode = 1;
  }
}
