import { expect, test } from 'vitest';
import { ledgerReasonLabel, ownerLabel, ownerSkillId, stripReferenceMarkup, unsupportedLine, unsupportedLineLabel } from './diagnosticText';

test('reference markup keeps the label part', () => {
  expect(stripReferenceMarkup('[Curse|Curses] you inflict have infinite Duration')).toBe('Curses you inflict have infinite Duration');
  expect(stripReferenceMarkup('Gain [Rage] and [Fire|Fire Damage]')).toBe('Gain Rage and Fire Damage');
  expect(stripReferenceMarkup('plain [ text')).toBe('plain [ text');
});

test('unsupported line prefixes are localized and the raw text is untouched', () => {
  const raw = 'Tree:59: [Curse|Curses] you inflict have infinite Duration';
  expect(unsupportedLine(raw, 'en-US')).toEqual({ source: 'Passive #59', text: 'Curses you inflict have infinite Duration' });
  expect(unsupportedLineLabel(raw, 'zh-CN')).toBe('天赋 #59: Curses you inflict have infinite Duration');
  expect(unsupportedLineLabel(raw, 'zh-TW')).toContain('天賦 #59');
  expect(unsupportedLineLabel('Config: [Frenzy] x', 'en-US')).toBe('Setup: Frenzy x');
  expect(unsupportedLine('+5 to [Spirit]', 'en-US')).toEqual({ source: null, text: '+5 to Spirit' });
});

test('known reason codes get labels and unknown ones stay raw', () => {
  expect(ledgerReasonLabel('unknown_mod_name', 'en-US')).toBe('Unknown modifier name');
  expect(ledgerReasonLabel('unknown_mod_name', 'zh-CN')).toBe('未知的词缀名称');
  expect(ledgerReasonLabel('quality-applicability: Foo', 'zh-TW')).toBe('品質加成不適用');
  expect(ledgerReasonLabel('mod_type (part of the entry)', 'en-US')).toBe('Modifier type is not supported (whole entry skipped)');
  for (const code of ['unextractable', 'scalar', 'missing_mod_type', 'mod_type', 'tag', 'flags', 'keyword_flags', 'skill_data_key', 'skill_flag', 'kind']) {
    expect(ledgerReasonLabel(code, 'zh-CN')).not.toBeNull();
  }
  expect(ledgerReasonLabel('something_new', 'en-US')).toBeNull();
});

test('owners resolve their skill id', () => {
  expect(ownerSkillId('curse.TemporalChainsPlayer')).toBe('TemporalChainsPlayer');
  expect(ownerSkillId('gem.SparkPlayer.q20')).toBe('SparkPlayer');
  expect(ownerSkillId('item.weapon1')).toBeNull();
  expect(ownerLabel('curse.TemporalChainsPlayer', id => `N(${id})`)).toBe('N(TemporalChainsPlayer)');
  expect(ownerLabel('item.weapon1', id => id)).toBe('item.weapon1');
});
