import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { checkLocales } from './check-locales.mjs';

function locales(files, registry = Object.keys(files)) {
  const dir = mkdtempSync(join(tmpdir(), 'pobr-locales-'));
  for (const [name, content] of Object.entries(files)) {
    writeFileSync(join(dir, name), typeof content === 'string' ? content : JSON.stringify(content));
  }
  const index = registry.map((name, i) => `import l${i} from './${name}';`).join('\n');
  try {
    return checkLocales(dir, index);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const en = { ui: { hello: 'Hello {name}', bye: 'Bye' }, slot: { ring1: 'Ring 1' } };

test('the shipped locale files pass', () => {
  const { errors, summary } = checkLocales(new URL('../src/locales', import.meta.url).pathname);
  assert.deepEqual(errors, []);
  assert.ok(summary.find(row => row.lang === 'en-US').keys > 2000);
});

test('missing keys are warnings that count the English fallbacks', () => {
  const result = locales({ 'en-US.json': en, 'ja-JP.json': { ui: { hello: 'こんにちは {name}' } } });
  assert.deepEqual(result.errors, []);
  assert.equal(result.warnings.length, 1);
  assert.match(result.warnings[0], /ja-JP\.json: 2 keys fall back to en-US: ui\/bye, slot\/ring1/);
  assert.deepEqual(result.summary.map(row => [row.lang, row.missing]), [['en-US', 0], ['ja-JP', 2]]);
});

test('extra keys, non-strings, placeholders and invalid JSON are errors', () => {
  const { errors } = locales({
    'en-US.json': en,
    'de-DE.json': { ui: { hello: 'Hallo {user}', bye: 3, extra: 'x' }, other: { a: 'b' } },
    'fr-FR.json': '{ "ui": ',
  });
  assert.ok(errors.some(error => /de-DE\.json: ui\/hello placeholders \[\{user\}\] differ from en-US \[\{name\}\]/.test(error)));
  assert.ok(errors.includes('de-DE.json: ui/bye is not a string'));
  assert.ok(errors.includes('de-DE.json: ui/extra is not in en-US.json'));
  assert.ok(errors.includes('de-DE.json: other/a is not in en-US.json'));
  assert.ok(errors.some(error => error.startsWith('fr-FR.json: invalid JSON')));
});

test('every file must be registered and named by its language tag', () => {
  const unregistered = locales({ 'en-US.json': en, 'ko-KR.json': {} }, ['en-US.json', 'pt-BR.json']);
  assert.deepEqual(unregistered.errors, ['ko-KR.json: not registered in index.ts', 'pt-BR.json: registered in index.ts but missing']);
  const badName = locales({ 'en-US.json': en, 'chinese.json': {} });
  assert.deepEqual(badName.errors, ['chinese.json: file name is not a language tag such as zh-CN']);
  assert.deepEqual(locales({ 'zh-CN.json': {} }).errors, ['en-US.json is missing']);
});
