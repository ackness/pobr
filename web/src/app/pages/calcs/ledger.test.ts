import { describe, expect, it } from 'vitest';
import type { LineUsageLine, LineUsageStatus } from '../../../api/types';
import { configLabel } from '../../../lib/configLabels';
import {
  ATTENTION, NO_FILTER, STATUS_ORDER, cleanLineText, filterLedger, ledgerDisplayText, ledgerReason, lineText, sortLedger, sourceGroup, statusHint, statusLabel,
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

  it('names synthetic config rows without replacing actual custom modifier text', () => {
    const config = line('inactive', { source_kind: 'Config', source_id: 'config.GamblesprintMovementSpeed', text: 'Config' });
    expect(ledgerDisplayText(config, 'zh-CN')).toBe('配置：赌命疾奔移动速度');
    expect(ledgerDisplayText(config, 'zh-CN')).not.toBe('Config');
    expect(ledgerDisplayText(config, 'en-US', { GamblesprintMovementSpeed: 'Gamblesprint Movement Speed' }))
      .toBe('Configuration: Gamblesprint Movement Speed');
    expect(ledgerDisplayText({ ...config, text: '10% increased Damage' }, 'zh-CN')).toBe('10% increased Damage');
    expect(ledgerDisplayText({ ...config, source_id: 'config.future' }, 'zh-CN')).toBe('配置：缺少配置名称');
    const generated = { ...config, source_id: 'config.raiseSpectreEnableCurses', text: 'SpectreCursesEnabled',
      modifiers: [{ name: 'SpectreCursesEnabled', mod_type: 'FLAG', value: true, applied: null,
        status: 'unread' as const, reason: null, used_in: [] }] };
    expect(ledgerDisplayText(generated, 'zh-CN', {
      raiseSpectreEnableCurses: configLabel('zh-CN', { var: 'raiseSpectreEnableCurses', label: 'Enable curses:' }),
    })).toBe('配置：启用幽魂的诅咒');
    expect(ledgerDisplayText(generated, 'zh-CN')).toBe('SpectreCursesEnabled');
  });

  it('localizes the full engine-owned ancestral description, not a mixture of terms', () => {
    const boost = line('inactive', {
      source_id: 'base.AncestralBoost',
      text: 'Ancestral Boost: 2 additional Strike targets; 30% more Slam damage and 25% increased Slam area',
    });
    expect(ledgerDisplayText(boost, 'zh-CN')).toBe('先祖增幅：打击额外以 2 个敌人为目标；猛击伤害总增 30%，范围扩大 25%');
    expect(ledgerDisplayText(boost, 'en-US')).toBe(boost.text);
    expect(ledgerDisplayText({ ...boost, text: 'Different future description' }, 'zh-CN')).toBe('Different future description');
  });

  it('does not describe unsupported text as having no effect in the game', () => {
    expect(statusHint('zh-CN', 'unparsed')).toContain('本次计算未计入');
    expect(statusLabel('zh-CN', 'inactive')).toBe('当前不生效');
  });

  it('shows the inactive modifier reason at row level and explains missing slot bases', () => {
    const inactive = line('inactive', { reason: null, modifiers: [
      { name: 'Evasion', mod_type: 'MORE', value: -50, applied: null, status: 'inactive',
        reason: 'the equipped body armour has no base Evasion', used_in: [] },
    ] });
    expect(ledgerReason(inactive, 'zh-CN')).toBe('当前胸甲没有可缩放的闪避基值。');
    expect(ledgerReason(inactive, 'en-US')).toBe('the equipped body armour has no base Evasion');
    expect(ledgerReason(line('unread', { reason: 'Unknown future reason' }), 'zh-CN')).toBe('Unknown future reason');
  });

  it('localizes known engine reasons without changing status or discarding unknown diagnostics', () => {
    const inactive = line('inactive', { reason: 'Condition Combat not met' });
    expect(ledgerReason(inactive, 'zh-CN')).toBe('当前未满足条件：战斗状态。');
    expect(ledgerReason(inactive, 'en-US')).toBe(inactive.reason);
    expect(inactive.status).toBe('inactive');
    const unmodelled = line('unmodelled', { reason: 'rule life_flask_self_only recognises it and stores nothing' });
    expect(ledgerReason(unmodelled, 'zh-CN')).toBe('已识别，但该机制尚未实现。');
    expect(unmodelled.reason).toContain('life_flask_self_only');
    expect(ledgerReason({ ...unmodelled, status: 'no_effect' }, 'zh-CN')).toBe(unmodelled.reason);
    expect(ledgerReason(line('no_effect', { reason: 'a config option nothing in this build asks for' }), 'zh-TW'))
      .toBe('目前構築沒有使用此配置項。');
    expect(ledgerReason(line('inactive', { reason: 'Condition FutureCondition not met' }), 'zh-CN'))
      .toBe('Condition FutureCondition not met');
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
