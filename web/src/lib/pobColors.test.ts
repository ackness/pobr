import { describe, expect, test } from 'vitest';
import { hasPobColorCodes, parsePobColorText } from './pobColors';

describe('parsePobColorText', () => {
  test('splits digit and hex codes into colored segments', () => {
    expect(parsePobColorText('plain ^1red ^xA0B1C2custom')).toEqual([
      { color: null, text: 'plain ' },
      { color: '#ff0000', text: 'red ' },
      { color: '#a0b1c2', text: 'custom' },
    ]);
  });

  test('returns single uncolored segment for plain text', () => {
    expect(parsePobColorText('no codes here')).toEqual([
      { color: null, text: 'no codes here' },
    ]);
    expect(hasPobColorCodes('no codes here')).toBe(false);
  });

  test('adjacent codes: later code wins, no empty segment emitted', () => {
    expect(parsePobColorText('^7^2green')).toEqual([{ color: '#00ff00', text: 'green' }]);
    expect(hasPobColorCodes('^7^2green')).toBe(true);
  });
});
