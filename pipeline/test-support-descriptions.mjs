import { test } from 'node:test';
import assert from 'node:assert/strict';
import { supportDescriptions } from './gen-support-descriptions.mjs';

const dictionary = pairs => ({ table: 'GemEffects', entries: [{ columns: { SupportText: pairs } }] });
const gem = (skill_id, description, is_support = true) => ({ skill_id, description, is_support });

test('joins complete prose after removing game links, retaining stable IDs and tier copies', () => {
  const result = supportDescriptions(dictionary([{ en: 'Supports [Attack|Attacks]. ', zh: '辅助[Attack|攻击]。' }]), [
    gem('supportTwo', 'Supports Attacks.'), gem('supportOne', 'Supports Attacks.'),
    gem('active', 'Supports Attacks.', false), gem('different', 'Supports Attacks. More damage.'), gem('missing'),
  ]);
  assert.deepEqual(result, { entries: {
    supportOne: { en: 'Supports Attacks.', zh_cn: '辅助攻击。' },
    supportTwo: { en: 'Supports Attacks.', zh_cn: '辅助攻击。' },
  }, untranslated: ['different', 'missing'] });
});

test('rejects ambiguous translations and duplicate identities, ignores untranslated source cells', () => {
  assert.throws(() => supportDescriptions(dictionary([{ en: 'Same', zh: '甲' }, { en: 'Same', zh: '乙' }]), []), /Conflicting/);
  assert.throws(() => supportDescriptions(dictionary([]), [gem('same'), gem('same')]), /Duplicate/);
  assert.deepEqual(supportDescriptions(dictionary([{ en: 'English', zh: 'English' }]), [gem('unknown', 'English')]).untranslated, ['unknown']);
});
