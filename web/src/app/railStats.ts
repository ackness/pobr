const EQUIVALENTS: Record<string, string[]> = {
  TotalDPS: ['CombinedDPS'], FullDPS: ['CombinedDPS', 'MainMinionDPS'],
  WithIgniteDPS: ['CombinedDPS', 'TotalDPS'], WithBleedDPS: ['CombinedDPS', 'TotalDPS'],
  WithPoisonDPS: ['CombinedDPS', 'TotalDPS'], WithDotDPS: ['CombinedDPS', 'TotalDPS'],
  Spirit: ['SpiritUnreserved'],
};

/** The compact first screen; all other values belong to the remembered detail folds. */
export function headlineRows(values: ReadonlyMap<string, number | null>): string[] {
  const pool = (values.get('EnergyShield') ?? 0) > (values.get('Life') ?? 0) ? 'EnergyShield' : 'Life';
  return ['TotalEHP', pool, 'FireResist', 'ColdResist', 'LightningResist', 'ChaosResist', 'SpiritUnreserved']
    .filter(id => values.has(id));
}

/** Only equivalent derivatives are hidden; unrelated stats may happen to share a value. */
export function hideDuplicateRow(id: string, values: ReadonlyMap<string, number | null>): boolean {
  const value = values.get(id);
  return typeof value === 'number' && (EQUIVALENTS[id] ?? []).some(other => values.get(other) === value);
}
