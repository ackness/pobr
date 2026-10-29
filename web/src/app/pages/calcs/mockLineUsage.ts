import type { LineUsageLine, LineUsageModifier, LineUsageResponse, LineUsageStatus } from '../../../api/types';

/** A small, realistic ledger for developing the UI against the mock backend. Not engine output. */

function mod(name: string, mod_type: string, value: number | boolean | string, status: LineUsageModifier['status'], extra: Partial<LineUsageModifier> = {}): LineUsageModifier {
  return { name, mod_type, value, applied: status === 'applied' && typeof value === 'number' ? value : null, status, reason: null, used_in: [], ...extra };
}

function row(status: LineUsageStatus, source_id: string, source_kind: string, holder: string | null, text: string | null, extra: Partial<LineUsageLine> = {}): LineUsageLine {
  const slot = /^item\.([^.]+)\./.exec(source_id)?.[1] ?? null;
  return { source_kind, source_id, slot, holder, text, status, reason: null, used_in: [], modifiers: [], ...extra };
}

const LINES: LineUsageLine[] = [
  row('applied', 'item.helmet.explicit', 'ItemAffix', 'Hubris Circlet', '+97 to maximum Energy Shield', {
    used_in: ['EnergyShield', 'TotalEHP'], modifiers: [mod('EnergyShield', 'BASE', 97, 'applied', { used_in: ['EnergyShield', 'TotalEHP'] })] }),
  row('applied', 'item.helmet.explicit', 'ItemAffix', 'Hubris Circlet', '28% increased Spell Damage', {
    used_in: ['TotalDPS', 'HitAverage'], modifiers: [mod('Damage', 'INC', 28, 'applied', { used_in: ['TotalDPS', 'HitAverage'] })] }),
  row('applied', 'item.bodyarmour.explicit', 'ItemAffix', 'Sacral Robe', '+30% to Cold Resistance', {
    used_in: ['ColdResist'], modifiers: [mod('ColdResist', 'BASE', 30, 'applied', { used_in: ['ColdResist'] })] }),
  row('applied', 'item.gloves.implicit', 'ItemImplicit', 'Gripped Gloves', '10% increased Cast Speed', {
    used_in: ['Speed', 'TotalDPS'], modifiers: [mod('Speed', 'INC', 10, 'applied', { used_in: ['Speed', 'TotalDPS'] })] }),
  row('applied', 'node.19330', 'PassiveNode', 'Spell Damage', '12% increased Spell Damage', {
    used_in: ['TotalDPS'], modifiers: [mod('Damage', 'INC', 12, 'applied', { used_in: ['TotalDPS'] })] }),
  row('applied', 'node.6967', 'PassiveNode', 'Mana Regeneration', '15% increased Mana Regeneration Rate', {
    used_in: ['ManaRegenRecovery'], modifiers: [mod('ManaRegen', 'INC', 15, 'applied', { used_in: ['ManaRegenRecovery'] })] }),
  row('partial', 'item.amulet.explicit', 'ItemAffix', 'Stellar Amulet', '+12 to Intelligence and 8% increased Mana Recovery rate', {
    reason: 'one of 2 modifiers is read', used_in: ['Int'], modifiers: [
      mod('Int', 'BASE', 12, 'applied', { used_in: ['Int'] }),
      mod('ManaRecoveryRate', 'INC', 8, 'unread', { reason: 'no query asks for this name' }) ] }),
  row('inactive', 'item.ring1.explicit', 'ItemAffix', 'Sapphire Ring', '25% increased Damage with Hits against Chilled Enemies', {
    reason: 'needs EnemyChilled', modifiers: [mod('Damage', 'INC', 25, 'inactive', { reason: 'needs EnemyChilled', read_gate: { state: 'EnemyChilled', config_option: 'conditionEnemyChilled' } })] }),
  row('inactive', 'node.35453', 'PassiveNode', 'Battle Trance', '30% increased Attack Damage while Surrounded', {
    reason: 'needs Attack', modifiers: [mod('Damage', 'INC', 30, 'inactive', { reason: 'needs Attack' })] }),
  row('inactive', 'node.2491', 'PassiveNode', 'Heart of Ice', '40% increased Critical Hit Chance if you have Frenzy Charges', {
    reason: 'needs 1+ Frenzy Charges', modifiers: [mod('CritChance', 'INC', 40, 'inactive', { reason: 'needs 1+ Frenzy Charges', read_gate: { state: 'FrenzyCharges', config_option: 'useFrenzyCharges' } })] }),
  row('inactive', 'flask.flask1', 'Flask', 'Quicksilver Flask', 'Gain Onslaught during Effect', {
    reason: 'flask is not active', modifiers: [mod('Onslaught', 'FLAG', true, 'inactive', { reason: 'flask is not active', read_gate: { state: 'UsingFlask', config_option: 'conditionUsingFlask' } })] }),
  row('unread', 'item.belt.explicit', 'ItemAffix', 'Heavy Belt', '+15% to Lightning Resistance when on Low Life', {
    reason: null, modifiers: [mod('LightningResist', 'BASE', 15, 'unread', { reason: 'LowLife is set by a pass that did not run' })] }),
  row('unread', 'node.51921', 'PassiveNode', 'Ancestral Alacrity', '8% increased Totem Placement speed', {
    modifiers: [mod('TotemPlacementSpeed', 'INC', 8, 'unread', { reason: 'no totem skill is selected' })] }),
  row('unmodelled', 'node.64405', 'PassiveNode', 'Heroic Tragedy', 'Passives in Radius of Remembrancing are rebuilt from the seed', {
    reason: 'seeded notable replacements are not modelled' }),
  row('unmodelled', 'item.weapon1.explicit', 'ItemAffix', 'Chiming Staff', 'Gain an Endurance Charge on every Tenth Cast', {
    reason: 'event frequency is not modelled' }),
  row('unparsed', 'item.ring2.explicit', 'ItemAffix', 'Amethyst Ring', '12% increased Hallucination of Frozen Enemies', {
    reason: 'no rule matched this line' }),
  row('unparsed', 'item.boots.explicit', 'ItemAffix', 'Wayfarer Boots', 'Skills have 20% chance to Refund Life', {
    reason: 'no rule matched this line' }),
  row('no_modifier', 'jewel.2', 'Jewel', 'Cobalt Jewel', '5% increased Effect of Chill on you', {
    reason: 'parsed, but the player-side Chill view is not stored' }),
  row('outside', 'item.helmet.implicit', 'ItemImplicit', 'Hubris Circlet', '+40 to maximum Energy Shield', {
    reason: 'item defences are computed per item (item::defence)', used_in: ['EnergyShield'] }),
  row('outside', 'item.weapon1.gem', 'ItemExplicit', 'Chiming Staff', '+2 to Level of all Cold Spell Skills', {
    reason: 'gem levels are resolved before skills are built (skill::resolve)' }),
  row('outside', 'jewel.1', 'Jewel', 'Timeless Jewel', 'Bathed in the blood of 12000 sacrificed in the name of Xibaqua', {
    reason: 'jewel radius is read by the tree pass' }),
  row('no_effect', 'node.52068', 'PassiveNode', 'Lifelong Learner', '5% increased Experience Gain', { reason: 'registered as having no effect on any output' }),
  row('no_effect', 'quest', 'Config', 'Campaign reward', 'Cannot be Chilled while on Frozen Ground', { reason: 'config option nobody asks for' }),
  row('applied', 'quest', 'Config', 'Campaign reward', '+20 to maximum Life', {
    used_in: ['Life'], modifiers: [mod('Life', 'BASE', 20, 'applied', { used_in: ['Life'] })] }),
  row('applied', 'config.custom', 'Config', null, '+10% to all Elemental Resistances', {
    used_in: ['FireResist', 'ColdResist', 'LightningResist'], modifiers: [mod('ElementalResist', 'BASE', 10, 'applied', { used_in: ['FireResist', 'ColdResist', 'LightningResist'] })] }),
  row('applied', 'gem.stat', 'Gem', 'Frost Bomb', null, {
    used_in: ['TotalDPS'], modifiers: [mod('FrostBombDamage', 'MORE', 25, 'applied', { used_in: ['TotalDPS'] })] }),
];

export function mockLineUsage(): LineUsageResponse {
  const count = (status: LineUsageStatus) => LINES.filter(line => line.status === status).length;
  const modifiers = LINES.flatMap(line => line.modifiers);
  const unparsedTexts = LINES.filter(line => line.status === 'unparsed' && line.text);
  return {
    usage: {
      summary: {
        lines: LINES.length, lines_applied: count('applied'), lines_partial: count('partial'), lines_inactive: count('inactive'),
        lines_unread: count('unread'), lines_outside: count('outside'), lines_no_effect: count('no_effect'),
        lines_unmodelled: count('unmodelled'), lines_unparsed: count('unparsed'), lines_no_modifier: count('no_modifier'),
        modifiers: modifiers.length, modifiers_applied: modifiers.filter(m => m.status === 'applied').length,
        modifiers_inactive: modifiers.filter(m => m.status === 'inactive').length, modifiers_unread: modifiers.filter(m => m.status === 'unread').length,
        modifiers_outside: modifiers.filter(m => m.status === 'outside').length, unparsed: unparsedTexts.length,
        unmapped_gem_stats: 1, unknown_gem_stats: 1, unset_conditions: 3, unprovided_multipliers: 0,
      },
      main_skill: 'FrostBombPlayer',
      lines: LINES,
      unparsed: unparsedTexts.map(line => ({ text: line.text!, count: 1, sources: [{ kind: 'Item', id: line.source_id, holder: line.holder }] })),
      gem_stats: [
        { stat: 'frost_bomb_chill_effect_per_stack', owner: 'FrostBombPlayer', status: 'unmapped', reason: 'unknown mod name' },
        { stat: 'skill_effect_animation_speed', owner: 'FrostBombPlayer', status: 'unknown', reason: null },
      ],
      conditions: [
        { condition: 'EnemyChilled', config_option: 'conditionEnemyChilled', flagged: false, modifiers: [{ name: 'Damage', mod_type: 'INC', value: '25', source: 'ItemAffix:item.ring1.explicit | 25% increased Damage with Hits against Chilled Enemies' }] },
        { condition: 'FrenzyCharges', config_option: 'useFrenzyCharges', flagged: false, modifiers: [{ name: 'CritChance', mod_type: 'INC', value: '40', source: 'PassiveNode:node.2491 | 40% increased Critical Hit Chance if you have Frenzy Charges' }] },
        { condition: 'UsingFlask', config_option: null, flagged: false, modifiers: [{ name: 'Onslaught', mod_type: 'FLAG', value: 'true', source: 'Flask:flask.flask1 | Gain Onslaught during Effect' }] },
      ],
      multipliers: [],
      skills: [{ skill_id: 'FrostBombPlayer', level: 20, quality: 0 }],
      reads: {},
    },
    item_errors: [],
  };
}
