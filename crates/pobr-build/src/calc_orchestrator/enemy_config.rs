//! Explicit incoming-hit inputs replace the enemy preset, including zero.

use pobr_core::rules::config_interpreter::ConfigInputValue;
use pobr_core::{ModValue, Modifier};
use pobr_data::prelude::*;

use crate::Build;

pub(super) fn modifiers(build: &Build) -> Vec<Modifier> {
    let inputs = &build.config.raw_inputs.values;
    let mut result = Vec::new();
    let mut push = |input: &str, name: &str, value: f64, text: String| {
        result.push(
            Modifier::new(name, ModType::Override, ModValue::Number(value))
                .with_source(format!("{input} = {text}"))
                .with_origin(ModifierSource::new(SourceId::new(
                    SourceKind::EnemyConfig,
                    format!("config.{input}"),
                ))),
        );
    };
    for (input, name) in [
        ("enemyPhysicalDamage", "EnemyPhysicalDamage"),
        ("enemyFireDamage", "EnemyFireDamage"),
        ("enemyColdDamage", "EnemyColdDamage"),
        ("enemyLightningDamage", "EnemyLightningDamage"),
        ("enemyChaosDamage", "EnemyChaosDamage"),
        ("enemyFirePen", "EnemyFirePen"),
        ("enemyColdPen", "EnemyColdPen"),
        ("enemyLightningPen", "EnemyLightningPen"),
        ("enemyCritChance", "EnemyCritChance"),
        ("enemyCritDamage", "EnemyCritDamage"),
        ("enemyPhysicalOverwhelm", "EnemyPhysicalOverwhelm"),
    ] {
        if let Some(ConfigInputValue::Number(value)) = inputs.get(input)
            && value.is_finite()
        {
            push(input, name, value.max(0.0), value.to_string());
        }
    }
    if let Some(ConfigInputValue::Text(value)) = inputs.get("enemyDamageType") {
        let category = match value.as_str() {
            "Average" => Some(0.0),
            "Untyped" => Some(1.0),
            "Melee" => Some(2.0),
            "Projectile" => Some(3.0),
            "Spell" => Some(4.0),
            "SpellProjectile" => Some(5.0),
            // The damage-over-time view is not an incoming hit scenario.
            _ => None,
        };
        if let Some(category) = category {
            push(
                "enemyDamageType",
                "EnemyHitCategory",
                category,
                value.clone(),
            );
        }
    }
    if let Some(ConfigInputValue::Number(value)) = inputs.get("EHPUnluckyWorstOf")
        && matches!(*value, 1.0 | 2.0 | 4.0)
    {
        push(
            "EHPUnluckyWorstOf",
            "EHPUnluckyWorstOf",
            *value,
            value.to_string(),
        );
    }
    result
}
