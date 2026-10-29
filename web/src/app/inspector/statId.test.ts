import { describe, expect, it } from 'vitest';
import { parseStatId, statFormat, statLabel } from './statId';

describe('parseStatId', () => {
  it('splits a minion stat and leaves a player stat alone', () => {
    expect(parseStatId('Minion:2:Life')).toEqual({ stat: 'Life', minion: 2 });
    expect(parseStatId('TotalEHP')).toEqual({ stat: 'TotalEHP' });
    expect(parseStatId('Minion:x:Life')).toEqual({ stat: 'Minion:x:Life' });
  });
});

describe('statFormat / statLabel', () => {
  it('uses the sidebar catalogue and falls back sensibly', () => {
    expect(statFormat('FireResist')).toBe('percent');
    expect(statFormat('SomethingUnlisted')).toBe('float2');
    expect(statLabel('en-US', 'FireResist')).toBe('Fire Res.');
    expect(statLabel('en-US', 'SomethingUnlisted')).toBeTruthy();
  });
});
