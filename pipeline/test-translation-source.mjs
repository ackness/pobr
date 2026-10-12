import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { translationSource, mergeTranslations } from './translation-source.mjs';

const snapshot = '20261012T000000000Z';
const row = (src, en, kind = 'trade') => ({ src, en, kind, provenance: { source: 'fixture' } });
const bundle = templates => ({ schema_version: 1, snapshot, templates, words: [], excluded: [] });
const life = row('{0:d} 生命上限', '{0:d} to maximum Life');

async function temporary(t) {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'pobr-translations-'));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  return directory;
}

test('pointer and export use one pinned commit and the complete result is usable offline', async t => {
  const cacheRoot = await temporary(t);
  const ref = 'a'.repeat(40);
  const seen = [];
  const result = await translationSource({ cacheRoot, refresh: true, fetcher: async url => {
    seen.push(url);
    if (url.includes('/commits/HEAD')) return { ok: true, json: async () => ({ sha: ref }) };
    assert.ok(url.includes(`/ackness/poe2-zh-cn-stats/${ref}/`));
    return { ok: true, text: async () => url.endsWith('/data/CURRENT') ? snapshot + '\n' : JSON.stringify(bundle([life])) };
  } });
  assert.equal(result.receipt.commit, ref);
  assert.equal(result.receipt.snapshot, snapshot);
  assert.deepEqual(result.bundle.templates, [life]);
  assert.equal(seen.filter(url => url.includes('/commits/HEAD')).length, 1);
  assert.deepEqual(await translationSource({ cacheRoot, ref, fetcher: () => { throw new Error('Must use immutable cache'); } }), result);
});

test('a failed export does not masquerade as a complete translation snapshot', async t => {
  const cacheRoot = await temporary(t);
  const ref = 'b'.repeat(40);
  await assert.rejects(translationSource({ cacheRoot, ref, fetcher: async url => url.endsWith('/data/CURRENT')
    ? { ok: true, text: async () => snapshot } : { ok: false, status: 503 } }), /503/);
  assert.deepEqual(await fs.readdir(path.join(cacheRoot, 'exports')), []);
  const recovered = await translationSource({ cacheRoot, ref, fetcher: async url => {
    assert.ok(url.endsWith('/generated/templates.json'));
    return { ok: true, text: async () => JSON.stringify(bundle([life])) };
  } });
  assert.equal(recovered.bundle.templates[0].src, life.src);
});

test('invalid pointers, mismatched snapshots and invalid placeholder mappings fail closed', async t => {
  const root = await temporary(t);
  for (const [index, pointer, document, pattern] of [
    [0, '../other', bundle([life]), /snapshot pointer/],
    [1, snapshot, { ...bundle([life]), snapshot: 'different' }, /inconsistent/],
    [2, snapshot, bundle([row('{1:d} 生命上限', life.en)]), /template/],
    [3, snapshot, bundle([row('{bad} 生命上限', life.en)]), /placeholder/],
  ]) {
    const cacheRoot = path.join(root, String(index));
    await assert.rejects(translationSource({ cacheRoot, ref: 'c'.repeat(40), fetcher: async url => ({
      ok: true, text: async () => url.endsWith('/data/CURRENT') ? pointer : JSON.stringify(document),
    }) }), pattern);
  }
});

test('reviewed nested templates retain exact slot identity and plural aliases', () => {
  const templates = [row('获得 {0} 个天赋技能点', 'Grants {0} Passive Skill Point', 'curated'),
    row('获得 {0} 个天赋技能点', 'Grants {0} Passive Skill Points', 'curated'),
    row('获得技能：{0} 级 {1}', 'Grants Skill: Level {0} {1}', 'curated')];
  const result = mergeTranslations([], bundle(templates));
  assert.deepEqual(new Set(result.pairs.map(p => p.en)), new Set(templates.map(p => p.en)));
  assert.equal(result.report.added_curated, 3);
});

test('new automatic text is admitted without taking over a different existing Chinese template', () => {
  const legacy = [{ src: '{0:+d} 生命上限', en: '{0:+d} to maximum Life' },
    { src: '{0:d}% 法术伤害', en: '{0:d}% increased Spell Damage' }];
  const mana = row('{0:d} 魔力上限', '{0:d} to maximum Mana');
  const result = mergeTranslations(legacy, bundle([life, mana,
    row('{0:d}% 法术伤害', '{0:d}% increased Attack Damage')]));
  assert.deepEqual(result.pairs.filter(p => p.en.includes('Life')), [legacy[0]]);
  assert.ok(result.pairs.some(p => p.en === mana.en && p.src === mana.src));
  assert.equal(result.report.added_trade, 1);
  assert.deepEqual(result.report.replaced_legacy, []);
  assert.deepEqual(new Set(result.report.excluded.map(p => p.reason)), new Set(['existing-english-template', 'existing-chinese-template']));
});

test('official wording replaces differing legacy Chinese and keeps the legacy English and slot format', () => {
  const legacy = [{ src: '{0:+d} 最大生命', en: '{0:+d} to maximum Life' },
    { src: '绑定词缀生效', en: 'Bonded modifiers apply' },
    { src: '第一行\n第二行 {0}', en: 'First line\nsecond {0}' }];
  const result = mergeTranslations(legacy, bundle([life,
    row('羁绊词缀生效', 'Bonded modifiers apply'),
    row('首行 次行 {0:d}', 'First line second {0:d}')]));
  assert.deepEqual(result.pairs.filter(p => p.en.includes('Life')), [{ src: '{0:+d} 生命上限', en: '{0:+d} to maximum Life' }]);
  assert.deepEqual(result.pairs.filter(p => p.en.includes('Bonded')), [{ src: '羁绊词缀生效', en: 'Bonded modifiers apply' }]);
  assert.ok(result.pairs.some(p => p.src === legacy[2].src), 'multi-line legacy templates are kept');
  assert.deepEqual(result.report.replaced_legacy.map(p => [p.legacy, p.official]),
    [['绑定词缀生效', '羁绊词缀生效'], ['{0:+d} 最大生命', '{0:+d} 生命上限']]);
  assert.equal(result.report.added_trade, 0);
});

test('official wording does not replace curated, literal-number or ambiguous templates', () => {
  const curated = row('每 {0} 点力量', 'Per {0} Strength', 'curated');
  const legacy = [{ src: '每 10 点敏捷获得 {0} 闪避', en: '{0} Evasion per 10 Dexterity' },
    { src: '旧甲', en: '{0} to Armour' }];
  const result = mergeTranslations(legacy, bundle([curated,
    row('每 {0:d} 点力量值', 'Per {0:d} Strength'),
    row('每 20 点敏捷增加 {0:d} 闪避值', '{0:d} Evasion per 20 Dexterity'),
    row('{0:d} 护甲', '{0:d} to Armour'), row('护甲 {0:d}', '{0:d} to armour')]));
  assert.deepEqual(result.report.replaced_legacy, []);
  assert.deepEqual(new Set(result.pairs.map(p => p.src)), new Set([curated.src, legacy[0].src, legacy[1].src]));
});

test('literal-number and case-only skeleton collisions are excluded independent of source order', () => {
  const rows = [row('每 10 点敏捷增加 {0:d} 闪避值', '{0:d} Evasion per 10 Dexterity'),
    row('每 20 点敏捷增加 {0:d} 闪避值', '{0:d} Evasion per 20 Dexterity'),
    row('额外 {0:d} 精魂', '{0:d} additional Spirit'),
    row('{0:d} 额外精魂', '{0:d} additional spirit')];
  const first = mergeTranslations([], bundle(rows));
  const reversed = mergeTranslations([], bundle([...rows].reverse()));
  assert.deepEqual(first, reversed);
  assert.deepEqual(first.pairs, []);
  assert.equal(first.report.excluded.filter(p => p.reason === 'automatic-skeleton-ambiguity').length, 4);
});
