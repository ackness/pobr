import { describe, expect, it } from 'vitest';
import type { LineUsageLine, LineUsageStatus } from '../../../api/types';
import {
  ATTENTION, NO_FILTER, STATUS_ORDER, cleanLineText, filterLedger, lineText, sortLedger, sourceGroup, statusHint, statusLabel,
  statusRank, statusTone, summarise, toggleStatus,
} from './ledger';

function line(status: LineUsageStatus, extra: Partial<LineUsageLine> = {}): LineUsageLine {
  return { source_kind: 'ItemAffix', source_id: 'item.helmet.explicit', slot: 'Helmet', holder: 'Hubris Circlet', text: `text of ${status}`,
    status, reason: null, used_in: [], modifiers: [], ...extra };
}

describe('statuses', () => {
  it('orders lost lines before inactive ones before counted ones', () => {
    expect(STATUS_ORDER.indexOf('unparsed')).toBeLessThan(STATUS_ORDER.indexOf('inactive'));
    expect(STATUS_ORDER.indexOf('inactive')).toBeLessThan(STATUS_ORDER.indexOf('applied'));
    expect(statusRank('unparsed')).toBe(0);
    expect(new Set(STATUS_ORDER).size).toBe(9);
  });

  it('gives every status a tone, a label and a hint in all three languages', () => {
    for (const status of STATUS_ORDER) {
      expect(['bad', 'warn', 'ok', 'neutral']).toContain(statusTone(status));
      for (const lang of ['en-US', 'zh-CN', 'zh-TW'] as const) {
        expect(statusLabel(lang, status).length).toBeGreaterThan(0);
        expect(statusHint(lang, status).length).toBeGreaterThan(0);
      }
    }
    expect(statusTone('unparsed')).toBe('bad');
    expect(statusTone('inactive')).toBe('warn');
    expect(statusTone('applied')).toBe('ok');
    expect(statusTone('no_effect')).toBe('neutral');
  });

  it('treats only actionable states as needing attention', () => {
    expect([...ATTENTION].sort()).toEqual(['inactive', 'no_modifier', 'unmodelled', 'unparsed', 'unread']);
  });
});

describe('sourceGroup', () => {
  it('groups by source id, then by source kind', () => {
    expect(sourceGroup({ source_id: 'item.helmet.explicit', source_kind: 'ItemAffix' })).toBe('item');
    expect(sourceGroup({ source_id: 'item.arm1.implicit', source_kind: null })).toBe('item');
    expect(sourceGroup({ source_id: 'node.12345', source_kind: 'PassiveNode' })).toBe('tree');
    expect(sourceGroup({ source_id: 'jewel.2', source_kind: 'Jewel' })).toBe('jewel');
    expect(sourceGroup({ source_id: 'flask.flask1', source_kind: 'Flask' })).toBe('flask');
    expect(sourceGroup({ source_id: 'quest', source_kind: 'Config' })).toBe('config');
    expect(sourceGroup({ source_id: 'config.custom', source_kind: null })).toBe('config');
    expect(sourceGroup({ source_id: null, source_kind: 'PassiveNode' })).toBe('tree');
    expect(sourceGroup({ source_id: null, source_kind: null })).toBe('other');
  });
});

describe('lineText', () => {
  it('strips game display markup', () => {
    expect(cleanLineText('Gain [Rage|Rage] on [Hit]')).toBe('Gain Rage on Hit');
  });

  it('falls back to the first modifier name for lines without original text', () => {
    expect(lineText(line('applied', { text: null, modifiers: [{ name: 'GemLevel', mod_type: 'BASE', value: 1, applied: 1, status: 'applied', reason: null, used_in: [] }] }))).toBe('GemLevel');
    expect(lineText(line('applied', { text: null }))).toBe('—');
  });
});

describe('ordering and filtering', () => {
  const lines = [
    line('applied', { text: 'a1' }), line('unparsed', { text: 'u1' }), line('inactive', { text: 'i1', reason: 'needs Attack' }),
    line('applied', { text: 'a2' }), line('unmodelled', { text: 'm1' }), line('inactive', { text: 'i2' }),
  ];

  it('puts problems first and keeps the engine order inside a status', () => {
    expect(sortLedger(lines).map(l => l.text)).toEqual(['u1', 'm1', 'i1', 'i2', 'a1', 'a2']);
  });

  it('does not mutate its input', () => {
    const copy = [...lines];
    sortLedger(lines);
    expect(lines).toEqual(copy);
  });

  it('filters by status set', () => {
    const shown = filterLedger(lines, { ...NO_FILTER, statuses: new Set<LineUsageStatus>(['inactive', 'unparsed']) });
    expect(shown.map(l => l.text)).toEqual(['u1', 'i1', 'i2']);
  });

  it('searches text, reason, holder, modifier names and used-in stats, ignoring case', () => {
    const rows = [
      line('applied', { text: 'Fire Damage', holder: 'Ember Ring' }),
      line('inactive', { text: 'Other', reason: 'Needs Attack', holder: 'Ring' }),
      line('applied', { text: 'Life', used_in: ['TotalEHP'], holder: 'Chest' }),
      line('applied', { text: 'Mana', holder: 'Chest', modifiers: [{ name: 'MaximumMana', mod_type: 'BASE', value: 5, applied: 5, status: 'applied', reason: null, used_in: [] }] }),
    ];
    const find = (query: string) => filterLedger(rows, { ...NO_FILTER, query }).map(l => l.text);
    expect(find('fire')).toEqual(['Fire Damage']);
    expect(find(' needs attack ')).toEqual(['Other']);
    expect(find('totalehp')).toEqual(['Life']);
    expect(find('maximummana')).toEqual(['Mana']);
    expect(find('ember')).toEqual(['Fire Damage']);
    expect(find('nothing like this')).toEqual([]);
  });

  it('filters by source group and combines with the other filters', () => {
    const rows = [
      line('inactive', { text: 'item line' }),
      line('inactive', { text: 'node line', source_id: 'node.9', source_kind: 'PassiveNode' }),
      line('applied', { text: 'node ok', source_id: 'node.10', source_kind: 'PassiveNode' }),
    ];
    expect(filterLedger(rows, { statuses: null, query: '', group: 'tree' }).map(l => l.text)).toEqual(['node line', 'node ok']);
    expect(filterLedger(rows, { statuses: new Set<LineUsageStatus>(['inactive']), query: 'node', group: 'tree' }).map(l => l.text)).toEqual(['node line']);
  });
});

describe('summarise', () => {
  it('counts every status, the attention total and the applied share', () => {
    const summary = summarise({ lines: [line('applied'), line('applied'), line('applied'), line('unparsed'), line('inactive', { source_id: 'node.1', source_kind: 'PassiveNode' }), line('no_effect')] });
    expect(summary.total).toBe(6);
    expect(summary.counts).toMatchObject({ applied: 3, unparsed: 1, inactive: 1, no_effect: 1, partial: 0 });
    expect(summary.attention).toBe(2);
    expect(summary.appliedPercent).toBe(50);
    expect(summary.groups).toMatchObject({ item: 5, tree: 1 });
  });

  it('copes with an empty ledger', () => {
    const summary = summarise({ lines: [] });
    expect(summary).toMatchObject({ total: 0, attention: 0, appliedPercent: 0 });
    expect(Object.values(summary.counts).every(count => count === 0)).toBe(true);
  });
});

describe('toggleStatus', () => {
  it('adds and removes statuses, and returns to "all" when emptied', () => {
    const one = toggleStatus(null, 'inactive');
    expect([...one!]).toEqual(['inactive']);
    const two = toggleStatus(one, 'unparsed');
    expect([...two!].sort()).toEqual(['inactive', 'unparsed']);
    expect(toggleStatus(toggleStatus(two, 'unparsed'), 'inactive')).toBeNull();
  });
});
