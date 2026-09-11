import { expect, test } from 'vitest';
import { situationalAffix } from './tradeMechanics';
import { buildTradeQuery } from './trade';
const arrow = { id: 'explicit.arrow', line: '+25% surpassing chance to fire an additional arrow', value: 25 };
test('measured expected projectiles survive zero DPS gain, but unrelated skills do not gain projectile advice', () => {
  expect(situationalAffix(arrow, { ProjectileCount: 1, TotalDPS: 100 }, { ProjectileCount: 1.25, TotalDPS: 100 })).toMatchObject({ kind: 'projectiles', delta: 0.25 });
  expect(situationalAffix(arrow, { TotalDPS: 100 }, { TotalDPS: 100 })).toBeUndefined();
});
test('debuff opportunities need configuration and market requirements never invent DPS weights', () => {
  expect(situationalAffix({ ...arrow, line: '25% chance to Shock' }, {}, {})).toMatchObject({ kind: 'debuff' });
  const query = buildTradeQuery([], { category: 'armour.quiver', requiredStats: [arrow.id, arrow.id], maxLevel: 71 });
  expect(query.query.stats.at(-1)).toEqual({ type: 'and', filters: [{ id: arrow.id }] });
  expect(query.sort).toEqual({ price: 'asc' });
});
