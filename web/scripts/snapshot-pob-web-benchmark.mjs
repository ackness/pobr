// Retain measurement evidence without committing the large full breakdown responses.
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const options = {
  input: resolve(root, '.cache/pob-web-performance.json'),
  output: resolve(root, 'devs/ci/pob-web-performance-baseline.json'),
};
try {
  const seen = new Set();
  for (let index = 2; index < process.argv.length; index++) {
    const flag = process.argv[index];
    if (flag === '--help' && process.argv.length === 3) {
      console.log('Usage: node web/scripts/snapshot-pob-web-benchmark.mjs [--input REPORT] [--output BASELINE]');
      process.exit(0);
    }
    const key = { '--input': 'input', '--output': 'output' }[flag];
    const value = process.argv[++index];
    if (!key || seen.has(key) || !value?.trim() || value.startsWith('--')) throw new Error(`Invalid option: ${flag}`);
    seen.add(key);
    options[key] = resolve(value);
  }
  const report = JSON.parse(readFileSync(options.input, 'utf8'));
  const pobrOnly = report.method?.mode === 'pobr-only';
  // The default output is the committed paired baseline; a single-engine report must never replace it.
  if (pobrOnly && !seen.has('output')) throw new Error('A PoBR-only report requires an explicit --output');
  const baseline = {
    ...report,
    fullLocalReport: relative(root, options.input),
    engines: report.engines.map(({ provenance, ...engine }) => {
      const { initializationLogs, ...source } = provenance;
      return { ...engine, provenance: source };
    }),
    measurements: report.measurements.map(row => row.error ? row : {
      ...row,
      // Keep PoBR's selected main skill: a fast calculation of the wrong skill is not a result.
      engines: row.engines.map(({ output, ...measurement }) =>
        output?.main_skill ? { ...measurement, mainSkill: output.main_skill } : measurement),
    }),
  };
  mkdirSync(dirname(options.output), { recursive: true });
  writeFileSync(options.output, JSON.stringify(baseline, null, 2) + '\n');
  console.log(`Snapshot: ${options.output}`);
  for (const corpus of ['ninja-latest-season', 'historical']) {
    console.log(`\n${corpus}`);
    console.log(pobrOnly
      ? '| Build | PoBR median ms | PoBR p95 ms | Selected skill |'
      : '| Build | PoBR median ms | Lua median ms | Speedup | PoBR p95 ms | Lua p95 ms | p95 ratio |');
    console.log(pobrOnly ? '| --- | ---: | ---: | --- |' : '| --- | ---: | ---: | ---: | ---: | ---: | ---: |');
    for (const row of baseline.measurements.filter(row => row.corpus === corpus)) {
      const name = `${row.character.class} / ${row.character.ascendancy} / ${row.character.main_skill}`;
      if (row.error) console.log(`| ${name} (failed; see report) | — | — | — | — | — | — |`);
      else if (pobrOnly) {
        const [rust] = row.engines;
        console.log(`| ${name} | ${rust.medianMs.toFixed(2)} | ${rust.p95Ms.toFixed(2)} | ${rust.mainSkill?.skill_id ?? '—'} |`);
      } else {
        const [rust, lua] = row.engines;
        console.log(`| ${name} | ${rust.medianMs.toFixed(2)} | ${lua.medianMs.toFixed(2)} | ${row.speedup.ratio.toFixed(2)}× | ${rust.p95Ms.toFixed(2)} | ${lua.p95Ms.toFixed(2)} | ${(lua.p95Ms / rust.p95Ms).toFixed(2)}× |`);
      }
    }
  }
} catch (error) {
  console.error(error.stack ?? String(error));
  process.exitCode = 1;
}
