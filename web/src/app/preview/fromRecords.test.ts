import { describe, expect, it } from 'vitest';
import { changesFromRecords } from './fromRecords';

describe('changesFromRecords', () => {
  it('reports only what moved, the biggest relative move first, judged by polarity', () => {
    const changes = changesFromRecords({ TotalDPS: 1000, Life: 500, ManaCost: 20 }, { TotalDPS: 1100, Life: 500, ManaCost: 25 });
    expect(changes.map(change => [change.id, change.tone])).toEqual([['ManaCost', 'worse'], ['TotalDPS', 'better']]);
  });
  it('puts a headline stat that moved ahead of the others', () => {
    const changes = changesFromRecords({ CombinedDPS: 1000, ManaCost: 20 }, { CombinedDPS: 1100, ManaCost: 30 });
    expect(changes.map(change => change.id)).toEqual(['CombinedDPS', 'ManaCost']);
    expect(changes[0]).toMatchObject({ delta: 100, tone: 'better' });
  });
  it('says nothing when nothing changed', () => {
    expect(changesFromRecords({ TotalDPS: 1 }, { TotalDPS: 1 })).toEqual([]);
  });
});
