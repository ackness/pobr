//! PoBR-owned exact-ID mapping for the bounded ordinary-passive native channel.

use pobr_data::modifier::ModType;
use pobr_data::native_stat::NativeStat;

use crate::Modifier;

/// Why a complete native packet could not supply this passive's modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativePassiveRejection {
    UnknownStat,
    UnsupportedContext,
    IncompletePacket,
    InexactValue,
    BindingMismatch,
    VersionMismatch,
    TransformedNode,
}

/// Maps a reviewed unconditional player stat, retaining exact signed integer values.
///
/// # Errors
/// Rejects unknown IDs and values not exactly representable by the engine's f64.
pub fn map_stat(stat: &NativeStat) -> Result<Modifier, NativePassiveRejection> {
    // Fixed client 0.5.5.5 and its CSD branches distinguish additive resistance
    // points from increased damage. Native Semantic/Category values do not
    // choose the aggregation type. See the source-backed native_passive_damage
    // fixture for all eight damage IDs and the passive-specific description.
    let (name, mod_type) = match stat.id.as_str() {
        "base_fire_damage_resistance_%" => ("FireResistance", ModType::Base),
        "physical_damage_+%" => ("PhysicalDamage", ModType::Inc),
        "fire_damage_+%" => ("FireDamage", ModType::Inc),
        "cold_damage_+%" => ("ColdDamage", ModType::Inc),
        "lightning_damage_+%" => ("LightningDamage", ModType::Inc),
        "chaos_damage_+%" => ("ChaosDamage", ModType::Inc),
        "elemental_damage_+%" => ("ElementalDamage", ModType::Inc),
        "spell_damage_+%" => ("SpellDamage", ModType::Inc),
        "projectile_damage_+%" => ("ProjectileDamage", ModType::Inc),
        _ => return Err(NativePassiveRejection::UnknownStat),
    };
    let value = stat.value as f64;
    // i128 avoids the saturating f64 -> i64 cast incorrectly accepting i64::MAX.
    if value as i128 != i128::from(stat.value) {
        return Err(NativePassiveRejection::InexactValue);
    }
    Ok(Modifier::number(name, mod_type, value))
}

#[cfg(test)]
mod tests {
    use pobr_data::{
        constants::DamageType,
        modifier::{ModFlags, ModType},
        native_stat::NativePassiveStats,
        passive_tree::NodeId,
    };

    use super::{NativePassiveRejection, map_stat};
    use crate::{
        CalcConfig, ModDb,
        calc::damage::{aggregate_inc_more, calculate_components},
        ingest::passive::{AllocatedNode, NativePassiveInput, ingest_passive_nodes_with_ctx},
        mod_parser::ParseCtx,
    };

    const FIXTURE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../pobr-data/tests/fixtures/native_passive_damage/packets.json"
    ));

    fn fixture() -> Vec<NativePassiveStats> {
        serde_json::from_str(FIXTURE).expect("native damage fixture is valid")
    }

    fn node(packet: NativePassiveStats) -> AllocatedNode {
        AllocatedNode {
            node_id: NodeId(packet.node_id),
            modifier_texts: packet.modifier_texts.clone(),
            native: Some(NativePassiveInput {
                packet,
                ineligible: None,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn native_damage_packets_feed_typed_noncritical_damage_buckets() {
        let packets = fixture();
        let nodes: Vec<_> = packets.iter().cloned().map(node).collect();
        let ingest = ingest_passive_nodes_with_ctx(&nodes, ParseCtx::none()).expect("ingest");
        assert_eq!(ingest.modifiers.len(), 8);
        assert!(ingest.unsupported.is_empty());
        assert!(ingest.native_diagnostics.is_empty());

        let mut db = ModDb::new();
        db.add_list(ingest.modifiers);
        let default = CalcConfig::new();
        let skill = CalcConfig::new().with_flags(
            pobr_data::modifier::ModFlags::SPELL | pobr_data::modifier::ModFlags::PROJECTILE,
        );
        for (ty, expected) in [
            (DamageType::Physical, 10.0),
            (DamageType::Fire, 20.0),
            (DamageType::Cold, 20.0),
            (DamageType::Lightning, 20.0),
            (DamageType::Chaos, 7.0),
        ] {
            assert_eq!(aggregate_inc_more(&db, &default, ty).0, expected, "{ty:?}");
        }
        assert_eq!(
            aggregate_inc_more(&db, &skill, DamageType::Physical).0,
            30.0
        );
        assert_eq!(
            aggregate_inc_more(
                &db,
                &default.clone().with_flags(ModFlags::SPELL),
                DamageType::Chaos
            )
            .0,
            17.0
        );
        assert_eq!(
            aggregate_inc_more(
                &db,
                &default.clone().with_flags(ModFlags::PROJECTILE),
                DamageType::Chaos
            )
            .0,
            17.0
        );

        // Generic ElementalDamage contributes to elemental final types only.
        assert_eq!(
            aggregate_inc_more(&db, &default, DamageType::Physical).0,
            10.0
        );
        assert_eq!(aggregate_inc_more(&db, &default, DamageType::Chaos).0, 7.0);

        let modifiers: Vec<_> = [
            "PhysicalDamage",
            "FireDamage",
            "ColdDamage",
            "LightningDamage",
            "ChaosDamage",
            "ElementalDamage",
            "SpellDamage",
            "ProjectileDamage",
        ]
        .iter()
        .map(|name| {
            db.named(name)
                .next()
                .expect("one mapped source per fixture ID")
        })
        .collect();
        for (packet, modifier) in packets.iter().zip(modifiers) {
            let origin = modifier
                .origin
                .as_ref()
                .expect("native modifier retains origin");
            assert_eq!(origin.native_stat.as_ref(), Some(&packet.stats[0]));
            assert_eq!(origin.source_id.id, format!("node.{}", packet.node_id));
            assert_eq!(
                origin.raw_text.as_deref(),
                packet.modifier_texts.first().map(String::as_str)
            );
        }
    }

    #[test]
    fn native_damage_mapping_is_exact_signed_and_unknowns_stay_rejected() {
        let mut resistance = fixture()[0].stats[0].clone();
        resistance.id = "base_fire_damage_resistance_%".into();
        resistance.value = 5;
        let modifier = map_stat(&resistance).unwrap();
        assert_eq!(modifier.name.as_str(), "FireResistance");
        assert_eq!(modifier.mod_type, ModType::Base);
        assert_eq!(modifier.value.as_number(), Some(5.0));

        let cases = [
            ("physical_damage_+%", "PhysicalDamage"),
            ("fire_damage_+%", "FireDamage"),
            ("cold_damage_+%", "ColdDamage"),
            ("lightning_damage_+%", "LightningDamage"),
            ("chaos_damage_+%", "ChaosDamage"),
            ("elemental_damage_+%", "ElementalDamage"),
            ("spell_damage_+%", "SpellDamage"),
            ("projectile_damage_+%", "ProjectileDamage"),
        ];
        for (id, name) in cases {
            let mut stat = fixture()[0].stats[0].clone();
            stat.id = id.into();
            for value in [-10, 0, 13, (1_i64 << 53) + 2] {
                stat.value = value;
                let modifier = map_stat(&stat).expect("exact signed i64 damage stat maps");
                assert_eq!(modifier.name.as_str(), name);
                assert_eq!(modifier.mod_type, ModType::Inc);
                assert_eq!(modifier.value.as_number(), Some(value as f64));
            }
            stat.value = (1_i64 << 53) + 1;
            assert_eq!(map_stat(&stat), Err(NativePassiveRejection::InexactValue));
            stat.value = i64::MAX;
            assert_eq!(map_stat(&stat), Err(NativePassiveRejection::InexactValue));
        }

        let mut unknown = fixture()[0].stats[0].clone();
        unknown.id = "physical_damage_+%_while_using_a_flask".into();
        assert_eq!(map_stat(&unknown), Err(NativePassiveRejection::UnknownStat));
    }

    #[test]
    fn native_damage_scales_existing_components_for_the_selected_skill() {
        for (flags, expected_increases) in [
            (ModFlags::HIT, [10.0, 20.0, 20.0, 20.0, 7.0]),
            (
                ModFlags::SPELL | ModFlags::HIT | ModFlags::PROJECTILE,
                [30.0, 40.0, 40.0, 40.0, 27.0],
            ),
            (
                ModFlags::ATTACK | ModFlags::HIT | ModFlags::PROJECTILE,
                [20.0, 30.0, 30.0, 30.0, 17.0],
            ),
        ] {
            let nodes: Vec<_> = fixture().into_iter().map(node).collect();
            let ingest = ingest_passive_nodes_with_ctx(&nodes, ParseCtx::none()).unwrap();
            let mut db = ModDb::new();
            db.add_list(ingest.modifiers);
            for prefix in ["Physical", "Fire", "Cold", "Lightning", "Chaos"] {
                db.add_mod(crate::Modifier::number(
                    format!("{prefix}DamageMin"),
                    ModType::Base,
                    100.0,
                ));
                db.add_mod(crate::Modifier::number(
                    format!("{prefix}DamageMax"),
                    ModType::Base,
                    200.0,
                ));
            }
            let components =
                calculate_components(&db, &CalcConfig::new().with_flags(flags), 0.0, 0.0);
            for (damage_type, increase) in [
                DamageType::Physical,
                DamageType::Fire,
                DamageType::Cold,
                DamageType::Lightning,
                DamageType::Chaos,
            ]
            .into_iter()
            .zip(expected_increases)
            {
                let component = components
                    .iter()
                    .find(|component| component.damage_type == damage_type)
                    .unwrap();
                let factor = 1.0 + increase / 100.0;
                assert!((component.min - 100.0 * factor).abs() < 1e-10);
                assert!((component.max - 200.0 * factor).abs() < 1e-10);
            }
        }
    }

    #[test]
    fn native_damage_signed_values_reach_the_component_reader() {
        for amount in [-10, 0, 37] {
            let mut packet = fixture().remove(3);
            packet.stats[0].value = amount;
            let ingest = ingest_passive_nodes_with_ctx(&[node(packet)], ParseCtx::none()).unwrap();
            assert!(ingest.native_diagnostics.is_empty());
            let mut db = ModDb::new();
            db.add_list(ingest.modifiers);
            db.add_mod(crate::Modifier::number(
                "LightningDamageMin",
                ModType::Base,
                1.0,
            ));
            db.add_mod(crate::Modifier::number(
                "LightningDamageMax",
                ModType::Base,
                10.0,
            ));
            let components = calculate_components(&db, &CalcConfig::spell(), 0.0, 0.0);
            let lightning = components
                .iter()
                .find(|component| component.damage_type == DamageType::Lightning)
                .unwrap();
            let factor = 1.0 + amount as f64 / 100.0;
            assert!((lightning.min - factor).abs() < 1e-10);
            assert!((lightning.max - 10.0 * factor).abs() < 1e-10);
        }
    }

    #[test]
    fn native_damage_packets_keep_whole_packet_fallback() {
        let mut packet = fixture().remove(0);
        packet.stats.push(packet.stats[0].clone());
        let ingest = ingest_passive_nodes_with_ctx(&[node(packet.clone())], ParseCtx::none())
            .expect("ingest");
        assert!(ingest.modifiers.is_empty());
        assert_eq!(ingest.native_diagnostics.len(), 1);
        assert_eq!(
            ingest.native_diagnostics[0].fallback_texts,
            packet.modifier_texts
        );

        for inexact in [false, true] {
            let mut packet = fixture().remove(0);
            let reason = if inexact {
                packet.stats[0].value = (1_i64 << 53) + 1;
                NativePassiveRejection::InexactValue
            } else {
                packet.stats[0].id = "damage_with_unknown_context".into();
                NativePassiveRejection::UnknownStat
            };
            let ingest =
                ingest_passive_nodes_with_ctx(&[node(packet.clone())], ParseCtx::none()).unwrap();
            assert!(ingest.modifiers.is_empty());
            assert_eq!(ingest.native_diagnostics.len(), 1);
            assert_eq!(ingest.native_diagnostics[0].reason, reason);
            assert_eq!(
                ingest.native_diagnostics[0].fallback_texts,
                packet.modifier_texts
            );
            assert_eq!(ingest.unsupported, packet.modifier_texts);
        }
    }
}
