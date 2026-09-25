use super::DataOrchestratorOptions;
use super::StatMapMode;
use super::collect::{combine_wrapped_then_filter, granted_passive_defs};
use super::conditions::{combat_conditions, weapon_cfg_flags, weapon_type_conditions};
use super::context::CalculationContext;
use super::item::mirror::slot_bonus_effect_scales;
use pobr_core::skill_env::unarmed_contribution;
use super::skill::buffs::{
    buff_skill_specs, herald_skill_names, self_buff_offensive_modifiers, support_buff_specs,
};
use super::skill::mods::{
    main_skill_quality_modifiers, skill_base_modifiers, unselected_set_global_modifiers,
};
use super::skill::resolve::{
    GemPropertyBonus, GemPropertyKind, gem_property_bonuses, pick_group_main_skill,
};
use super::skill::triggers::{trigger_modifiers, trigger_source_stats};
use super::stat_map::exposure_support_modifiers;
use super::*;
use super::{
    calculate, calculate_with_data, calculate_with_data_report, calculate_with_data_session,
};
use crate::build::{Build, CharacterIdentity, RadiusJewel, SocketGroup};
use crate::build_data::{BuildData, ClassBaseAttributes, ResolvedSkillLevel};
use pobr_core::CampaignProgress;
use pobr_core::Modifier;
use pobr_core::calc::BuffKind;
use pobr_core::calc::CalculationSession;
use pobr_core::calc::MinimalInput;
use pobr_core::mod_parser::ParseCtx;
use pobr_core::rules::stat_map_engine::StatMapCatalog;
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_data::modifier::{ModFlags, ModType};
use pobr_data::monster::EnemyTier;
use pobr_data::passive_tree::{NodeId, PassiveTreeSpec};
use pobr_data::source::SourceKind;
use pobr_gamedata::{GameData, repo_data_root};
use std::collections::HashMap;

#[cfg(test)]
/// Shared engine rules for tests (real data directory, reused across the whole test binary).
pub(crate) fn test_parser_rules() -> std::sync::Arc<pobr_core::mod_parser::CompiledParserRules> {
    static RULES: std::sync::LazyLock<std::sync::Arc<pobr_core::mod_parser::CompiledParserRules>> =
        std::sync::LazyLock::new(|| {
            std::sync::Arc::new(pobr_core::mod_parser::test_compiled_rules())
        });
    RULES.clone()
}

#[cfg(test)]
mod ring3_tests {
    use super::*;
    use crate::build::Build;
    use crate::build_data::BuildData;
    use pobr_core::calc::MinimalInput;
    use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
    use pobr_data::passive_tree::{NodeId, PassiveTreeSpec};
    use std::collections::HashMap;

    fn life_ring() -> Item {
        Item {
            base: ItemBaseId::from("Ring"),
            rarity: ItemRarity::Rare,
            quality: 0,
            corrupted: false,
            implicit_texts: vec![],
            modifier_texts: vec!["+30 to maximum Life".into()],
            enchant_texts: vec![],
            rolled_defence: RolledDefence::default(),
            parsed_stats: vec![],
        }
    }

    fn ring_slot_data() -> BuildData {
        // A "+1 Ring Slot" mod node (modelled after Ritualist's "Unfurled Finger").
        let node = pobr_data::catalog::PassiveNodeDef {
            apply_to_armour: false,
            skill: 34785,
            id: "ascendancy_ritualist_unfurled_finger".into(),
            name: Some("Unfurled Finger".into()),
            kind: pobr_data::catalog::PassiveNodeKind::Notable,
            stats: vec!["+1 Ring Slot".into()],
            group: None,
            orbit: None,
            orbit_index: None,
            x: None,
            y: None,
            connections: vec![],
            ascendancy_id: Some("Huntress3".into()),
            unlock_constraint: None,
            variants: vec![],
        };
        let mut passive_nodes = HashMap::new();
        passive_nodes.insert(34785u32, node);
        BuildData {
            passive_nodes,
            parser_rules: Some(test_parser_rules()),
            ..BuildData::empty()
        }
    }

    fn base_opts() -> DataOrchestratorOptions {
        DataOrchestratorOptions {
            base_input: MinimalInput {
                base_life: 100.0,
                ..MinimalInput::default()
            },
            inject_character_base: false,
            ..Default::default()
        }
    }

    /// Without an allocated "+1 Ring Slot", the Ring 3 item is ignored entirely
    /// (same semantics as PoB2 CalcSetup.lua:821 "ignore item in Ring 3 if The
    /// Unseen Hand is not allocated").
    #[test]
    fn ring3_ignored_without_additional_ring_slot() {
        let build = Build::new().set_item(EquipmentSlot::Ring3, life_ring());
        let out = calculate_with_data(&build, &ring_slot_data(), &base_opts()).expect("calc");
        assert_eq!(
            out.life, 100.0,
            "Ring 3 is ignored when +1 Ring Slot has not been allocated"
        );
    }

    /// Ring 3 mods take effect once the "+1 Ring Slot" node is allocated.
    #[test]
    fn ring3_counts_with_additional_ring_slot() {
        let build = Build::new()
            .set_item(EquipmentSlot::Ring3, life_ring())
            .with_tree(PassiveTreeSpec {
                allocated_nodes: vec![NodeId(34785)],
                ..Default::default()
            });
        let out = calculate_with_data(&build, &ring_slot_data(), &base_opts()).expect("calc");
        assert_eq!(
            out.life, 130.0,
            "Ring 3 mods take effect once +1 Ring Slot is allocated"
        );
    }
}

/// Engine parse context for tests (real rules, compiled once and shared across the process).
fn test_ctx() -> ParseCtx<'static> {
    use std::sync::LazyLock;
    static RULES: LazyLock<std::sync::Arc<pobr_core::mod_parser::CompiledParserRules>> =
        LazyLock::new(test_parser_rules);
    ParseCtx::with_engine(&RULES)
}

/// Wrapped tree-line merging (vendor PassiveTree.lua:445-462): when a single
/// line fails to parse, it's joined with the next line and retried; if that
/// also fails, the line is dropped and subsequent lines continue independently.
#[test]
fn combine_wrapped_then_filter_joins_wrapped_tree_lines() {
    // Demolitionist example: two lines = one mod (a `\n`-wrapped stat in the catalog).
    let joined = combine_wrapped_then_filter(
        vec![
            "Gain 4% of Damage as Extra Fire Damage for".into(),
            "every different Grenade fired in the past 8 seconds".into(),
        ],
        test_ctx(),
    );
    assert_eq!(
            joined,
            vec![
                "Gain 4% of Damage as Extra Fire Damage for every different Grenade fired in the past 8 seconds"
                    .to_string()
            ]
        );

    // Independently parseable lines are unaffected; lines that still fail after merging are dropped, as before.
    let mixed = combine_wrapped_then_filter(
        vec![
            "10% increased Damage".into(),
            "this line is not a known modifier".into(),
            "+50 to maximum Life".into(),
        ],
        test_ctx(),
    );
    assert_eq!(
        mixed,
        vec![
            "10% increased Damage".to_string(),
            "+50 to maximum Life".to_string()
        ]
    );
}

fn life_item(amount: &str) -> Item {
    Item {
        base: ItemBaseId::from("Iron Ring"),
        rarity: ItemRarity::Rare,
        quality: 0,
        corrupted: false,
        implicit_texts: vec![],
        modifier_texts: vec![format!("+{amount} to maximum Life")],
        enchant_texts: vec![],
        rolled_defence: RolledDefence::default(),
        parsed_stats: vec![],
    }
}

/// Anointed notables feed into the GemProperty scan (vendor: a granted node's
/// modList joins the global modDB just like an allocated node,
/// CalcSetup.lua:1322-1331 + applyGemMods): an amulet enchant "Allocates
/// Paragon" → the anoint-pool node 20686 (backfilled via --tree-anoints) whose
/// `+5% to Quality of all Skills` should produce a Quality +5 catch-all mod.
#[test]
fn granted_anoint_notable_feeds_gem_property_scan() {
    let data = repo_data();
    let amulet = Item {
        base: ItemBaseId::from("Solar Amulet"),
        rarity: ItemRarity::Rare,
        quality: 0,
        corrupted: false,
        implicit_texts: vec![],
        modifier_texts: vec![],
        enchant_texts: vec!["Allocates Paragon".into()],
        rolled_defence: RolledDefence::default(),
        parsed_stats: vec![],
    };
    let build = Build::new().set_item(EquipmentSlot::Amulet, amulet);

    // Name resolution: Paragon = anoint-pool node 20686 (not on the main tree, reachable only via a grant).
    let defs = granted_passive_defs(&build, &data);
    assert_eq!(
        defs.iter().map(|d| d.skill).collect::<Vec<_>>(),
        vec![20686],
        "Allocates Paragon should resolve to anoint-pool node 20686"
    );

    // GemProperty scan: +5% Quality (bare "all Skills", no attribute requirement).
    let bonuses = gem_property_bonuses(&build, &data);
    assert!(
        bonuses.contains(&GemPropertyBonus {
            value: 5,
            kind: GemPropertyKind::Quality,
            category: String::new(),
            attr_req: None,
        }),
        "granting Paragon should produce a catch-all Quality +5 mod, got {bonuses:?}"
    );

    // Idempotency: allocating the same node on the tree must not double-count it.
    let allocated = Build::new()
        .set_item(EquipmentSlot::Amulet, {
            let mut a = build.items[&EquipmentSlot::Amulet].clone();
            a.enchant_texts = vec!["Allocates Paragon".into()];
            a
        })
        .with_tree(PassiveTreeSpec {
            allocated_nodes: vec![NodeId(20686)],
            ..Default::default()
        });
    let quality_count = gem_property_bonuses(&allocated, &data)
        .iter()
        .filter(|b| b.kind == GemPropertyKind::Quality && b.value == 5)
        .count();
    assert_eq!(
        quality_count, 1,
        "allocated + granted should only count once"
    );

    // Duplicate names are deterministic even when tree definitions share a
    // display name. Case differences and repeated grants must not add copies.
    let mut data = data;
    let mut same_name = data.passive_nodes[&20686].clone();
    same_name.skill = 1;
    data.passive_nodes.insert(1, same_name.clone());
    same_name.skill = 0;
    same_name.kind = pobr_data::catalog::PassiveNodeKind::Normal;
    data.passive_nodes.insert(0, same_name);
    let mut build = build;
    build
        .items
        .get_mut(&EquipmentSlot::Amulet)
        .unwrap()
        .enchant_texts = vec![
        "Allocates PARAGON".into(),
        "Allocates Paragon".into(),
        "Allocates Unknown Notable".into(),
    ];
    assert_eq!(
        granted_passive_defs(&build, &data)
            .iter()
            .map(|node| node.skill)
            .collect::<Vec<_>>(),
        vec![1]
    );
}

fn repo_data() -> BuildData {
    let data = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
    BuildData::load(&data).expect("load repo build data")
}

// Text-only path (backward compatible, preserves the existing assertions)

#[test]
fn calculates_with_life_modifier() {
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 1,
            class_name: "Ranger".into(),
            ascendancy_name: String::new(),
        })
        .set_item(EquipmentSlot::Ring1, life_item("50"));

    let opts = OrchestratorOptions {
        base_input: MinimalInput {
            base_life: 100.0,
            ..MinimalInput::default()
        },
        extra_modifier_texts: vec![],
    };

    let out = calculate(&build, &opts).expect("calc");
    assert_eq!(out.life, 150.0);
}

#[test]
fn empty_build_calculates_base() {
    let build = Build::new();
    let opts = OrchestratorOptions {
        base_input: MinimalInput {
            base_life: 80.0,
            ..MinimalInput::default()
        },
        extra_modifier_texts: vec![],
    };
    let out = calculate(&build, &opts).expect("calc");
    assert_eq!(out.life, 80.0);
}

// End-to-end attribution path (calculate_with_data)

#[test]
fn data_path_item_life_matches_text_path() {
    // Equipment goes through the add_item attribution path; values should match the text-only path.
    let build = Build::new().set_item(EquipmentSlot::Ring1, life_item("50"));
    let data = BuildData {
        parser_rules: Some(test_parser_rules()),
        ..BuildData::empty()
    };
    let opts = DataOrchestratorOptions {
        base_input: MinimalInput {
            base_life: 100.0,
            ..MinimalInput::default()
        },
        inject_character_base: false,
        ..Default::default()
    };
    let out = calculate_with_data(&build, &data, &opts).expect("calc");
    assert_eq!(out.life, 150.0);
}

#[test]
fn character_base_injects_life_from_class_and_level() {
    // Derive CharacterBase from the injected class-attributes table; life = 28 + 12*level + 2*str.
    let mut class_attributes = HashMap::new();
    class_attributes.insert(
        "Warrior".to_string(),
        ClassBaseAttributes {
            strength: 15,
            dexterity: 7,
            intelligence: 7,
        },
    );
    let data = BuildData {
        class_attributes,
        ..BuildData::empty()
    };
    let build = Build::new().with_character(CharacterIdentity {
        level: 10,
        class_name: "Warrior".into(),
        ascendancy_name: String::new(),
    });

    let opts = DataOrchestratorOptions {
        inject_character_base: true,
        ..Default::default()
    };
    let out = calculate_with_data(&build, &data, &opts).expect("calc");
    // 12*10 + 16 + 2*15 = 166 (PoB2 `Life BASE 12 × Level + 16`).
    assert_eq!(out.life, 166.0);

    // Injection off → no CharacterBase life.
    let opts_off = DataOrchestratorOptions {
        inject_character_base: false,
        ..Default::default()
    };
    let out_off = calculate_with_data(&build, &data, &opts_off).expect("calc");
    assert_eq!(out_off.life, 0.0);
    assert!(
        out.life > out_off.life,
        "CharacterBase in effect raises life"
    );
}

#[test]
fn passive_node_contributes_attributed_life() {
    // Build a Normal node carrying +30 maximum Life; allocating it should raise life.
    let node = pobr_data::catalog::PassiveNodeDef {
        apply_to_armour: false,
        skill: 12345,
        id: "test_life_node".into(),
        name: Some("Life Node".into()),
        kind: pobr_data::catalog::PassiveNodeKind::Normal,
        stats: vec!["+30 to maximum Life".into()],
        group: None,
        orbit: None,
        orbit_index: None,
        x: None,
        y: None,
        connections: vec![],
        ascendancy_id: None,
        unlock_constraint: None,
        variants: vec![],
    };
    let mut passive_nodes = HashMap::new();
    passive_nodes.insert(12345u32, node);
    let data = BuildData {
        passive_nodes,
        parser_rules: Some(test_parser_rules()),
        ..BuildData::empty()
    };

    let build = Build::new().with_tree(PassiveTreeSpec {
        allocated_nodes: vec![NodeId(12345)],
        ..Default::default()
    });

    let opts = DataOrchestratorOptions {
        base_input: MinimalInput {
            base_life: 100.0,
            ..MinimalInput::default()
        },
        inject_character_base: false,
        ..Default::default()
    };
    let out = calculate_with_data(&build, &data, &opts).expect("calc");
    assert_eq!(
        out.life, 130.0,
        "node's +30 life takes effect via the node attribution path"
    );
}

/// Build a Normal node with coordinates (defaults to a +5 to maximum Life mod; overridable).
fn normal_node_at(
    skill: u32,
    x: f64,
    y: f64,
    stats: Vec<String>,
) -> pobr_data::catalog::PassiveNodeDef {
    pobr_data::catalog::PassiveNodeDef {
        apply_to_armour: false,
        skill,
        id: format!("n{skill}"),
        name: None,
        kind: pobr_data::catalog::PassiveNodeKind::Normal,
        stats,
        group: None,
        orbit: None,
        orbit_index: None,
        x: Some(x),
        y: Some(y),
        connections: vec![],
        ascendancy_id: None,
        unlock_constraint: None,
        variants: vec![],
    }
}

/// Dedicated unit test for the attribute-small-passive predicate.
#[test]
fn is_attribute_node_matches_any_attribute_choice() {
    let attr = normal_node_at(1, 0.0, 0.0, vec!["+5 to any Attribute".into()]);
    let attrs = normal_node_at(2, 0.0, 0.0, vec!["+5 to any Attributes".into()]);
    let life = normal_node_at(3, 0.0, 0.0, vec!["+5 to maximum Life".into()]);
    assert!(super::is_attribute_node(&attr));
    assert!(super::is_attribute_node(&attrs));
    assert!(!super::is_attribute_node(&life));
}

/// **Dedicated regression: radius-jewel attribute miscounting** (a named
/// roadmap acceptance item).
///
/// The Small count for `Small Passive Skills in Radius also grant <mod>` must
/// exclude attribute small passives (vendor ModParser.lua:6855-6857
/// `node.type=="Normal" and not node.isAttribute`). With 1 normal life small
/// passive + 1 attribute-choice small passive in radius, the grant count
/// should be 1 (not 2).
#[test]
fn radius_small_grant_excludes_attribute_nodes() {
    let socket = 100u32;
    // All three nodes sit near the socket (distance << any radius tier).
    let mut passive_nodes = HashMap::new();
    // The socket node itself (Normal; excluded from the geometry calc by definition).
    passive_nodes.insert(socket, normal_node_at(socket, 0.0, 0.0, vec![]));
    // A normal life small passive (should count toward Small).
    passive_nodes.insert(
        101,
        normal_node_at(101, 50.0, 0.0, vec!["+5 to maximum Life".into()]),
    );
    // An attribute-choice small passive (must be excluded).
    passive_nodes.insert(
        102,
        normal_node_at(102, 0.0, 50.0, vec!["+5 to any Attribute".into()]),
    );

    let data = BuildData {
        passive_nodes,
        parser_rules: Some(test_parser_rules()),
        ..BuildData::empty()
    };

    let jewel = RadiusJewel {
        socket_node: socket,
        radius_label: Some("Large".into()),
        grant_lines: vec!["Small Passive Skills in Radius also grant +10 to maximum Mana".into()],
        notable_effect_inc: 0,
        small_effect_inc: 0,
        tree_texts: vec![],
    };
    let build = Build::new()
        .with_tree(PassiveTreeSpec {
            allocated_nodes: vec![NodeId(socket), NodeId(101), NodeId(102)],
            ..Default::default()
        })
        .with_radius_jewels(vec![jewel]);

    let mods = radius_jewel_grant_modifiers(&build, &data);
    // Only the non-attribute Small node receives the parsed mana grant.
    let count = mods
        .iter()
        .filter(|m| m.name.as_str() == "MaximumMana" && m.value.as_number() == Some(10.0))
        .count();
    assert_eq!(
        count, 1,
        "attribute-choice nodes should not count toward the Small grant tally, got {mods:?}"
    );
}

#[test]
fn unknown_passive_node_is_skipped() {
    // Allocating a node absent from the node table → skipped, no error, life stays at base.
    let data = BuildData::empty();
    let build = Build::new().with_tree(PassiveTreeSpec {
        allocated_nodes: vec![NodeId(99999)],
        ..Default::default()
    });
    let opts = DataOrchestratorOptions {
        base_input: MinimalInput {
            base_life: 100.0,
            ..MinimalInput::default()
        },
        inject_character_base: false,
        ..Default::default()
    };
    let out = calculate_with_data(&build, &data, &opts).expect("calc");
    assert_eq!(out.life, 100.0);
}

#[test]
fn gems_classified_and_do_not_error() {
    // An enabled socket group (one active + one support); classification doesn't error, and these gems carry no mods → life is unchanged.
    let mut skill_gems = HashMap::new();
    skill_gems.insert(
        "ActiveGem".to_string(),
        pobr_data::catalog::SkillGemDef {
            id: "ActiveGem".into(),
            gem_type: Some(0),
            gem_colour: Some(1),
            min_level_req: 1,
            str_pct: 0,
            dex_pct: 0,
            int_pct: 0,
            is_support: false,
            granted_effect_id: None,
            additional_granted_effect_ids: Vec::new(),
        },
    );
    skill_gems.insert(
        "SupportGem".to_string(),
        pobr_data::catalog::SkillGemDef {
            id: "SupportGem".into(),
            gem_type: Some(1),
            gem_colour: Some(1),
            min_level_req: 1,
            str_pct: 0,
            dex_pct: 0,
            int_pct: 0,
            is_support: true,
            granted_effect_id: None,
            additional_granted_effect_ids: Vec::new(),
        },
    );
    let data = BuildData {
        skill_gems,
        ..BuildData::empty()
    };
    let build = Build::new().add_socket_group(
        SocketGroup::new()
            .with_gem("ActiveGem")
            .with_gem("SupportGem"),
    );
    let opts = DataOrchestratorOptions {
        base_input: MinimalInput {
            base_life: 100.0,
            ..MinimalInput::default()
        },
        inject_character_base: false,
        ..Default::default()
    };
    let out = calculate_with_data(&build, &data, &opts).expect("calc");
    assert_eq!(out.life, 100.0);
}

#[test]
fn mode_effective_changes_hit_chance_vs_panel() {
    // (Semantics update) Non-attacks always hit: a build with no main skill (so
    // non-attack) skips the accuracy/evasion check, and both hit_chance readings
    // are 1 (vendor CalcOffence.lua:2611-2612
    // `if not isAttack then output.AccuracyHitChance = 100`).
    let data = BuildData::empty();
    let build = Build::new();
    let base = MinimalInput {
        base_accuracy: 1000.0,
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
        ..MinimalInput::default()
    };

    for mode_effective in [false, true] {
        let out = calculate_with_data(
            &build,
            &data,
            &DataOrchestratorOptions {
                base_input: base,
                inject_character_base: false,
                mode_effective,
                enemy_level: 80,
                enemy_tier: EnemyTier::Pinnacle,
                ..Default::default()
            },
        )
        .expect("calc");
        assert_eq!(
            out.hit_chance, 1.0,
            "non-attacks always hit (vendor :2611): mode_effective={mode_effective}"
        );
    }

    // Attack context (CalcConfig::attack() sets SkillTypes::ATTACK): enemy
    // evasion enters the accuracy formula → hit_chance < 1 (PoE2 formula
    // acc*1.25/(acc+eva*0.3), CalcDefence.lua:32-38).
    let mut session =
        CalculationSession::new(base).with_config(CalcConfig::attack().with_mode_effective(true));
    session.setup_enemy(80, EnemyTier::Pinnacle);
    let out = session.perform_minimal().expect("perform");
    assert!(
        out.hit_chance < 1.0,
        "attacks should run the accuracy/evasion check: hit_chance={}",
        out.hit_chance
    );
}

/// (Regression pin) main-skill type drives the accuracy check: a Spell main
/// skill always hits (hit_chance=1, vendor CalcOffence.lua:2611-2612), an
/// Attack main skill does the accuracy/evasion check (<1). Pins the fix for
/// "orchestration failed to fill cfg.skill_types → spells got pulled into the accuracy formula".
#[test]
fn spell_main_skill_skips_accuracy_check_attack_does_not() {
    let data = repo_data();
    let base = MinimalInput {
        base_accuracy: 1000.0,
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
        ..MinimalInput::default()
    };
    let run = |skill: &str| {
        let build = Build::new()
            .add_socket_group(SocketGroup::new().with_gem_skill(skill, 10))
            .with_main_socket_group(1);
        calculate_with_data(
            &build,
            &data,
            &DataOrchestratorOptions {
                base_input: base,
                inject_character_base: false,
                mode_effective: true,
                enemy_level: 80,
                enemy_tier: EnemyTier::Pinnacle,
                ..Default::default()
            },
        )
        .expect("calc")
    };
    // FireballPlayer: a projectile spell; ArmourBreakerPlayer: a melee attack (both real data).
    assert_eq!(
        run("FireballPlayer").hit_chance,
        1.0,
        "spells always hit (vendor :2611)"
    );
    assert!(
        run("ArmourBreakerPlayer").hit_chance < 1.0,
        "attacks should run the accuracy/evasion check"
    );
}

/// Attribute-derived stats consume the **final** attribute value (PoB2
/// CalcPerform.lua:381-388 `round(calcLib.val(modDB, stat))` + :424-431 Life
/// from Str×2): `N% increased Strength` must scale the full BASE — including
/// the class starting value — before it feeds into derivation.
#[test]
fn attribute_increased_modifiers_scale_derived_life() {
    let data = repo_data();
    let character = CharacterIdentity {
        level: 1,
        class_name: "Warrior".into(),
        ascendancy_name: String::new(),
    };
    let run = |texts: Vec<String>| {
        let build = Build::new().with_character(character.clone());
        calculate_with_data(
            &build,
            &data,
            &DataOrchestratorOptions {
                extra_modifier_texts: texts,
                ..Default::default()
            },
        )
        .expect("calc")
    };

    let base = run(vec!["+100 to Strength".into()]);
    let inc = run(vec![
        "+100 to Strength".into(),
        "50% increased Strength".into(),
    ]);

    let cls_str = f64::from(
        data.class_attributes("Warrior")
            .expect("warrior attrs")
            .strength,
    );
    // Δlife = life_per_strength × (round((cls+100)×1.5) − (cls+100)).
    let expected = 2.0 * (((cls_str + 100.0) * 1.5).round() - (cls_str + 100.0));
    assert_eq!(inc.life - base.life, expected);
}

#[test]
fn setup_enemy_session_method_is_exposed() {
    // setup_enemy is exposed via the session and usable standalone (minimal smoke test for the attribution path).
    let mut session = CalculationSession::new(MinimalInput {
        base_accuracy: 1000.0,
        base_hit_min: 50.0,
        base_hit_max: 50.0,
        base_action_rate: 1.0,
        ..MinimalInput::default()
    })
    .with_config(CalcConfig::attack().with_mode_effective(true));
    session.setup_enemy(80, EnemyTier::Pinnacle);
    let out = session.perform_minimal().expect("perform");
    assert!(out.hit_chance <= 1.0);
}

#[test]
fn resistance_penalty_follows_campaign_progress() {
    // resistancePenalty wiring (19-G5): unconfigured → PoB2 defaults to Endgame
    // (-60); explicit Act1 → 0 penalty, all three elemental resistances 60 points
    // higher (chaos has no penalty, so it's unaffected either way).
    let data = repo_data();
    let character = CharacterIdentity {
        level: 90,
        class_name: "Ranger".into(),
        ascendancy_name: String::new(),
    };
    let opts = DataOrchestratorOptions::default();

    let build = Build::new().with_character(character.clone());
    let endgame = calculate_with_data(&build, &data, &opts).expect("endgame calc");

    let mut act1_build = Build::new().with_character(character);
    act1_build.config.campaign_progress = Some(CampaignProgress::Act1);
    let act1 = calculate_with_data(&act1_build, &data, &opts).expect("act1 calc");

    assert_eq!(act1.fire_resistance - endgame.fire_resistance, 60.0);
    assert_eq!(act1.cold_resistance - endgame.cold_resistance, 60.0);
    assert_eq!(
        act1.lightning_resistance - endgame.lightning_resistance,
        60.0
    );
}

#[test]
fn xml_enemy_tier_overrides_orchestrator_option() {
    // enemyIsBoss wiring (19-G3): an explicit None tier in the build XML config
    // should override the caller-supplied Pinnacle — a normal monster's
    // dps_mult (1/4.4) is far below Pinnacle's (8/4.4), so the EHP pipeline's
    // total incoming hit damage should be lower. (A build with no main skill is
    // non-attack and always hits, so hit_chance no longer distinguishes
    // tiers; physical damage reduction hits the same DR cap at both tiers, so
    // incoming enemy damage is used as the observation point instead.)
    let data = BuildData::empty();
    let base = MinimalInput {
        base_accuracy: 1000.0,
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
        ..MinimalInput::default()
    };
    let opts = DataOrchestratorOptions {
        base_input: base,
        inject_character_base: false,
        mode_effective: true,
        enemy_level: 80,
        enemy_tier: EnemyTier::Pinnacle,
        ..Default::default()
    };

    // XML omits enemyIsBoss → falls back to the option's Pinnacle.
    let pinnacle_build = Build::new();
    let pinnacle = calculate_with_data(&pinnacle_build, &data, &opts).expect("pinnacle calc");

    // XML explicitly sets enemyIsBoss=None → overrides the option's tier.
    let mut none_build = Build::new();
    none_build.config.enemy_tier = Some(EnemyTier::None);
    let none = calculate_with_data(&none_build, &data, &opts).expect("none-tier calc");

    assert!(
        pinnacle.total_enemy_damage_in > none.total_enemy_damage_in,
        "Pinnacle-tier enemy damage (dps_mult 8/4.4) should exceed normal-tier (1/4.4): none={} pinnacle={}",
        none.total_enemy_damage_in,
        pinnacle.total_enemy_damage_in,
    );
}

#[test]
fn full_repo_data_end_to_end_smoke() {
    // End-to-end run with real repo data: a class + one item + a real node, must not panic and must produce a finite value.
    let data = repo_data();
    // Pick a real Normal node that has stats.
    let (skill, _) = data
        .passive_nodes
        .iter()
        .find(|(_, n)| n.kind == pobr_data::catalog::PassiveNodeKind::Normal && !n.stats.is_empty())
        .expect("a normal node with stats exists");
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Ranger".into(),
            ascendancy_name: String::new(),
        })
        .set_item(EquipmentSlot::Ring1, life_item("80"))
        .with_tree(PassiveTreeSpec {
            allocated_nodes: vec![NodeId(*skill)],
            ..Default::default()
        });
    let opts = DataOrchestratorOptions {
        base_input: MinimalInput {
            base_life: 50.0,
            ..MinimalInput::default()
        },
        inject_character_base: true,
        mode_effective: true,
        enemy_level: 80,
        enemy_tier: EnemyTier::Pinnacle,
        ..Default::default()
    };
    let out = calculate_with_data(&build, &data, &opts).expect("end-to-end calc");
    // CharacterBase (level 90 Ranger: 28 + 1080 + 2*7=14 = 1122) + ring 80 ≥ contribution from the equipment.
    assert!(out.life >= 1122.0 + 80.0, "life={}", out.life);
    assert!(out.life.is_finite());
}

#[test]
fn mark_gem_injects_offensive_gain_as_buff() {
    // Data-driven: an enabled Freezing Mark (grants the player a 30%
    // gain-as-cold buff on freeze hits) should produce one DamageGainAsCold
    // BASE=30 modifier; a build without the Mark produces none. Never
    // hardcoded against the gem's name.
    let data = repo_data();
    // Precondition: Freezing Mark is not an aura (it's a Mark/Buff, no Aura tag), and its stat-set contains the target buff stat.
    assert!(
        !data.is_aura("FreezingMarkPlayer"),
        "Freezing Mark is not an aura (it's a Mark/Buff)"
    );

    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Ranger".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_gem_skill("FreezingMarkPlayer", 20));

    let mods = self_buff_offensive_modifiers(&build, &data);
    let cold: f64 = mods
        .iter()
        .filter(|m| m.name.as_str() == "DamageGainAsCold")
        .filter_map(|m| m.value.as_number())
        .sum();
    assert_eq!(
        cold, 30.0,
        "Freezing Mark should give 30% gain-as-cold, got {cold}"
    );

    // A build without a Mark produces no offensive self-buff.
    let bare = Build::new().with_character(CharacterIdentity {
        level: 90,
        class_name: "Ranger".into(),
        ascendancy_name: String::new(),
    });
    assert!(
        self_buff_offensive_modifiers(&bare, &data).is_empty(),
        "a build without a Mark should not produce a gain-as buff"
    );
}

/// T1.7: the main skill's quality tier is injected via the stat-map, with
/// trunc truncation + `SourceKind::GemQuality` attribution (id prefix
/// `gem.<effect id>.q<Q>`). Uses a synthetic quality entry (damage_+% is
/// mappable), so it doesn't depend on whether any real gem's quality stat is already mapped.
#[test]
fn main_skill_quality_modifiers_truncate_and_attribute_gem_quality() {
    use pobr_data::catalog::QualityStat;
    let mut data = repo_data();
    data.gem_quality_stats.insert(
        "FireballPlayer".into(),
        vec![QualityStat {
            stat: "damage_+%".into(),
            per_quality_rate: 0.55,
            alt: false,
        }],
    );
    // q19: trunc(0.55 × 19) = trunc(10.45) = 10 (math.modf semantics, not round).
    let group = SocketGroup::new().with_gem_skill_quality("FireballPlayer", 20, 19);
    let mods =
        main_skill_quality_modifiers(&mut test_context(&data), &group, &data, "FireballPlayer");
    assert_eq!(mods.len(), 1, "damage_+% should map to a single Damage INC");
    let m = &mods[0];
    assert_eq!(m.name.as_str(), "Damage");
    assert_eq!(m.mod_type, ModType::Inc);
    assert_eq!(m.value.as_number(), Some(10.0), "trunc(0.55×19)=10");
    let origin = m.origin.as_ref().expect("has attribution");
    assert_eq!(origin.source_id.kind, SourceKind::GemQuality);
    assert!(
        origin.source_id.id.starts_with("gem.FireballPlayer.q19"),
        "attribution id prefix gem.<id>.q<Q>, got {}",
        origin.source_id.id
    );

    // Quality 0: no quality modifier produced.
    let group0 = SocketGroup::new().with_gem_skill("FireballPlayer", 20);
    assert!(
        main_skill_quality_modifiers(&mut test_context(&data), &group0, &data, "FireballPlayer")
            .is_empty()
    );
}

/// The per-set override key of the selected statSet threads through to the
/// engine's set_key — with statSetIndex=2, the same stat routes to set "2"'s
/// override entry (a synthetic catalog); by default it routes to set "1"/global.
#[test]
fn selected_set_key_threads_per_set_override() {
    use pobr_core::rules::stat_map_engine::StatMapCatalog;
    let mut data = repo_data();
    // A synthetic multi-set effect: primary set at vendor index 1, an additional set at index 2 (same stat).
    data.skill_stat_sets.insert(
        "SynthEff".to_string(),
        pobr_data::catalog::SkillStatSetDef {
            effect_id: "SynthEff".into(),
            sets: vec![
                synth_stat_set("SynthMain", Some(1)),
                synth_stat_set("SynthAlt", Some(2)),
            ],
        },
    );
    // Synthetic catalog: global → Damage INC; per-set "2" override → ColdDamage INC.
    let catalog: StatMapCatalog = StatMapCatalog::new(
        serde_json::from_str(
            r#"{
                  "global": { "synth_stat_+%": { "mods": [
                      { "kind": "mod", "name": "Damage", "mod_type": "INC" } ] } },
                  "per_stat_set": { "SynthEff": { "2": { "synth_stat_+%": { "mods": [
                      { "kind": "mod", "name": "ColdDamage", "mod_type": "INC" } ] } } } }
                }"#,
        )
        .expect("synthetic statmap is valid"),
    );
    data.stat_map_catalog = Some(std::sync::Arc::new(catalog));
    let skill = ResolvedSkillLevel {
        base_damage: vec![pobr_data::catalog::SkillDamageStat {
            stat: "synth_stat_+%".into(),
            value: 25.0,
        }],
        damage_multiplier: 1.0,
        ..Default::default()
    };
    // statSetIndex=2 → the per-set override hits (ColdDamage).
    let set_key = data.selected_set_key("SynthEff", Some(2));
    assert_eq!(set_key.as_deref(), Some("2"));
    let mods = skill_base_modifiers(
        &mut test_context(&data),
        &skill,
        "SynthEff",
        set_key.as_deref(),
    );
    let mapped: Vec<&str> = mods
        .iter()
        .filter(|m| m.mod_type == ModType::Inc)
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(mapped, vec!["ColdDamage"], "set 2 override should hit");
    // Default (primary set, key "1", no override) → falls back to global (Damage).
    let set_key = data.selected_set_key("SynthEff", None);
    assert_eq!(set_key.as_deref(), Some("1"));
    let mods = skill_base_modifiers(
        &mut test_context(&data),
        &skill,
        "SynthEff",
        set_key.as_deref(),
    );
    let mapped: Vec<&str> = mods
        .iter()
        .filter(|m| m.mod_type == ModType::Inc)
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(mapped, vec!["Damage"], "default should fall back to global");
}

/// A synthetic stat set (shared across tests): a single level row of `synth_stat_+%`.
fn synth_stat_set(set_id: &str, vendor_idx: Option<u32>) -> pobr_data::catalog::StatSetDef {
    pobr_data::catalog::StatSetDef {
        set_id: set_id.into(),
        label: None,
        vendor_set_index: vendor_idx,
        base_effectiveness: 0.0,
        constant_stats: Vec::new(),
        skill_attack_speed_more: None,
        dot_flags: Default::default(),
        explode_corpse: false,
        implicit_stats: Vec::new(),
        levels: vec![pobr_data::catalog::SkillStatSetLevel {
            gem_level: 1,
            damage_multiplier: 1.0,
            stats: vec![pobr_data::catalog::SkillDamageStat {
                stat: "synth_stat_+%".into(),
                value: 25.0,
            }],
        }],
    }
}

/// Global-only merge for the main skill's unselected statSet — the full path
/// is reachable with real data (FlameWall as the multi-set carrier); gating on
/// the GlobalEffect tag is a translation boundary (see the switchover log
/// §5), so current injection is always zero (the structure is in place but
/// doesn't compute wrong values). Non-global stats are never injected from an unselected set.
#[test]
fn unselected_set_global_only_zero_injection_before_m3() {
    let data = repo_data();
    // Precondition: FlameWall really is multi-set (vendor export has ≥2), and its unselected-set snapshot is non-empty.
    let unsel = data.unselected_set_stats("FlameWallPlayer", 20, 0, None);
    assert!(
        !unsel.is_empty(),
        "FlameWallPlayer should have an unselected set (set 2 = projectile buff form)"
    );
    let group = SocketGroup::new().with_gem_skill("FlameWallPlayer", 20);
    let mods =
        unselected_set_global_modifiers(&mut test_context(&data), &group, &data, "FlameWallPlayer");
    assert!(
        mods.is_empty(),
        "before M3 wires up the GlobalEffect tag, unselected-set injection should be zero, got {mods:?}"
    );
    // Builder path (no gem_skills): no statSet context → empty.
    let empty_group = SocketGroup::new();
    assert!(
        unselected_set_global_modifiers(
            &mut test_context(&data),
            &empty_group,
            &data,
            "FlameWallPlayer"
        )
        .is_empty()
    );
}

#[test]
fn aura_gem_injects_defensive_buff() {
    // Data-driven: an enabled Discipline (ES aura) + Purity of Fire (fire
    // resist aura) should each raise EnergyShield / FireResist respectively; a
    // build without auras (no stats) is unaffected. Never hardcoded against gem names.
    let data = repo_data();
    // Precondition check: both are actually auras (skill_types contains Aura), and their per-level stats are non-empty (data is present).
    assert!(
        data.is_aura("DisciplinePlayer"),
        "Discipline should be recognized as an aura"
    );
    assert!(
        data.is_aura("PurityOfFirePlayer"),
        "Purity of Fire should be recognized as an aura"
    );

    let base_build = Build::new().with_character(CharacterIdentity {
        level: 90,
        class_name: "Witch".into(),
        ascendancy_name: String::new(),
    });
    let aura_build = base_build.clone().add_socket_group(
        SocketGroup::new()
            .with_gem_skill("DisciplinePlayer", 20)
            .with_gem_skill("PurityOfFirePlayer", 20),
    );

    let opts = DataOrchestratorOptions {
        inject_character_base: true,
        ..Default::default()
    };
    let base = calculate_with_data(&base_build, &data, &opts).expect("base calc");
    let aura = calculate_with_data(&aura_build, &data, &opts).expect("aura calc");

    assert!(
        aura.energy_shield > base.energy_shield,
        "Discipline should raise ES: base={} aura={}",
        base.energy_shield,
        aura.energy_shield,
    );
    assert!(
        aura.fire_resistance > base.fire_resistance,
        "Purity of Fire should raise fire resistance: base={} aura={}",
        base.fire_resistance,
        aura.fire_resistance,
    );
    // A non-fire-resist aura shouldn't leak into cold/lightning resist (Purity of Fire only grants fire resist).
    assert_eq!(aura.cold_resistance, base.cold_resistance);
    assert_eq!(aura.lightning_resistance, base.lightning_resistance);
}

// BuffSpec extraction (aura/curse classification + double-count guard)

/// Aura/curse skill → BuffSpec classification: an `Aura` token → Aura kind
/// (mods = defensive buffs at the same level as aura_buff_modifiers);
/// `Mark`/`AppliesCurse` token → Curse kind (is_mark follows the Mark token);
/// slot/socket_index pass through as-is. Precision II support in a Persistent
/// Buff host group → BuffSpec(kind=Buff, Accuracy INC 50,
/// sup_dex.lua:4216-4250 constantStats); an incompatible host
/// (require_skill_types=Persistent+Buff+AND four-way check rejects it, e.g.
/// Fireball) → not injected.
#[test]
fn support_buff_specs_maps_precision_accuracy_inc() {
    let data = repo_data();
    let host = |skill: &str| {
        Build::new().add_socket_group(
            SocketGroup::new()
                .with_gem_skill(skill, 20)
                .with_gem_skill("SupportPrecisionPlayerTwo", 1),
        )
    };

    let specs = support_buff_specs(&mut test_context(&data), &host("HeraldOfAshPlayer"), &data);
    assert_eq!(
        specs.len(),
        1,
        "Persistent Buff host: injects one support buff"
    );
    let spec = &specs[0];
    assert_eq!(spec.kind, BuffKind::Buff);
    assert_eq!(spec.skill_id, "SupportPrecisionPlayerTwo");
    assert_eq!(spec.mods.len(), 1);
    let m = &spec.mods[0];
    assert_eq!(m.name.as_str(), "Accuracy");
    assert_eq!(m.mod_type, ModType::Inc);
    assert_eq!(m.value.as_number(), Some(50.0));

    assert!(
        support_buff_specs(&mut test_context(&data), &host("FireballPlayer"), &data).is_empty(),
        "non-Persistent-Buff host: require check rejects it, nothing injected"
    );
}

/// Extended detection for exposure hosts outside the main group: when the
/// host itself carries no exposure debuff payload but the exposure ability
/// comes from a support (Fire Exposure's
/// `inflict_exposure_for_x_ms_on_ignite` → `flag("InflictExposure",
/// on-Ignited)`, vendor SkillStatMap.lua:1701-1703), Potent Exposure's
/// `<El>ExposureEffect` in the same group is still injected globally (vendor
/// CalcPerform.lua:3196-3200 gates the exposure-source config on
/// `HasMod(FLAG, "InflictExposure")`; oracle sorceress-stormweaver-comet: skillInc=20).
#[test]
fn exposure_support_modifiers_detects_support_granted_inflict() {
    let data = repo_data();
    let aux = SocketGroup::new()
        .with_gem_skill("ElementalStormPlayer", 20)
        .with_gem_skill("SupportFireExposurePlayer", 1)
        .with_gem_skill("SupportPotentExposurePlayer", 1);
    let build = Build::new().add_socket_group(aux);
    let mods = exposure_support_modifiers(&mut test_context(&data), &build, &data, None);
    let names: Vec<&str> = mods.iter().map(|m| m.name.as_str()).collect();
    for el in ["Fire", "Cold", "Lightning"] {
        let name = format!("{el}ExposureEffect");
        let m = mods
            .iter()
            .find(|m| m.name.as_str() == name)
            .unwrap_or_else(|| panic!("{name} should be injected globally (got {names:?})"));
        assert_eq!(m.mod_type, ModType::Inc);
        assert_eq!(m.value.as_number(), Some(20.0), "Potent Exposure lv1 = 20");
    }
    // A group with no exposure ability (pure casting) gets no injection.
    let plain = Build::new().add_socket_group(
        SocketGroup::new()
            .with_gem_skill("SparkPlayer", 20)
            .with_gem_skill("SupportPotentExposurePlayer", 1),
    );
    assert!(
        exposure_support_modifiers(&mut test_context(&data), &plain, &data, None).is_empty(),
        "host with no exposure source: the Potent effect mod does not leak globally"
    );
}

/// The statmap buff-domain supplementary channel for Aura-kind buff skills:
/// War Banner's `base_skill_buff_banner_accuracy_+%_to_apply` (GlobalEffect
/// Aura + Condition BannerPlanted) → spec.mods carries Accuracy INC (the
/// condition tag is preserved as a literal translation), value = the raw
/// statset value at that gem level (verified independently of the mapping data).
#[test]
fn buff_skill_specs_maps_banner_accuracy_from_statmap() {
    let data = repo_data();
    let build =
        Build::new().add_socket_group(SocketGroup::new().with_gem_skill("WarBannerPlayer", 10));

    let specs = buff_skill_specs(&mut test_context(&data), &build, &data);
    let banner = specs
        .iter()
        .find(|s| s.skill_id == "WarBannerPlayer")
        .expect("War Banner spec (Aura kind)");
    assert_eq!(banner.kind, BuffKind::Aura);

    let expected: f64 = data
        .effect_stats("WarBannerPlayer", 10, 0, None)
        .all()
        .into_iter()
        .find(|ds| ds.stat == "base_skill_buff_banner_accuracy_+%_to_apply")
        .map(|ds| ds.value)
        .expect("banner accuracy stat should be in the statset data");
    let acc = banner
        .mods
        .iter()
        .find(|m| m.name.as_str() == "Accuracy")
        .expect("Accuracy INC should reach spec.mods via the statmap buff domain");
    assert_eq!(acc.mod_type, ModType::Inc);
    assert_eq!(acc.value.as_number(), Some(expected));
    assert!(
        acc.tags
            .contains(&pobr_core::ModTag::condition("BannerPlanted", false)),
        "Condition:BannerPlanted is preserved as a literal translation, got {:?}",
        acc.tags
    );
}

/// Pinnacle of Power (granted by the Adonia's Ego weapon, other.lua:12503, a
/// fromItem buff skill) → BuffSpec(kind=Buff): the statmap buff-domain flag
/// channel produces six `<El>Can<Ailment>` FLAGs (GlobalEffect/Buff payload);
/// the entry's leading scalar `Damage MORE` is unrelated and not swept in
/// (each element handled independently, zero numeric injection).
/// This is the stormweaver-comet IgniteDPS cross-type gateway.
#[test]
fn buff_skill_specs_emits_buff_kind_for_pinnacle_of_power_flags() {
    let data = repo_data();
    let build = Build::new()
        .add_socket_group(SocketGroup::new().with_gem_skill("PinnacleOfPowerPlayer", 20));

    let specs = buff_skill_specs(&mut test_context(&data), &build, &data);
    let pinnacle = specs
        .iter()
        .find(|s| s.skill_id == "PinnacleOfPowerPlayer")
        .expect("Pinnacle of Power spec (Buff kind)");
    assert_eq!(pinnacle.kind, BuffKind::Buff);

    let flags: Vec<&str> = pinnacle
        .mods
        .iter()
        .filter(|m| m.mod_type == ModType::Flag)
        .map(|m| m.name.as_str())
        .collect();
    for expected in [
        "ColdCanIgnite",
        "ColdCanShock",
        "FireCanFreeze",
        "FireCanShock",
        "LightningCanFreeze",
        "LightningCanIgnite",
    ] {
        assert!(
            flags.contains(&expected),
            "missing {expected} flag, got {flags:?}"
        );
    }
}

/// Quiver-bonus effect (vendor `EffectOfBonusesFromQuiver`, ModParser.lua:4866;
/// consumed per CalcSetup.lua:1366-1373's Weapon 2 quiver special case): a
/// tree node's "N% increased bonuses gained from Equipped Quiver" → Weapon2
/// slot scale; not collected when the off-hand isn't a quiver.
#[test]
fn slot_bonus_effect_scales_covers_equipped_quiver() {
    use pobr_data::passive_tree::{NodeId, PassiveTreeSpec};
    let quiver_node = pobr_data::catalog::PassiveNodeDef {
        apply_to_armour: false,
        skill: 30341,
        id: "bow_quiver_effect".into(),
        name: Some("Master Fletching".into()),
        kind: pobr_data::catalog::PassiveNodeKind::Notable,
        stats: vec!["20% increased bonuses gained from Equipped [Quiver]".into()],
        group: None,
        orbit: None,
        orbit_index: None,
        x: None,
        y: None,
        connections: vec![],
        ascendancy_id: None,
        unlock_constraint: None,
        variants: vec![],
    };
    let mut passive_nodes = HashMap::new();
    passive_nodes.insert(30341u32, quiver_node);
    let mut base_items = HashMap::new();
    base_items.insert(
        "Visceral Quiver".to_string(),
        weapon_base_item("Visceral Quiver", "Quiver"),
    );
    let data = BuildData {
        passive_nodes,
        base_items,
        ..BuildData::empty()
    };
    let quiver = Item {
        base: ItemBaseId::from("Visceral Quiver"),
        rarity: ItemRarity::Rare,
        quality: 0,
        corrupted: false,
        implicit_texts: vec![],
        modifier_texts: vec!["53% increased Damage with Bow Skills".into()],
        enchant_texts: vec![],
        rolled_defence: RolledDefence::default(),
        parsed_stats: vec![],
    };
    let tree = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(30341)],
        ..Default::default()
    };
    let with_quiver = Build::new()
        .with_tree(tree.clone())
        .set_item(EquipmentSlot::Weapon2, quiver);
    let scales = slot_bonus_effect_scales(&with_quiver, &data);
    assert_eq!(
        scales,
        vec![(EquipmentSlot::Weapon2, 0.2)],
        "a quiver in the off-hand → Weapon2 slot scales by 0.20"
    );

    let without_quiver = Build::new().with_tree(tree);
    assert!(
        slot_bonus_effect_scales(&without_quiver, &data).is_empty(),
        "not collected when the off-hand isn't a quiver (vendor type == \"Quiver\" gate)"
    );
}

/// Collecting the names of active heralds (vendor CalcPerform.lua:1792-1805
/// heraldList + buff-branch naming `gsub(" ","")` — the connector "of" stays
/// lowercase, matching the oracle's condVars form `AffectedByHeraldofPlague`).
/// Deduplicated by name; supports/non-heralds don't count.
#[test]
fn herald_skill_names_collects_and_normalizes_of() {
    let data = repo_data();
    let build = Build::new().add_socket_group(
        SocketGroup::new()
            .with_gem_skill("HeraldOfPlaguePlayer", 10)
            .with_gem_skill("HeraldOfIcePlayer", 10)
            .with_gem_skill("FireballPlayer", 10),
    );
    let names = herald_skill_names(&build, &data);
    assert_eq!(
        names,
        vec!["Herald of Ice".to_string(), "Herald of Plague".to_string()],
        "deduplicated + lowercase 'of' (concatenated AffectedBy = AffectedByHeraldofIce/Plague)"
    );
    assert!(herald_skill_names(&Build::new(), &data).is_empty());
}

#[test]
fn buff_skill_specs_classifies_aura_and_curse() {
    let data = repo_data();
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("Body Armour")
                .with_gem_skill("DisciplinePlayer", 20)
                .with_gem_skill("TemporalChainsPlayer", 20)
                .with_gem_skill("FreezingMarkPlayer", 20),
        );

    let specs = buff_skill_specs(&mut test_context(&data), &build, &data);
    assert_eq!(specs.len(), 3, "one spec each for aura + hex + mark");

    let aura = specs
        .iter()
        .find(|s| s.skill_id == "DisciplinePlayer")
        .expect("Discipline spec");
    assert_eq!(aura.kind, BuffKind::Aura);
    assert_eq!(aura.name, "Discipline");
    assert_eq!(aura.slot.as_deref(), Some("Body Armour"));
    assert_eq!(aura.socket_index, 1, "in-group gem order is 1-based");
    assert!(!aura.is_mark);
    // mods value convention: the per-level buff stat (verified independently —
    // the raw ES apply-stat value from effect_stats, not routed back through
    // the mapping function to self-validate).
    let expected_es: f64 = data
        .effect_stats("DisciplinePlayer", 20, 0, None)
        .all()
        .filter(|ds| ds.stat == "base_skill_buff_total_maximum_energy_shield_+_to_apply")
        .map(|ds| ds.value)
        .sum();
    let spec_es: f64 = aura
        .mods
        .iter()
        .filter(|m| m.name.as_str() == "EnergyShieldTotal")
        .filter_map(|m| m.value.as_number())
        .sum();
    assert!(spec_es > 0.0, "Discipline should carry an ES buff mod");
    assert_eq!(
        spec_es, expected_es,
        "BuffSpec mods = raw per-level buff stat value"
    );

    let hex = specs
        .iter()
        .find(|s| s.skill_id == "TemporalChainsPlayer")
        .expect("Temporal Chains spec");
    assert_eq!(hex.kind, BuffKind::Curse);
    assert!(!hex.is_mark, "AppliesCurse (not Mark) → hex");
    assert_eq!(
        hex.name, "Temporal Chains",
        "name derived from active_skill's snake_case (curse_base lookup key)"
    );
    assert_eq!(hex.socket_index, 2);

    let mark = specs
        .iter()
        .find(|s| s.skill_id == "FreezingMarkPlayer")
        .expect("Freezing Mark spec");
    assert_eq!(mark.kind, BuffKind::Curse);
    assert!(mark.is_mark, "Mark token → is_mark");
    assert_eq!(mark.socket_index, 3);

    // Active skills that aren't aura/curse, and supports, produce no spec.
    let bare = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_gem_skill("FireballPlayer", 20));
    assert!(buff_skill_specs(&mut test_context(&data), &bare, &data).is_empty());
}

/// Precondition for vendor curse registration: a curse skill with no
/// GlobalEffect Curse payload in the statMap at all (Repulsion — its per-set
/// statMap is entirely empty, so buffList is always empty,
/// CalcActiveSkill.lua:976-1041) produces no BuffSpec — it doesn't occupy a
/// curse slot and doesn't count toward `Multiplier:CurseOnEnemy`
/// (CalcPerform.lua:2969 `#curseSlots`); a curse with a payload but outside
/// the allow-list (Temporal Chains) still registers (vendor also slots it in).
#[test]
fn buff_skill_specs_skips_curse_without_payload() {
    let data = repo_data();
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_gem_skill("CurseOfRepulsionPlayer", 20)
                .with_gem_skill("TemporalChainsPlayer", 20),
        );

    let specs = buff_skill_specs(&mut test_context(&data), &build, &data);
    assert!(
        data.granted_effects.contains_key("CurseOfRepulsionPlayer"),
        "precondition: the Repulsion effect should be in the data pack (otherwise this test degenerates)"
    );
    assert!(
        !specs.iter().any(|s| s.skill_id == "CurseOfRepulsionPlayer"),
        "Repulsion has no curse payload → not registered (vendor buffList is empty)"
    );
    let hex = specs
        .iter()
        .find(|s| s.skill_id == "TemporalChainsPlayer")
        .expect("Temporal Chains has a payload (counts even outside the allow-list) → registered");
    assert_eq!(hex.kind, BuffKind::Curse);
}

/// Debuff classification: Frost Bomb (an active skill that's neither
/// aura nor curse) has `active_skill_all_elemental_exposure_magnitude`
/// (GlobalEffect Debuff, SkillStatMap.lua:1721-1725) → BuffSpec(kind=Debuff),
/// mods = the three elemental `<El>Exposure BASE 20` (raw statset constants).
/// vendor applies this to the entire activeSkillList (CalcPerform.lua:2219-2285)
/// — a non-main skill group still produces it.
#[test]
fn buff_skill_specs_classifies_frost_bomb_debuff() {
    let data = repo_data();
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Druid".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_gem_skill("FrostBombPlayer", 18));

    let specs = buff_skill_specs(&mut test_context(&data), &build, &data);
    let bomb = specs
        .iter()
        .find(|s| s.skill_id == "FrostBombPlayer")
        .expect("Frost Bomb debuff spec");
    assert_eq!(bomb.kind, BuffKind::Debuff);
    assert!(!bomb.is_mark);
}

/// Single-channel invariant (after the C5-3 legacy-code removal): the
/// orchestrator pipeline's (BuffSpec → buff_pass multiplier zone) aura ES
/// contribution == a manual session using only the buff_pass channel —
/// proving the orchestrator has no leftover second aura-injection path (the
/// old static direct-inject was removed; at mult = 1.0, ScaleAddMod returns
/// the raw value, i.e. the value equals the raw buff stat).
#[test]
fn buff_spec_injection_does_not_double_count_auras() {
    let data = repo_data();
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_gem_skill("DisciplinePlayer", 20)
                .with_gem_skill("TemporalChainsPlayer", 20),
        );
    let opts = DataOrchestratorOptions {
        inject_character_base: true,
        ..Default::default()
    };
    let through_orchestrator =
        calculate_with_data(&build, &data, &opts).expect("orchestrator calc");
    // Manual session: only the BuffSpec → buff_pass channel (same mode_buffs convention as the orchestrator).
    let mut manual = CalculationSession::new(MinimalInput::default())
        .with_config(CalcConfig::attack().with_mode_buffs(true));
    for spec in buff_skill_specs(&mut test_context(&data), &build, &data) {
        manual.add_buff_skill(spec);
    }
    let manual_es = {
        manual.perform_minimal().expect("perform");
        manual.output().energy_shield
    };
    assert!(
        manual_es > 0.0,
        "Discipline should have a non-zero ES contribution via buff_pass"
    );
    assert_eq!(
        through_orchestrator.energy_shield, manual_es,
        "aura mods count only once via the single buff_pass channel (no leftover static direct-inject)"
    );
}

/// New-path end to end: buff_skill_specs → add_buff_skill → buff_pass aura
/// multiplier zone (mode_buffs set — the orchestrator entry point has set it
/// unconditionally since C5-2; here the manual session sets it explicitly).
#[test]
fn buff_spec_aura_path_end_to_end_with_mode_buffs() {
    let data = repo_data();
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_gem_skill("DisciplinePlayer", 20));

    let es_with_aura_effect = |aura_effect_inc: f64| {
        let mut session = CalculationSession::new(MinimalInput::default())
            .with_config(CalcConfig::attack().with_mode_buffs(true));
        if aura_effect_inc != 0.0 {
            session.add_modifiers([Modifier::number(
                "AuraEffect",
                ModType::Inc,
                aura_effect_inc,
            )]);
        }
        for spec in buff_skill_specs(&mut test_context(&data), &build, &data) {
            session.add_buff_skill(spec);
        }
        session.perform_minimal().expect("perform");
        session.output().energy_shield
    };

    let base = es_with_aura_effect(0.0);
    assert!(
        base > 0.0,
        "on the new path, Discipline raises ES via buff_pass"
    );
    let boosted = es_with_aura_effect(20.0);
    assert!(
        boosted > base,
        "20% inc AuraEffect amplifies the aura buff: base={base} boosted={boosted}"
    );
}

// Curse effect stat→mod mapping (the statmap curse domain)

/// A curse spec's mods are filled from the statmap curse domain: Despair →
/// enemy-side `ChaosResist` BASE (a negative resist-reducer, SkillGem
/// attribution); Sniper's Mark → `SelfCritMultiplier` BASE; Temporal Chains
/// (its payload name has no pobr consumer) → empty mods (falls into the
/// Unsupported report, not silently injected).
#[test]
fn buff_skill_specs_fill_curse_mods_from_statmap() {
    let data = repo_data();
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("Body Armour")
                .with_gem_skill("DespairPlayer", 20)
                .with_gem_skill("SnipersMarkPlayer", 20)
                .with_gem_skill("TemporalChainsPlayer", 20),
        );
    let specs = buff_skill_specs(&mut test_context(&data), &build, &data);

    let despair = specs
        .iter()
        .find(|s| s.skill_id == "DespairPlayer")
        .expect("Despair spec");
    // Verified independently: the raw per-level buff stat (not routed back through the mapping function to self-validate).
    let expected_res: f64 = data
        .effect_stats("DespairPlayer", 20, 0, None)
        .all()
        .filter(|ds| ds.stat == "base_skill_buff_chaos_damage_resistance_%_to_apply")
        .map(|ds| ds.value)
        .sum();
    assert!(
        expected_res < 0.0,
        "Despair's resist-reduction stat should be negative"
    );
    let chaos_res: Vec<&Modifier> = despair
        .mods
        .iter()
        .filter(|m| m.name.as_str() == "ChaosResist")
        .collect();
    assert_eq!(
        chaos_res.len(),
        1,
        "Despair → a single enemy-side ChaosResist"
    );
    assert_eq!(chaos_res[0].mod_type, ModType::Base);
    assert_eq!(chaos_res[0].value.as_number(), Some(expected_res));
    let origin = chaos_res[0].origin.as_ref().expect("SkillGem attribution");
    assert_eq!(origin.source_id.kind, SourceKind::SkillGem);
    assert!(origin.source_id.id.starts_with("curse.DespairPlayer."));

    let mark = specs
        .iter()
        .find(|s| s.skill_id == "SnipersMarkPlayer")
        .expect("Sniper's Mark spec");
    assert!(
        mark.mods
            .iter()
            .any(|m| m.name.as_str() == "SelfCritMultiplier" && m.mod_type == ModType::Base),
        "Sniper's Mark → enemy-side SelfCritMultiplier BASE"
    );

    let chains = specs
        .iter()
        .find(|s| s.skill_id == "TemporalChainsPlayer")
        .expect("Temporal Chains spec");
    assert!(
        !chains
            .mods
            .iter()
            .any(|m| m.name.as_str() == "TemporalChainsActionSpeed"),
        "no pobr consumer for the payload name (TemporalChainsActionSpeed) → not injected (falls into the Compare report)"
    );
    // BuffExpireFaster is allow-listed (consumer = ailment::debuff_duration_mult,
    // CalcOffence.lua:1833-1835 / :5040) → a negative enemy-side MORE goes into spec.mods.
    let expire = chains
        .mods
        .iter()
        .find(|m| m.name.as_str() == "BuffExpireFaster")
        .expect("Temporal Chains → enemy-side BuffExpireFaster MORE");
    assert_eq!(expire.mod_type, ModType::More);
    assert!(
        expire.value.as_number().is_some_and(|v| v < 0.0),
        "expire slower = a negative MORE value, got {:?}",
        expire.value
    );
}

/// Visibility, not silence: in Compare mode, every mapped / unsupported stat
/// in a curse payload lands in a [`StatMapCompareRecord`] (label = `curse.<skill_id>`).
#[test]
fn curse_unmapped_stats_land_in_compare_report() {
    let data = repo_data();
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_gem_skill("DespairPlayer", 20)
                .with_gem_skill("TemporalChainsPlayer", 20),
        );
    let mut context = CalculationContext::new(
        &data,
        &DataOrchestratorOptions {
            stat_map_mode: StatMapMode::Compare,
            ..Default::default()
        },
    );
    let _ = buff_skill_specs(&mut context, &build, &data);
    let records = context.compare_records;
    assert!(
        records.iter().any(|r| r.label == "curse.DespairPlayer"
            && r.classification == "mapped"
            && r.detail.contains("ChaosResist")),
        "Despair's successfully mapped row lands in the report: {records:?}"
    );
    assert!(
        records
            .iter()
            .any(|r| r.label == "curse.TemporalChainsPlayer"
                && r.classification == "unsupported"
                && r.detail.contains("unknown_mod_name")),
        "Temporal Chains' unmapped payload reports unknown_mod_name: {records:?}"
    );
}

/// End to end (effective mode): a build with Elemental Weakness lowers the
/// enemy's elemental resistance → fire main-skill DPS rises; a panel-mode
/// anchor (mode_effective=false, vendor :2289's hex gate doesn't pass)
/// verifies every value stays unchanged.
#[test]
fn curse_mods_raise_effective_dps_panel_unchanged() {
    let data = repo_data();
    let base_build = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_gem_skill("FireballPlayer", 20));
    let cursed_build = base_build
        .clone()
        .add_socket_group(SocketGroup::new().with_gem_skill("ElementalWeaknessPlayer", 20));
    let calc = |build: &Build, effective: bool| {
        calculate_with_data(
            build,
            &data,
            &DataOrchestratorOptions {
                inject_character_base: true,
                mode_effective: effective,
                enemy_tier: EnemyTier::Pinnacle,
                ..Default::default()
            },
        )
        .expect("calc")
    };

    // Effective mode: enemy fire resist -59 (EW lv20) enters the enemy db through the CurseEffect multiplier zone → DPS rises.
    let eff_base = calc(&base_build, true);
    let eff_cursed = calc(&cursed_build, true);
    assert!(
        eff_base.dps > 0.0,
        "the fire main-skill baseline DPS should be non-zero"
    );
    assert!(
        eff_cursed.dps > eff_base.dps,
        "Elemental Weakness lowering enemy fire resist should raise effective DPS: base={} cursed={}",
        eff_base.dps,
        eff_cursed.dps,
    );
    assert_eq!(
        eff_cursed.curse_slots,
        vec!["Elemental Weakness".to_string()]
    );

    // Panel-mode anchor: the hex is skipped at :2289's gate
    // (mode_effective=false) → attaching a curse gem leaves every output value unchanged.
    let panel_base = calc(&base_build, false);
    let panel_cursed = calc(&cursed_build, false);
    assert_eq!(
        panel_cursed.dps, panel_base.dps,
        "panel-mode DPS is unchanged value-for-value"
    );
    assert_eq!(panel_cursed.life, panel_base.life);
    assert_eq!(panel_cursed.fire_resistance, panel_base.fire_resistance);
    assert!(
        panel_cursed.curse_slots.is_empty(),
        "in panel mode, hexes don't occupy a slot"
    );
}

/// End to end (effective mode): CurseEffect inc amplifies the mapped result;
/// when limit=1 truncates, the loser (Despair, lower priority by socket order)'s mods have no DPS effect.
#[test]
fn curse_effect_amplifies_and_limit_truncates_end_to_end() {
    let data = repo_data();
    // Main damage: a manually injected chaos hit (affected by enemy
    // ChaosResist); the Despair spec is obtained through buff_skill_specs's real mapping.
    let despair_only = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_gem_skill("DespairPlayer", 20));
    // Despair(socket 1, priority 8+100) vs Enfeeble(socket 2, priority 2+200)
    // → Enfeeble takes the slot, Despair is truncated.
    let both_hexes = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_gem_skill("DespairPlayer", 20)
                .with_gem_skill("EnfeeblePlayer", 20),
        );
    let dps = |build: Option<&Build>, curse_effect_inc: f64| {
        let mut session = CalculationSession::new(MinimalInput {
            base_accuracy: 1_000_000.0,
            base_action_rate: 1.0,
            ..Default::default()
        })
        .with_config(
            CalcConfig::attack()
                .with_mode_buffs(true)
                .with_mode_effective(true),
        );
        if let Some(priority) = data.curse_priority.clone() {
            session.set_curse_priority(priority);
        }
        session.add_modifiers([
            Modifier::number("ChaosDamageMin", ModType::Base, 100.0),
            Modifier::number("ChaosDamageMax", ModType::Base, 100.0),
        ]);
        if curse_effect_inc != 0.0 {
            session.add_modifiers([Modifier::number(
                "CurseEffect",
                ModType::Inc,
                curse_effect_inc,
            )]);
        }
        if let Some(build) = build {
            for spec in buff_skill_specs(&mut test_context(&data), build, &data) {
                session.add_buff_skill(spec);
            }
        }
        session.setup_enemy(80, EnemyTier::Pinnacle);
        session.perform_minimal().expect("perform");
        (session.output().dps, session.output().curse_slots.clone())
    };

    let (dps_bare, slots_bare) = dps(None, 0.0);
    let (dps_despair, slots_despair) = dps(Some(&despair_only), 0.0);
    let (dps_amplified, _) = dps(Some(&despair_only), 20.0);
    let (dps_truncated, slots_truncated) = dps(Some(&both_hexes), 0.0);

    assert!(slots_bare.is_empty());
    assert_eq!(slots_despair, vec!["Despair".to_string()]);
    assert!(
        dps_despair > dps_bare,
        "Despair lowering enemy chaos resist → DPS rises: bare={dps_bare} despair={dps_despair}"
    );
    assert!(
        dps_amplified > dps_despair,
        "20% inc CurseEffect amplifies the resist reduction: despair={dps_despair} amplified={dps_amplified}"
    );
    // limit=1 truncation: Enfeeble (higher priority) takes the slot alone,
    // Despair's mods never enter the enemy db — Enfeeble's payload (enemy
    // Damage MORE) doesn't affect player DPS → equals the bare baseline value for value.
    assert_eq!(slots_truncated, vec!["Enfeeble".to_string()]);
    assert_eq!(
        dps_truncated, dps_bare,
        "the losing Despair mods have no DPS effect (Enfeeble's payload is DPS-neutral)"
    );
}

// mode_combat automatic combat-condition setting

/// combat_conditions checked branch-by-branch against vendor
/// CalcPerform.lua:242-266: attack/spell are mutually exclusive,
/// Movement/Minion/Channel stack, Duration suppresses minion, and exemptions clear everything.
#[test]
fn combat_conditions_follow_vendor_branches() {
    let types = |ts: &[&str]| ts.iter().map(|t| t.to_string()).collect::<Vec<_>>();
    // attack takes priority over spell (vendor elseif).
    assert_eq!(
        combat_conditions(&types(&["Attack"]), ModFlags::ATTACK),
        vec!["AttackedRecently"]
    );
    assert_eq!(
        combat_conditions(&types(&["Spell"]), ModFlags::SPELL),
        vec!["CastSpellRecently"]
    );
    assert_eq!(
        combat_conditions(
            &types(&["Attack", "Spell"]),
            ModFlags::ATTACK | ModFlags::SPELL
        ),
        vec!["AttackedRecently"],
        "attack elseif spell (:249-253 are mutually exclusive)"
    );
    // Movement / Channel stack on top of attack/spell.
    assert_eq!(
        combat_conditions(&types(&["Attack", "Movement"]), ModFlags::ATTACK),
        vec!["AttackedRecently", "UsedMovementSkillRecently"]
    );
    assert_eq!(
        combat_conditions(&types(&["Spell", "Channel"]), ModFlags::SPELL),
        vec!["CastSpellRecently", "Channelling"]
    );
    // minion and not duration (:257-259).
    assert_eq!(
        combat_conditions(&types(&["Spell", "Minion"]), ModFlags::SPELL),
        vec!["CastSpellRecently", "UsedMinionSkillRecently"]
    );
    assert_eq!(
        combat_conditions(&types(&["Spell", "Minion", "Duration"]), ModFlags::SPELL),
        vec!["CastSpellRecently"],
        "Duration suppresses UsedMinionSkillRecently"
    );
    // Exemptions (:248): triggered / mine / totem clear the whole set.
    for exempt in ["Triggered", "InbuiltTrigger", "RemoteMined", "SummonsTotem"] {
        assert!(
            combat_conditions(&types(&["Attack", exempt]), ModFlags::ATTACK).is_empty(),
            "{exempt} should be exempt from combat conditions"
        );
    }
}

/// B4 end to end (existing consumer = Channelling): a Channel main skill
/// (Bonestorm, cast 0.125s) + 5000% cast speed → the rate far exceeds the
/// server tick cap (~30.3/s), but B4 auto-sets Channelling based on
/// SkillType.Channel (vendor :264-266) → channelled skills are exempt from
/// the tick cap (same convention as offence::apply_server_tick_cap / skill_use_time).
/// Contrast against a non-Channel spell (Fireball, cast 1.2s) with the same cast speed, which does get capped.
#[test]
fn channel_main_skill_sets_channelling_condition() {
    let data = repo_data();
    let mk = |skill: &str| {
        Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(SocketGroup::new().with_gem_skill(skill, 20))
            .with_main_socket_group(1)
    };
    let opts = DataOrchestratorOptions {
        inject_character_base: true,
        extra_modifier_texts: vec!["5000% increased Cast Speed".into()],
        ..Default::default()
    };
    let server_cap = 1.0 / 0.033; // ≈ 30.3/s (game_constants server_tick_seconds)

    let channel = calculate_with_data(&mk("BonestormPlayer"), &data, &opts).expect("calc");
    let channel_sut = channel.skill_use_time.expect("skill_use_time filled");
    assert!(
        !channel_sut.capped_by_server_tick && channel.effective_action_rate > server_cap,
        "a Channel main skill should auto-set Channelling (exempt from the tick cap): rate={} capped={}",
        channel.effective_action_rate,
        channel_sut.capped_by_server_tick
    );

    let spell = calculate_with_data(&mk("FireballPlayer"), &data, &opts).expect("calc");
    let spell_sut = spell.skill_use_time.expect("skill_use_time filled");
    assert!(
        spell_sut.capped_by_server_tick && spell.effective_action_rate <= server_cap + 1e-9,
        "a non-Channel spell doesn't set Channelling (the tick cap applies): rate={} capped={}",
        spell.effective_action_rate,
        spell_sut.capped_by_server_tick
    );
}

// Build-layer wiring for the trigger chain (findings 03-01/03-02/03-06)

/// A built-in triggered main skill (`ElementalStormPlayer`: Spell/Damage, cd
/// 3s, Triggered/InbuiltTrigger) → the orchestrator injects the trigger
/// cooldown → perform's fill_trigger writes a non-placeholder
/// trigger_rate_cap / skill_trigger_rate (cd 3s → cap ≈ 1/3.003 ≈ 0.333/s).
#[test]
fn inbuilt_trigger_skill_fills_trigger_rate_cap() {
    let data = repo_data();
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 80,
            class_name: "Sorceress".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_gem_skill("ElementalStormPlayer", 20))
        .with_main_socket_group(1);

    let opts = DataOrchestratorOptions {
        inject_character_base: true,
        ..Default::default()
    };
    let out = calculate_with_data(&build, &data, &opts).expect("trigger calc");

    // cd 3s → cap = 1/ceil_tick(3.0) ≈ 0.333/s.
    assert!(
        out.trigger_rate_cap > 0.0,
        "a built-in trigger should write a non-zero trigger_rate_cap, got {}",
        out.trigger_rate_cap
    );
    assert!(
        (out.trigger_rate_cap - 0.333).abs() < 0.05,
        "a 3s cd trigger cap should be ≈0.333/s, got {}",
        out.trigger_rate_cap
    );
    assert!(
        out.skill_trigger_rate > 0.0,
        "skill_trigger_rate should not be the placeholder 0, got {}",
        out.skill_trigger_rate
    );
}

/// T5.6 meta/composite gem expansion: when none of the group's own gems are
/// damage skills, the gem_effects foreign key is used to pick a damage skill
/// from the additional granted effects as the main skill (PoB2
/// CalcSetup.lua:1714-1718 adds additionalGrantedEffects into socketGroupSkillList too).
#[test]
fn meta_gem_expands_additional_granted_effect_as_main_skill() {
    // This test module has no shared effect constructor, so build one inline.
    let mk_effect = |id: &str, skill_types: &[&str]| pobr_data::catalog::GrantedEffectDef {
        id: id.into(),
        is_support: false,
        active_skill: Some(id.to_string()),
        cast_time: Some(1000),
        require_skill_types: vec![],
        add_skill_types: vec![],
        exclude_skill_types: vec![],
        cannot_be_supported: false,
        support_gems_only: false,
        stat_set: None,
        additional_stat_set_ids: vec![],
        cost_types: vec![],
        minion_list: vec![],
        add_minion_list: vec![],
        minion_uses: vec![],
        minion_has_item_set: false,
        skill_types: skill_types.iter().map(|s| s.to_string()).collect(),
    };
    let mut granted_effects = HashMap::new();
    // Host effect: a summon skill (neither attack nor spell), not a damage-skill candidate on its own.
    granted_effects.insert(
        "SummonShellPlayer".to_string(),
        mk_effect("SummonShellPlayer", &["Totem"]),
    );
    // Additional effect: the actual damage spell.
    granted_effects.insert(
        "ShellQuakePlayer".to_string(),
        mk_effect("ShellQuakePlayer", &["Spell", "Damage"]),
    );
    let mut gem_effects = HashMap::new();
    gem_effects.insert(
        "SummonShellPlayer".to_string(),
        pobr_data::catalog::GemEffectDef {
            gem_id: "Metadata/Items/Gems/SkillGemShell".into(),
            variant_id: "Shell".into(),
            granted_effect_id: "SummonShellPlayer".into(),
            additional_granted_effect_ids: vec!["ShellQuakePlayer".into()],
            additional_stat_set_ids: vec![],
        },
    );
    let data = BuildData {
        granted_effects,
        gem_effects,
        ..BuildData::empty()
    };
    let group = SocketGroup::new().with_gem_skill("SummonShellPlayer", 12);
    let picked = pick_group_main_skill(&data, &group);
    assert_eq!(
        picked,
        Some(("ShellQuakePlayer", 12, None)),
        "an additional granted effect should be expanded into the main skill (level follows the host gem)"
    );

    // Missing foreign key (an old data pack without the overlay) → stays None (a pure summon group has no main skill, backward compatible).
    let data_no_link = BuildData {
        granted_effects: data.granted_effects.clone(),
        ..BuildData::empty()
    };
    assert_eq!(pick_group_main_skill(&data_no_link, &group), None);
}

/// A non-triggered main skill (an ordinary spell) → the orchestrator injects no
/// trigger mods → the trigger panel stays at its placeholder 0 (backward compatible).
#[test]
fn non_trigger_skill_leaves_trigger_panel_zero() {
    let data = repo_data();
    // FireballPlayer: an ordinary projectile spell, not Triggered/InbuiltTrigger.
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 80,
            class_name: "Sorceress".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_gem_skill("FireballPlayer", 20))
        .with_main_socket_group(1);

    let opts = DataOrchestratorOptions::default();
    let out = calculate_with_data(&build, &data, &opts).expect("non-trigger calc");

    assert_eq!(
        out.trigger_rate_cap, 0.0,
        "a non-triggered skill's trigger_rate_cap should stay 0"
    );
    assert_eq!(
        out.skill_trigger_rate, 0.0,
        "a non-triggered skill's skill_trigger_rate should stay 0"
    );
}

/// `trigger_modifiers` unit test: a built-in trigger + a cooldown → injects
/// TriggeredSkillCooldown + TriggerCooldownBase; a non-triggered skill → empty (backward-compat gating).
#[test]
fn trigger_modifiers_gates_on_triggered_skill_type() {
    let mut granted_effects = HashMap::new();
    // A built-in triggered skill (has a cooldown).
    granted_effects.insert(
        "TrigSkill".to_string(),
        pobr_data::catalog::GrantedEffectDef {
            id: "TrigSkill".into(),
            is_support: false,
            active_skill: Some("TrigSkill".into()),
            cast_time: Some(1000),
            require_skill_types: vec![],
            add_skill_types: vec![],
            exclude_skill_types: vec![],
            cannot_be_supported: false,
            support_gems_only: false,
            stat_set: None,
            additional_stat_set_ids: vec![],
            cost_types: vec![],
            minion_list: vec![],
            add_minion_list: vec![],
            minion_uses: vec![],
            minion_has_item_set: false,
            skill_types: vec!["Spell".into(), "Triggered".into(), "InbuiltTrigger".into()],
        },
    );
    // An ordinary (non-triggered) skill.
    granted_effects.insert(
        "NormalSkill".to_string(),
        pobr_data::catalog::GrantedEffectDef {
            id: "NormalSkill".into(),
            is_support: false,
            active_skill: Some("NormalSkill".into()),
            cast_time: Some(1000),
            require_skill_types: vec![],
            add_skill_types: vec![],
            exclude_skill_types: vec![],
            cannot_be_supported: false,
            support_gems_only: false,
            stat_set: None,
            additional_stat_set_ids: vec![],
            cost_types: vec![],
            minion_list: vec![],
            add_minion_list: vec![],
            minion_uses: vec![],
            minion_has_item_set: false,
            skill_types: vec!["Spell".into()],
        },
    );
    let data = BuildData {
        granted_effects,
        ..BuildData::empty()
    };
    let build = Build::new();
    let group = SocketGroup::new();

    // A triggered skill + a cooldown → injects both cooldown BASEs.
    let triggered = ResolvedSkillLevel {
        cooldown_s: Some(0.5),
        ..ResolvedSkillLevel::default()
    };
    let opts = DataOrchestratorOptions::default();
    let mods = trigger_modifiers(
        &mut test_context(&data),
        &build,
        &data,
        &opts,
        &triggered,
        &group,
        "TrigSkill",
    );
    let names: Vec<&str> = mods.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"TriggeredSkillCooldown"));
    assert!(names.contains(&"TriggerCooldownBase"));

    // A non-triggered skill → empty (no trigger mods injected).
    let normal = ResolvedSkillLevel {
        cooldown_s: Some(0.5),
        ..ResolvedSkillLevel::default()
    };
    let mods_none = trigger_modifiers(
        &mut test_context(&data),
        &build,
        &data,
        &opts,
        &normal,
        &group,
        "NormalSkill",
    );
    assert!(
        mods_none.is_empty(),
        "a non-triggered skill should not inject trigger mods"
    );
}

// trigger_configs recognition + source-rate sub-calc

/// CoC fixture (a named gate item): group = [attack, MetaCastOnCritPlayer,
/// spell], main skill = spell. `trigger_configs`'s `match_effect_ids`
/// recognizes the CoC trigger relationship (the trigger panel no longer
/// degrades to self-cast 0), folding in the source hit/crit.
#[test]
fn coc_group_recognized_and_trigger_rate_filled() {
    let data = repo_data();
    assert!(
        data.trigger_configs.contains_key("MetaCastOnCritPlayer"),
        "trigger_configs overlay should include the CoC join key"
    );
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 80,
            class_name: "Sorceress".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_gem_skill("ArmourBreakerPlayer", 10)
                .with_gem_skill("MetaCastOnCritPlayer", 10)
                .with_gem_skill("FireballPlayer", 10)
                .with_main_active_skill(3),
        )
        .with_main_socket_group(1);
    let out =
        calculate_with_data(&build, &data, &DataOrchestratorOptions::default()).expect("coc calc");

    assert!(
        out.skill_trigger_rate > 0.0,
        "after CoC recognition, the trigger rate should not be the placeholder 0, got {}",
        out.skill_trigger_rate
    );
    // Crit folded in (trigger_on_crit): the trigger rate should be noticeably lower than the source's attack rate (source crit chance ≪ 100%).
    let source_stats = trigger_source_stats(
        &mut test_context(&data),
        &build,
        &data,
        &DataOrchestratorOptions::default(),
        &build.socket_groups[0],
        &build.socket_groups[0].gem_skills[0],
        "FireballPlayer",
    )
    .expect("source sub-calc");
    assert!(
        out.skill_trigger_rate < source_stats.action_rate,
        "CoC trigger rate {} should be discounted by the source's crit chance to below source rate {}",
        out.skill_trigger_rate,
        source_stats.action_rate
    );
}

#[test]
fn trigger_subcalc_preserves_outer_exposure_supports() {
    let data = repo_data();
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 80,
            class_name: "Sorceress".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_gem_skill("ArmourBreakerPlayer", 10)
                .with_gem_skill("MetaCastOnCritPlayer", 10)
                .with_gem_skill("FireballPlayer", 10)
                .with_main_active_skill(3),
        )
        .add_socket_group(
            SocketGroup::new()
                .with_gem_skill("ElementalStormPlayer", 20)
                .with_gem_skill("SupportFireExposurePlayer", 1)
                .with_gem_skill("SupportPotentExposurePlayer", 1),
        )
        .with_main_socket_group(1);
    let session = calculate_with_data_session(&build, &data, &DataOrchestratorOptions::default())
        .expect("trigger calculation");
    assert!(session.output().skill_trigger_rate > 0.0);
    for element in ["Fire", "Cold", "Lightning"] {
        assert!(
            session
                .mods_named(&format!("{element}ExposureEffect"))
                .iter()
                .any(|m| { m.mod_type == ModType::Inc && m.value.as_number() == Some(20.0) }),
            "trigger source calculation must preserve the outer exposure support mapping: {element}"
        );
    }
    let compare_options = DataOrchestratorOptions {
        stat_map_mode: StatMapMode::Compare,
        ..Default::default()
    };
    let report = calculate_with_data_report(&build, &data, &compare_options).unwrap();
    assert_eq!(report.session.output(), session.output());
    assert_eq!(
        report
            .stat_map_records
            .iter()
            .filter(|r| {
                r.label == "SupportPotentExposurePlayer" && r.stat == "exposure_effect_+%"
            })
            .count(),
        2,
        "both the trigger source and the outer calculation must record exposure mapping"
    );

    // An explicit empty catalog must override BuildData for all mapping domains,
    // including the outer calculation after its trigger source has completed.
    let override_options = DataOrchestratorOptions {
        stat_map_catalog: Some(std::sync::Arc::new(StatMapCatalog::new(
            serde_json::from_str(r#"{"global":{}}"#).unwrap(),
        ))),
        ..Default::default()
    };
    let overridden = calculate_with_data_report(&build, &data, &override_options).unwrap();
    assert!(overridden.stat_map_records.is_empty());
    assert!(
        overridden
            .session
            .mods_named("FireExposureEffect")
            .is_empty()
    );

    // Keeping an earlier report alive requires no take/clear protocol. A later
    // calculation starts with fresh diagnostics and the original data catalog.
    let repeated = calculate_with_data_report(&build, &data, &compare_options).unwrap();
    assert_eq!(repeated.stat_map_records, report.stat_map_records);
    assert_eq!(repeated.session.output(), report.session.output());
}

/// CoC directional assertion: source skill +100% attack speed → trigger rate
/// (the rate factor feeding DPS) rises in step — a regression guard for
/// 14-G2's "source rate didn't scale with attack speed" bug.
#[test]
fn coc_directional_attack_speed_raises_trigger_rate() {
    let data = repo_data();
    let mk_build = || {
        Build::new()
            .with_character(CharacterIdentity {
                level: 80,
                class_name: "Sorceress".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_gem_skill("ArmourBreakerPlayer", 10)
                    .with_gem_skill("MetaCastOnCritPlayer", 10)
                    .with_gem_skill("FireballPlayer", 10)
                    .with_main_active_skill(3),
            )
            .with_main_socket_group(1)
    };
    let base_out = calculate_with_data(&mk_build(), &data, &DataOrchestratorOptions::default())
        .expect("coc base");
    let fast_opts = DataOrchestratorOptions {
        extra_modifier_texts: vec!["100% increased Attack Speed".to_string()],
        ..Default::default()
    };
    let fast_out = calculate_with_data(&mk_build(), &data, &fast_opts).expect("coc fast");
    assert!(
        fast_out.skill_trigger_rate > base_out.skill_trigger_rate * 1.5,
        "+100% attack speed should nearly double the trigger rate (14-G2 fix): {} → {}",
        base_out.skill_trigger_rate,
        fast_out.skill_trigger_rate
    );
}

/// Recursion guards: ① a cycle (source == the triggered skill itself) → None
/// (falls back to the base convention); ② a trigger-source context rejects a nested
/// sub-calculation; ③ that context strips the whole trigger relationship.
#[test]
fn trigger_subcalc_recursion_guards() {
    let data = repo_data();
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 80,
            class_name: "Sorceress".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_gem_skill("ArmourBreakerPlayer", 10)
                .with_gem_skill("FireballPlayer", 10),
        )
        .with_main_socket_group(1);
    let opts = DataOrchestratorOptions::default();
    let mut context = test_context(&data);
    let group = &build.socket_groups[0];

    // ① Cycle detection: source gem id == the triggered main skill's id.
    assert!(
        trigger_source_stats(
            &mut context,
            &build,
            &data,
            &opts,
            group,
            &group.gem_skills[0],
            "ArmourBreakerPlayer"
        )
        .is_none(),
        "source == the triggered skill itself should fall back to None (base use_time convention)"
    );

    // ② A trigger-source context cannot expand another sub-calculation.
    {
        let mut child = context.trigger_source();
        assert!(
            trigger_source_stats(
                &mut child,
                &build,
                &data,
                &opts,
                group,
                &group.gem_skills[0],
                "FireballPlayer"
            )
            .is_none(),
            "depth ≥1 should reject expanding another sub-calc"
        );
        // ③ The trigger-source context strips trigger relationships entirely.
        let resolved = ResolvedSkillLevel {
            cooldown_s: Some(0.5),
            ..ResolvedSkillLevel::default()
        };
        assert!(
            trigger_modifiers(
                &mut child,
                &build,
                &data,
                &opts,
                &resolved,
                group,
                "ElementalStormPlayer"
            )
            .is_empty(),
            "the trigger relationship should be stripped in the sub-calc env"
        );
    }
    // The parent context remains usable after the child context is dropped.
    assert!(
        trigger_source_stats(
            &mut context,
            &build,
            &data,
            &opts,
            group,
            &group.gem_skills[0],
            "FireballPlayer"
        )
        .is_some(),
        "the parent calculation should still be able to expand a trigger source"
    );
}

/// requires_condition gating: a Hidden Blade-style entry requires Phasing —
/// recognition hits but injection is skipped when the condition isn't met
/// (vendor's disable semantics, panel stays at 0), and it doesn't fall into the built-in path.
#[test]
fn trigger_config_requires_condition_gates_injection() {
    let mut data = BuildData::empty();
    data.granted_effects.insert(
        "UnseenStrikePlayer".to_string(),
        pobr_data::catalog::GrantedEffectDef {
            id: "UnseenStrikePlayer".into(),
            is_support: false,
            active_skill: Some("UnseenStrikePlayer".into()),
            cast_time: Some(1000),
            require_skill_types: vec![],
            add_skill_types: vec![],
            exclude_skill_types: vec![],
            cannot_be_supported: false,
            support_gems_only: false,
            stat_set: None,
            additional_stat_set_ids: vec![],
            cost_types: vec![],
            minion_list: vec![],
            add_minion_list: vec![],
            minion_uses: vec![],
            minion_has_item_set: false,
            skill_types: vec!["Attack".into()],
        },
    );
    data.trigger_configs.insert(
        "UnseenStrikePlayer".to_string(),
        pobr_data::catalog::TriggerConfigDef {
            key: pobr_data::catalog::TriggerKeyDef {
                kind: "unique_item".into(),
                name: "the hidden blade".into(),
            },
            trigger_name: None,
            trigger_on_use: false,
            use_cast_rate: false,
            source_skill_cond: None,
            triggered_skill_cond: None,
            source_skill_name: None,
            requires_main_skill_name: None,
            trigger_chance_stat: None,
            source_rate_stat: None,
            cooldown_override_s: None,
            trigger_rate_cap_override: Some(2.0),
            global_trigger: true,
            source_is_self: true,
            source_rate_is_final: false,
            ignores_tick_rate: false,
            assuming_every_hit_kills: false,
            ignore_source_rate: false,
            trigger_on_crit: false,
            requires_condition: Some("Phasing".into()),
            match_effect_ids: vec!["UnseenStrikePlayer".into()],
            handler_id: None,
            note: None,
            vendor_ref: "Modules/CalcTriggers.lua:907-921".into(),
            verified: false,
        },
    );
    let group = SocketGroup::new().with_gem_skill("UnseenStrikePlayer", 10);
    let resolved = ResolvedSkillLevel::default();
    let opts = DataOrchestratorOptions::default();

    // Condition unmet (build config has no Phasing) → recognition hits but injection is empty.
    let build = Build::new();
    let mods = trigger_modifiers(
        &mut test_context(&data),
        &build,
        &data,
        &opts,
        &resolved,
        &group,
        "UnseenStrikePlayer",
    );
    assert!(
        mods.is_empty(),
        "should not inject when Phasing isn't set (vendor disable)"
    );

    // Condition met → injects the cap override + the global marker.
    let mut build_phasing = Build::new();
    build_phasing
        .config
        .conditions
        .insert("Phasing".to_string(), true);
    let mods = trigger_modifiers(
        &mut test_context(&data),
        &build_phasing,
        &data,
        &opts,
        &resolved,
        &group,
        "UnseenStrikePlayer",
    );
    let names: Vec<&str> = mods.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"TriggerRateCapOverride"));
    assert!(names.contains(&"TriggerSourceGlobal"));
}

// Unarmed base / weapon-type table-lookup switchover (migration-invariant regression)

/// After switching the unarmed base to the injected table, values still
/// match the old hardcoded match arm-for-arm (`BuildData::empty()` takes the
/// Default fallback, = the JSON values value-for-value; covers all 9 classes + the unknown-class fallback).
#[test]
fn unarmed_contribution_matches_legacy_hardcoded_values() {
    let data = BuildData::empty();
    let legacy: &[(&str, f64)] = &[
        ("Warrior", 8.0),
        ("Scion", 6.0),
        ("Mercenary", 6.0),
        ("Druid", 6.0),
        ("Witch", 5.0),
        ("Ranger", 5.0),
        ("Sorceress", 5.0),
        ("Huntress", 5.0),
        ("Monk", 5.0),
        // Unknown class: the old match's else branch (generic fallback).
        ("NoSuchClass", 5.0),
    ];
    for &(class, phys_max) in legacy {
        let build = Build::new().with_character(CharacterIdentity {
            level: 1,
            class_name: class.into(),
            ascendancy_name: String::new(),
        });
        let c = unarmed_contribution(&data, &build.character.class_name);
        assert_eq!(c.phys_min, 2.0, "{class} phys_min");
        assert_eq!(c.phys_max, phys_max, "{class} phys_max");
        assert_eq!(c.attack_rate, 1.65, "{class} attack_rate");
        // Old hardcoded value 0.05 (unit-convention TODO(parity), see the unarmed_contribution doc).
        assert_eq!(c.crit_chance, 0.05, "{class} crit_chance");
    }
}

/// A weapon base item for tests (only item_class matters for hold/melee classification).
fn weapon_base_item(name: &str, item_class: &str) -> pobr_data::catalog::BaseItemDef {
    pobr_data::catalog::BaseItemDef {
        req_str: 0,
        req_dex: 0,
        req_int: 0,
        id: format!("Test/{name}"),
        name: name.to_string(),
        item_class: item_class.to_string(),
        drop_level: 1,
        width: 1,
        height: 1,
        tags: vec![],
        implicits: vec![],
        mod_domain: 1,
        weapon: None,
        armour: None,
        spirit: None,
        charm_buff: Vec::new(),
    }
}

/// After switching weapon-type conditions to the injected table, they're
/// equivalent class-by-class to the old scattered predicates (including a
/// parity guard: Talisman / FishingRod aren't melee, and GGG's `Staff`
/// (quarterstaff) gets no conditions — vendor discrepancies are pinned to the old behavior).
#[test]
fn weapon_type_conditions_match_legacy_predicates() {
    let mut data = BuildData::empty();
    let cases: &[(&str, &[&str])] = &[
        // GGG `Warstaff` (quarterstaff) → table key `Staff` (label=Quarterstaff).
        ("Warstaff", &["UsingQuarterstaff", "UsingTwoHandedMelee"]),
        ("One Hand Mace", &["UsingMace", "UsingOneHandedMelee"]),
        ("Two Hand Mace", &["UsingMace", "UsingTwoHandedMelee"]),
        ("Bow", &["UsingBow"]),
        ("Crossbow", &["UsingCrossbow"]),
        ("Spear", &["UsingSpear", "UsingOneHandedMelee"]),
        ("Dagger", &["UsingDagger", "UsingOneHandedMelee"]),
        ("Claw", &["UsingOneHandedMelee"]),
        ("Flail", &["UsingOneHandedMelee"]),
        ("One Hand Sword", &["UsingOneHandedMelee"]),
        ("Two Hand Sword", &["UsingTwoHandedMelee"]),
        ("Two Hand Axe", &["UsingTwoHandedMelee"]),
        // parity guard: the old predicates didn't treat Talisman / FishingRod
        // as melee (vendor has melee=true; the discrepancy is recorded as a
        // schema TODO(parity), behavior alignment left for a separate commit).
        ("Talisman", &[]),
        ("FishingRod", &[]),
        // GGG `Staff` (quarterstaff class): the vendor table has no matching entry, so no weapon-type condition at all.
        ("Staff", &[]),
        ("Wand", &[]),
        ("Sceptre", &[]),
    ];
    for &(cls, expected) in cases {
        let base_name = format!("Test {cls}");
        data.base_items
            .insert(base_name.clone(), weapon_base_item(&base_name, cls));
        let build = Build::new().set_item(
            EquipmentSlot::Weapon1,
            Item {
                base: ItemBaseId::from(base_name.as_str()),
                rarity: ItemRarity::Normal,
                quality: 0,
                corrupted: false,
                implicit_texts: vec![],
                modifier_texts: vec![],
                enchant_texts: vec![],
                rolled_defence: RolledDefence::default(),
                parsed_stats: vec![],
            },
        );
        let vars = weapon_type_conditions(&build, &data);
        assert_eq!(&vars[..], expected, "item_class = {cls}");
    }
}

/// cfg weapon-slot flags: derived per vendor getWeaponFlags (same source and
/// gating as the Using* conditions).
#[test]
fn weapon_cfg_flags_dual_write_channel() {
    let mut data = BuildData::empty();
    let base_name = "Test One Hand Mace".to_string();
    data.base_items.insert(
        base_name.clone(),
        weapon_base_item(&base_name, "One Hand Mace"),
    );
    let build = Build::new().set_item(
        EquipmentSlot::Weapon1,
        Item {
            base: ItemBaseId::from(base_name.as_str()),
            rarity: ItemRarity::Normal,
            quality: 0,
            corrupted: false,
            implicit_texts: vec![],
            modifier_texts: vec![],
            enchant_texts: vec![],
            rolled_defence: RolledDefence::default(),
            parsed_stats: vec![],
        },
    );
    let bits = weapon_cfg_flags(&build, &data);
    let unarmed = weapon_cfg_flags(&Build::new(), &data);
    assert_eq!(
        bits,
        ModFlags::MACE | ModFlags::WEAPON | ModFlags::WEAPON_1H | ModFlags::WEAPON_MELEE,
        "one-handed mace → vendor getWeaponFlags bitset"
    );
    assert_eq!(
        unarmed,
        ModFlags::UNARMED,
        "empty main hand → only the Unarmed bit"
    );
}
