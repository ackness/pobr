//! Base monster resistances from the official monster-variety/type tables.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Normal-rarity base resistances, in percentage points, before modifiers.
/// Negative values are vulnerabilities. These are data prerequisites;
/// loading them does not apply resistances to minion calculations.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonsterResistanceDef {
    pub fire: i32,
    pub cold: i32,
    pub lightning: i32,
    pub chaos: i32,
}

/// `base/monster_resistances.json`, keyed by `MonsterVarieties.Id`.
/// Each variety combines all resistance records linked by its monster type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonsterResistancesDef {
    pub monsters: BTreeMap<String, MonsterResistanceDef>,
}
