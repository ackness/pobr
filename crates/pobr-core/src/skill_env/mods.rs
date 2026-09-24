//! Pure skill-group → `Modifier` translations (no context, no `Build`/`BuildData`).
//!
//! These are the engine-semantics helpers extracted from `pobr-build`'s
//! `skill/mods.rs`: they take plain data types and produce `Vec<Modifier>`,
//! letting the same logic be reused by WASM, CLI, and test harnesses.

use pobr_data::catalog::DotFlags;
use pobr_data::source::{ModifierSource, SourceId, SourceKind};

use crate::Modifier;

/// Maps the selected statSet's dotIs* flags into `FLAG` modifiers.
///
/// Vendor semantics: entries like statSet `baseMods`'s `skill("dotIsArea", true)` hang
/// directly on skillData. PoBR injects a FLAG under the same name as the stat-driven
/// channel (the dotIs* skill_data keys from `stat_map_engine::collect_skill_data`) —
/// `calc::skill_dot::DotIsFlags::from_db` is the unified consumption point for both
/// paths. Returns empty when all flags are false.
pub fn dot_flag_modifiers(flags: DotFlags, skill_id: &str) -> Vec<Modifier> {
    let pairs = [
        ("DotIsArea", flags.area),
        ("DotIsProjectile", flags.projectile),
        ("DotIsSpell", flags.spell),
        ("DotIsAttack", flags.attack),
        ("DotIsHit", flags.hit),
    ];
    pairs
        .iter()
        .filter(|(_, on)| *on)
        .map(|(name, _)| {
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::SkillGem,
                format!("skill.{skill_id}.{name}"),
            ))
            .with_raw_text(format!("statSet dot flag {name}"));
            Modifier::flag(*name).with_origin(origin)
        })
        .collect()
}

/// Whether a base_damage stat is the off-hand weapon's physical damage (already
/// counted into `base_hit_min/max` as a **weapon source** by
/// `non_weapon_attack_contribution`, so it must not also be injected as
/// `PhysicalDamageMin/Max` BASE via stat-map).
pub fn is_off_hand_weapon_base_stat(stat: &str) -> bool {
    matches!(
        stat,
        "off_hand_weapon_minimum_physical_damage" | "off_hand_weapon_maximum_physical_damage"
    )
}
