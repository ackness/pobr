//! Fail on game-data upgrades that invalidate the fixed possession buff values.
use pobr_core::calc::charm_spirits::{SPIRIT_DEFINITIONS, SPIRIT_FAMILY_STATS};
use pobr_data::{catalog::ModDef, modifier::ModType};
use pobr_gamedata::{GameData, current_data_dir};

fn game_mod<'a>(mods: &'a [ModDef], id: &str) -> &'a ModDef {
    mods.iter()
        .find(|modifier| modifier.id == id)
        .unwrap_or_else(|| panic!("active base/mods.json is missing {id}"))
}

fn assert_game_value(modifier: &ModDef, stat_id: &str, engine_value: f64, scale: f64) {
    let stat = modifier
        .stats
        .iter()
        .find(|stat| stat.stat_id == stat_id)
        .unwrap_or_else(|| {
            panic!(
                "active base/mods.json: {} is missing {stat_id}",
                modifier.id
            )
        });
    for (bound, value) in [("min", stat.min), ("max", stat.max)] {
        assert_eq!(
            engine_value,
            value as f64 * scale,
            "hardcoded possession value differs from {}.{stat_id} {bound}; update calc::charm_spirits for the active game data",
            modifier.id,
        );
    }
}

fn numeric_stat(name: &str) -> (&str, ModType, f64) {
    use ModType::{Base, Inc};
    match name {
        "MaximumLife" => ("maximum_life_+%", Inc, 1.0),
        "EnemyHeavyStunBuildup" => ("hit_damage_stun_multiplier_+%", Inc, 1.0),
        "StunThreshold" => ("stun_threshold_+%", Inc, 1.0),
        "DamageGainAsFire" => ("non_skill_base_all_damage_%_to_gain_as_fire", Base, 1.0),
        // The game boolean "always" becomes a 100% application chance.
        "BleedChance" => ("global_bleed_on_hit", Base, 100.0),
        "PoisonChance" => ("global_poison_on_hit", Base, 100.0),
        "SlowEffectOnSelf" => ("base_slow_potency_+%", Inc, 1.0),
        "AilmentThreshold" => ("ailment_threshold_+%", Inc, 1.0),
        "Armour" => ("physical_damage_reduction_rating_+%", Inc, 1.0),
        "Evasion" => ("evasion_rating_+%", Inc, 1.0),
        "CriticalStrikeChance" => ("critical_strike_chance_+%", Inc, 1.0),
        "DamageGainAsLightning" => (
            "non_skill_base_all_damage_%_to_gain_as_lightning",
            Base,
            1.0,
        ),
        "FireResistance" | "ColdResistance" | "LightningResistance" => {
            ("base_resist_all_elements_%", Base, 1.0)
        }
        "DamageGainAsCold" => ("non_skill_base_all_damage_%_to_gain_as_cold", Base, 1.0),
        "EnergyShield" => ("maximum_energy_shield_+%", Inc, 1.0),
        "EnemyFreezeBuildup" => ("hit_damage_freeze_multiplier_+%", Inc, 1.0),
        "DamageTaken" => ("base_damage_taken_+%", Inc, 1.0),
        "SkillSpeed" => ("skill_speed_+%", Inc, 1.0),
        "MovementSpeed" => ("base_movement_velocity_+%", Inc, 1.0),
        "Damage" => ("damage_+%", Inc, 1.0),
        _ => panic!("possession modifier {name} needs a game-stat cross-check"),
    }
}

#[test]
fn hardcoded_spirit_buffs_match_active_game_mods() {
    let mods = GameData::new(current_data_dir())
        .mods()
        .expect("load active base/mods.json");
    assert_eq!(SPIRIT_DEFINITIONS.len(), 9);
    for &(spirit, family, stats, flags) in SPIRIT_DEFINITIONS {
        let (game_spirit, expected_family) = match spirit {
            "Bear" | "Boar" | "Ox" => (spirit, 0),
            "Cat" | "Stag" | "Wolf" => (spirit, 1),
            "Owl" => (spirit, 2),
            "Primate" => ("Monkey", 2),
            "Serpent" => ("Snake", 2),
            _ => panic!("spirit {spirit} needs a game-mod cross-check"),
        };
        assert_eq!(family, expected_family, "{spirit} possession family");
        let modifier = game_mod(&mods, &format!("SpiritOfThe{game_spirit}PossessedPlayer"));
        for &(name, kind, value) in stats {
            let (stat_id, expected_kind, scale) = numeric_stat(name);
            assert_eq!(kind, expected_kind, "{spirit}: {name}");
            assert_game_value(modifier, stat_id, value, scale);
        }
        for &name in flags {
            let stat_id = match name {
                "PhysicalCanChill" | "FireCanChill" | "LightningCanChill" | "ChaosCanChill" => {
                    "all_damage_can_chill"
                }
                "FireCanPoison" | "ColdCanPoison" | "LightningCanPoison" => "all_damage_can_poison",
                _ => panic!("possession flag {name} needs a game-stat cross-check"),
            };
            assert_game_value(modifier, stat_id, 1.0, 1.0);
        }
    }
    for (stats, family) in SPIRIT_FAMILY_STATS.iter().zip(["Str", "Dex", "Int"]) {
        let modifier = game_mod(&mods, &format!("SpiritPossessed{family}Player"));
        for &(name, kind, value) in *stats {
            let (stat_id, expected_kind, scale) = numeric_stat(name);
            assert_eq!(kind, expected_kind, "{family}: {name}");
            assert_game_value(modifier, stat_id, value, scale);
        }
    }
}
