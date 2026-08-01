//! Mageblood legacies application (vendor `CalcPerform.lua:66-142` legacies
//! table + `:1502-1528` application logic; line numbers verified against source).
//!
//! Each selected variant on a Mageblood belt renders as a `Legacy of <X>` line;
//! the parse layer's `special:mageblood_legacy` handler turns it into a
//! `LegacyOf<X>` BASE 1 mod plus a `MagebloodEquipped` FLAG (vendor
//! `ModParser.lua:5554-5557`). The implicit "All Mage's Legacies have N%
//! increased effect per duplicate..." becomes `MagesLegacyEffect` INC N (special
//! DSL). This module aggregates those marker mods into real armour/evasion/
//! resistance mods etc.
//!
//! Vendor logic (`:1502-1528`):
//! - No-op unless the `MagebloodEquipped` flag is set;
//! - For each legacy name, `Sum("BASE", LegacyOf<X>)` gives how many copies of
//!   that variant are equipped (stacks);
//! - `totalDuplicates = Σ max(stacks-1, 0)` (the count above 1 for each variant);
//! - `globalEffect = 1 + totalDuplicates × Sum("INC", MagesLegacyEffect)/100`;
//! - Each **present** legacy applies its effect **once** (not multiplied by its
//!   own stack count — duplicates only scale up `globalEffect`), with value
//!   `floor(globalEffect × base)` and source = "Mageblood".
//!
//! No-op safe: writes nothing without `MagebloodEquipped` or any `LegacyOf*`
//! mod present. Idempotent: skips re-injection (by checking source) if already
//! applied to this `Env`.

use pobr_data::prelude::*;

use super::Env;
use crate::Modifier;

/// A single legacy effect: (stat name, mod type, base value).
type LegacyEffect = (&'static str, ModType, f64);
/// A legacy: (`LegacyOf<X>` name, list of effects).
type LegacyDef = (&'static str, &'static [LegacyEffect]);

/// Line-for-line mirror of the vendor `legacies` table (`CalcPerform.lua:66-142`).
const LEGACIES: &[LegacyDef] = &[
    ("LegacyOfAmethyst", &[("ChaosResist", ModType::Base, 45.0)]),
    ("LegacyOfBasalt", &[("Armour", ModType::Inc, 150.0)]),
    (
        "LegacyOfBismuth",
        &[("ElementalResist", ModType::Base, 45.0)],
    ),
    // Vendor stat `CritChance` maps to the PoBR consumer name
    // `CriticalStrikeChance` (calc::crit reads the latter; same mapping as
    // special_mod::translate_vendor_name). Direct injection here bypasses the
    // parser's translation, so the table must already use the PoBR canonical
    // name or the value lands in a dead bucket — same lesson as the Virtuous
    // Barrier Life→MaximumLife bug.
    (
        "LegacyOfDiamond",
        &[("CriticalStrikeChance", ModType::Inc, 75.0)],
    ),
    ("LegacyOfGold", &[("LootRarity", ModType::Inc, 45.0)]),
    ("LegacyOfGranite", &[("Armour", ModType::Base, 2000.0)]),
    ("LegacyOfJade", &[("Evasion", ModType::Base, 2000.0)]),
    (
        "LegacyOfQuicksilver",
        &[("MovementSpeed", ModType::Inc, 30.0)],
    ),
    (
        "LegacyOfRuby",
        &[
            ("FireResist", ModType::Base, 60.0),
            ("FireResistMax", ModType::Base, 5.0),
        ],
    ),
    (
        "LegacyOfSapphire",
        &[
            ("ColdResist", ModType::Base, 60.0),
            ("ColdResistMax", ModType::Base, 5.0),
        ],
    ),
    (
        "LegacyOfSilver",
        &[
            // Vendor's bare `Speed` (generic action speed) maps to the PoBR
            // speed-bucket name `SkillSpeed` (covers both attack and cast,
            // SPEED_BUCKET; same dead-bucket class as CritChance —
            // translate_vendor_name maps bare Speed → SkillSpeed).
            ("SkillSpeed", ModType::Inc, 30.0),
            ("WarcrySpeed", ModType::Inc, 30.0),
            ("TotemPlacementSpeed", ModType::Inc, 30.0),
        ],
    ),
    ("LegacyOfStibnite", &[("Evasion", ModType::Inc, 150.0)]),
    ("LegacyOfSulphur", &[("Damage", ModType::Inc, 60.0)]),
    (
        "LegacyOfTopaz",
        &[
            ("LightningResist", ModType::Base, 60.0),
            ("LightningResistMax", ModType::Base, 5.0),
        ],
    ),
];

/// Source string for injected mods (vendor `NewMod(..., "Mageblood")`) —
/// doubles as the idempotency check.
const MAGEBLOOD_SOURCE: &str = "Mageblood";

/// env_finalize stage: applies Mageblood legacies (vendor `CalcPerform.lua:1502-1528`).
pub fn apply_mageblood_legacies(env: &mut Env) {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    if !db.flag(cfg, ModName::from("MagebloodEquipped")) {
        return;
    }
    // Idempotency guard: already injected for this Env (repeated perform).
    if db
        .iter_mods()
        .any(|m| m.source.as_deref() == Some(MAGEBLOOD_SOURCE))
    {
        return;
    }

    // Count stacks per legacy (`Sum("BASE", LegacyOf<X>)` = copies equipped).
    let found: Vec<(f64, &'static [LegacyEffect])> = LEGACIES
        .iter()
        .filter_map(|(name, effects)| {
            let stacks = db.sum(ModType::Base, cfg, &[ModName::from(*name)]);
            (stacks > 0.0).then_some((stacks, *effects))
        })
        .collect();
    if found.is_empty() {
        return;
    }

    let total_duplicates: f64 = found
        .iter()
        .map(|(stacks, _)| (stacks - 1.0).max(0.0))
        .sum();
    let effect_per_dupe = db.sum(ModType::Inc, cfg, &[ModName::from("MagesLegacyEffect")]);
    let global_effect = 1.0 + total_duplicates * (effect_per_dupe / 100.0);

    let source_id = SourceId::new(SourceKind::Item, "mageblood");
    let mut injected: Vec<Modifier> = Vec::new();
    for (_, effects) in &found {
        for (stat, mod_type, value) in *effects {
            let scaled = (global_effect * value).floor();
            let mut origin = ModifierSource::new(source_id.clone());
            origin.raw_text = Some(MAGEBLOOD_SOURCE.to_string());
            injected.push(
                Modifier::number(*stat, *mod_type, scaled)
                    .with_source(MAGEBLOOD_SOURCE)
                    .with_origin(origin),
            );
        }
    }
    env.player.mod_db.add_list(injected);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Actor, ActorBaseStats};

    /// Equips Mageblood plus some `LegacyOf*` marker mods, runs aggregation, and returns the `Env`.
    fn run(legacy_stacks: &[(&str, f64)], effect_per_dupe: f64) -> Env {
        let mut player = Actor::new(1, ActorBaseStats::default());
        player.mod_db.add_mod(Modifier::flag("MagebloodEquipped"));
        for (name, stacks) in legacy_stacks {
            for _ in 0..(*stacks as usize) {
                player
                    .mod_db
                    .add_mod(Modifier::number(*name, ModType::Base, 1.0));
            }
        }
        if effect_per_dupe > 0.0 {
            player.mod_db.add_mod(Modifier::number(
                "MagesLegacyEffect",
                ModType::Inc,
                effect_per_dupe,
            ));
        }
        let mut env = Env::new(player);
        apply_mageblood_legacies(&mut env);
        env
    }

    fn base(env: &Env, name: &str) -> f64 {
        env.player
            .mod_db
            .sum(ModType::Base, &env.cfg, &[ModName::from(name)])
    }
    fn inc(env: &Env, name: &str) -> f64 {
        env.player
            .mod_db
            .sum(ModType::Inc, &env.cfg, &[ModName::from(name)])
    }

    #[test]
    fn no_flag_no_effect() {
        let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
        apply_mageblood_legacies(&mut env);
        assert_eq!(base(&env, "Armour"), 0.0);
    }

    /// ice-shot build: Jade + Stibnite with no duplicates → globalEffect=1, Evasion BASE 2000 + INC 150.
    #[test]
    fn distinct_legacies_no_duplicate_bonus() {
        let env = run(&[("LegacyOfJade", 1.0), ("LegacyOfStibnite", 1.0)], 0.0);
        assert_eq!(base(&env, "Evasion"), 2000.0);
        assert_eq!(inc(&env, "Evasion"), 150.0);
    }

    /// titan build: Silver×2 → totalDuplicates=1, globalEffect=1.46; Basalt Armour INC
    /// floor(1.46×150)=219 (pinned against the observed vendor `defenceModList` value).
    #[test]
    fn duplicate_scales_global_effect() {
        let env = run(
            &[
                ("LegacyOfBasalt", 1.0),
                ("LegacyOfSilver", 2.0),
                ("LegacyOfQuicksilver", 1.0),
            ],
            46.0,
        );
        assert_eq!(inc(&env, "Armour"), 219.0); // floor(1.46 × 150)
        assert_eq!(inc(&env, "MovementSpeed"), (1.46 * 30.0f64).floor()); // 43
        // Silver applies only once (duplicates only scale globalEffect, not stack count).
        assert_eq!(inc(&env, "SkillSpeed"), (1.46 * 30.0f64).floor()); // 43, not 86
    }

    /// Idempotent: repeated perform does not re-inject.
    #[test]
    fn idempotent_reapply() {
        let mut env = run(&[("LegacyOfGranite", 1.0)], 0.0);
        assert_eq!(base(&env, "Armour"), 2000.0);
        apply_mageblood_legacies(&mut env);
        assert_eq!(base(&env, "Armour"), 2000.0);
    }
}
