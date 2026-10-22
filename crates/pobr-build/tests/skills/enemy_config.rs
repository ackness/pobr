//! Raw incoming-hit inputs must replace the preset and reach EHP/max-hit output.
use pobr_build::breakdown::detail::{DetailKind, stat_detail};
use pobr_build::breakdown::stat_breakdown;
use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data_session, parse_build};
use pobr_core::calc::{CalculationSession, MinimalInput};

use crate::test_data as data;

fn calc(data: &BuildData, inputs: &str, extra: &[&str]) -> CalculationSession {
    let xml = format!(
        r#"<PathOfBuilding2><Build level="80" className="Warrior"/>
<Config><Input name="resistancePenalty" number="0"/>{inputs}</Config></PathOfBuilding2>"#
    );
    calculate_with_data_session(
        &parse_build(&xml).unwrap(),
        data,
        &DataOrchestratorOptions {
            inject_character_base: false,
            base_input: MinimalInput {
                base_life: 10000.0,
                ..Default::default()
            },
            extra_modifier_texts: extra.iter().map(|s| (*s).into()).collect(),
            ..Default::default()
        },
    )
    .unwrap()
}

fn single_hit(kind: &str, damage: f64) -> String {
    let mut inputs = String::from(r#"<Input name="enemyCritChance" number="0"/>"#);
    for name in ["Physical", "Fire", "Cold", "Lightning", "Chaos"] {
        let value = if name == kind { damage } else { 0.0 };
        inputs += &format!(r#"<Input name="enemy{name}Damage" number="{value}"/>"#);
    }
    inputs
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 0.01, "{actual} != {expected}");
}

#[test]
fn each_damage_input_replaces_the_preset_and_zero_removes_the_type() {
    let data = data();
    for (index, kind) in ["Physical", "Fire", "Cold", "Lightning", "Chaos"]
        .iter()
        .enumerate()
    {
        let small = calc(&data, &single_hit(kind, 100.0), &[]);
        let large = calc(&data, &single_hit(kind, 200.0), &[]);
        close(small.output().total_enemy_damage_in, 100.0);
        close(large.output().total_enemy_damage_in, 200.0);
        close(
            small.output().number_of_damaging_hits,
            2.0 * large.output().number_of_damaging_hits,
        );
        let terms = small.ehp_terms().unwrap();
        close(terms.enemy_damage[index], 100.0);
        close(terms.enemy_damage.iter().sum(), 100.0);
        let detail = stat_detail(&small, "TotalEHP").unwrap();
        assert_eq!(detail.kind, DetailKind::Derived);
        assert!(detail.breakdown.mods.is_empty());
        assert_eq!(detail.breakdown.base_total, 0.0);
        let inputs = &detail.derived.unwrap().inputs;
        close(
            inputs
                .iter()
                .find(|input| input.id == "EnemyDamage")
                .unwrap()
                .value,
            100.0,
        );
        // Enemy provenance remains inspectable, separately from additive stat rows.
        let rows = small.enemy_hit_mods(true);
        let id = format!("config.enemy{kind}Damage");
        let entry = rows
            .iter()
            .find(|row| {
                row.modifier
                    .origin
                    .as_ref()
                    .is_some_and(|origin| origin.source_id.id == id)
            })
            .unwrap();
        assert!(entry.active);
        assert_eq!(
            format!(
                "{:?}",
                entry.modifier.origin.as_ref().unwrap().source_id.kind
            ),
            "EnemyConfig"
        );
        assert_eq!(entry.value, Some(100.0));
        assert!(
            entry
                .modifier
                .source
                .as_ref()
                .unwrap()
                .contains(&format!("enemy{kind}Damage = 100"))
        );
        assert!(
            rows.iter().any(
                |row| row.modifier.mod_type == pobr_data::prelude::ModType::Base && !row.active
            )
        );
    }
}

#[test]
fn each_penetration_input_lowers_the_matching_maximum_hit() {
    let data = data();
    for kind in ["Fire", "Cold", "Lightning"] {
        let plain = calc(
            &data,
            &format!(
                r#"{}<Input name="enemy{kind}Pen" number="0"/>"#,
                single_hit(kind, 100.0)
            ),
            &["+75% to all Elemental Resistances"],
        );
        let configured = calc(
            &data,
            &format!(
                r#"{}<Input name="enemy{kind}Pen" number="25"/>"#,
                single_hit(kind, 100.0)
            ),
            &["+75% to all Elemental Resistances"],
        );
        let hit = |s: &CalculationSession| match kind {
            "Fire" => s.output().fire_max_hit_pob2,
            "Cold" => s.output().cold_max_hit_pob2,
            _ => s.output().lightning_max_hit_pob2,
        };
        close(hit(&plain), 2.0 * hit(&configured));
        close(
            plain.output().number_of_damaging_hits,
            2.0 * configured.output().number_of_damaging_hits,
        );
        let stat = format!("{kind}MaximumHitTaken");
        // A non-additive enemy query must never masquerade as this stat's sum.
        assert!(stat_breakdown(&configured, &stat).is_none());
        assert!(configured.enemy_hit_mods(false).iter().any(|row| {
            row.modifier
                .origin
                .as_ref()
                .is_some_and(|origin| origin.source_id.id == format!("config.enemy{kind}Pen"))
        }));
    }
}

#[test]
fn crit_chance_and_bonus_scale_incoming_damage_separately() {
    let data = data();
    let hit = single_hit("Physical", 100.0);
    let no_crit = calc(&data, &hit, &[]);
    let crit = calc(
        &data,
        &hit.replace(
            "enemyCritChance\" number=\"0",
            "enemyCritChance\" number=\"100",
        ),
        &[],
    );
    let double = calc(
        &data,
        &format!(
            r#"{}<Input name="enemyCritDamage" number="100"/>"#,
            hit.replace(
                "enemyCritChance\" number=\"0",
                "enemyCritChance\" number=\"100"
            )
        ),
        &[],
    );
    close(
        no_crit.output().number_of_damaging_hits / crit.output().number_of_damaging_hits,
        1.3,
    );
    close(
        no_crit.output().number_of_damaging_hits / double.output().number_of_damaging_hits,
        2.0,
    );
    close(double.ehp_terms().unwrap().enemy_damage[0], 200.0);
}

#[test]
fn physical_overwhelm_reduces_armour_mitigation_and_maximum_hit() {
    let data = data();
    let hit = single_hit("Physical", 100.0);
    let plain = calc(&data, &hit, &["+10000 to Armour"]);
    let overwhelmed = calc(
        &data,
        &format!(r#"{hit}<Input name="enemyPhysicalOverwhelm" number="90"/>"#),
        &["+10000 to Armour"],
    );
    assert!(plain.output().number_of_damaging_hits > overwhelmed.output().number_of_damaging_hits);
    assert!(plain.output().physical_max_hit_pob2 > overwhelmed.output().physical_max_hit_pob2);
    close(
        overwhelmed.output().number_of_damaging_hits,
        overwhelmed.output().life / 100.0,
    );
    close(
        overwhelmed.output().physical_max_hit_pob2,
        overwhelmed.output().life,
    );
}

#[test]
fn hit_category_selects_attack_spell_and_untyped_evasion() {
    let data = data();
    let hit = single_hit("Physical", 100.0);
    let calc_category = |category: &str| {
        calc(
            &data,
            &format!(r#"{hit}<Input name="enemyDamageType" string="{category}"/>"#),
            &["+10000 to Evasion Rating"],
        )
    };
    let untyped = calc_category("Untyped");
    let melee = calc_category("Melee");
    let projectile = calc_category("Projectile");
    let spell = calc_category("Spell");
    let spell_projectile = calc_category("SpellProjectile");
    assert!(melee.output().total_ehp_pob2 > untyped.output().total_ehp_pob2);
    close(
        projectile.output().total_ehp_pob2,
        melee.output().total_ehp_pob2,
    );
    close(spell.output().total_ehp_pob2, melee.output().total_ehp_pob2);
    close(
        spell_projectile.output().total_ehp_pob2,
        projectile.output().total_ehp_pob2,
    );
    let average = calc_category("Average").output().total_ehp_pob2;
    close(average, melee.output().total_ehp_pob2);
}

#[test]
fn unlucky_rolls_reduce_evasion_and_ehp_without_changing_the_hit() {
    let data = data();
    let hit = format!(
        r#"{}<Input name="enemyDamageType" string="Melee"/>"#,
        single_hit("Physical", 100.0)
    );
    let plain = calc(&data, &hit, &["+10000 to Evasion Rating"]);
    let two = calc(
        &data,
        &format!(r#"{hit}<Input name="EHPUnluckyWorstOf" number="2"/>"#),
        &["+10000 to Evasion Rating"],
    );
    let four = calc(
        &data,
        &format!(r#"{hit}<Input name="EHPUnluckyWorstOf" number="4"/>"#),
        &["+10000 to Evasion Rating"],
    );
    assert!(plain.output().total_ehp_pob2 > two.output().total_ehp_pob2);
    assert!(two.output().total_ehp_pob2 > four.output().total_ehp_pob2);
    close(
        two.ehp_terms().unwrap().not_hit_chance,
        100.0 * (plain.ehp_terms().unwrap().not_hit_chance / 100.0).powi(2),
    );
    close(
        plain.output().number_of_damaging_hits,
        four.output().number_of_damaging_hits,
    );
}

#[test]
fn ehp_calcs_fields_share_stage_terms_and_details_reconstruct() {
    use pobr_build::breakdown::detail::detail_kind;
    use pobr_core::{Modifier, display_value};
    use pobr_data::prelude::ModType;

    let kinds = ["Physical", "Fire", "Cold", "Lightning", "Chaos"];
    let mut session = CalculationSession::new(MinimalInput {
        base_life: 10_000.0,
        ..Default::default()
    });
    for (kind, damage) in kinds.into_iter().zip([1000.0, 200.0, 300.0, 400.0, 500.0]) {
        session.add_enemy_modifiers([Modifier::number(
            format!("Enemy{kind}Damage"),
            ModType::Base,
            damage,
        )]);
    }
    session.add_enemy_modifiers([
        Modifier::flag("NeverCrit"),
        Modifier::number("Damage", ModType::Inc, 20.0),
        Modifier::number("EHPUnluckyWorstOf", ModType::Override, 2.0),
    ]);
    for (name, value) in [
        ("PhysicalDamageTakenAsFire", 25.0),
        ("PhysicalDamageReduction", 10.0),
        ("FireDamageReduction", 20.0),
        ("ColdDamageReduction", 30.0),
        ("LightningDamageReduction", 40.0),
        ("PhysicalDamageTaken", 10.0),
        ("FireResist", 50.0),
        ("ColdResist", 25.0),
        ("LightningResist", 75.0),
        ("ShieldBlockChance", 40.0),
        ("SpellBlockChance", 40.0),
        ("BlockEffect", 20.0),
        ("AvoidAllDamageFromHitsChance", 20.0),
    ] {
        session.add_modifiers([Modifier::number(name, ModType::Base, value)]);
    }
    session.add_modifiers([Modifier::number("DamageTaken", ModType::More, -10.0)]);
    for (kind, bypass) in kinds.into_iter().zip([0.0, 25.0, 50.0, 75.0, 100.0]) {
        session.add_modifiers([Modifier::number(
            format!("{kind}EnergyShieldBypass"),
            ModType::Base,
            bypass,
        )]);
    }
    session.perform_minimal().unwrap();
    let t = session.ehp_terms().unwrap();
    let out = session.output();
    // 20% enemy increase, then 25% physical taken as fire. Resistances and
    // per-type reduction apply before +10 physical and the 0.9 taken multiplier.
    let expected_enemy = [1200.0, 240.0, 360.0, 480.0, 600.0];
    let expected_converted = [900.0, 540.0, 360.0, 480.0, 600.0];
    let expected_hit = [738.0, 194.4, 170.1, 64.8, 540.0];
    for (i, kind) in kinds.into_iter().enumerate() {
        for (suffix, expected) in [
            ("EnemyDamage", expected_enemy[i]),
            ("TakenDamage", expected_converted[i]),
            ("TakenHit", expected_hit[i]),
            ("TakenHitMult", expected_hit[i] / expected_converted[i]),
            ("EnergyShieldBypass", i as f64 * 25.0),
        ] {
            let stat = format!("{kind}{suffix}");
            close(display_value(out, &stat), expected);
            if matches!(suffix, "EnemyDamage" | "EnergyShieldBypass") {
                assert_eq!(detail_kind(&stat), DetailKind::None);
                assert!(stat_detail(&session, &stat).is_none());
                continue;
            }
            let detail = stat_detail(&session, &stat).unwrap();
            assert_eq!(detail.kind, DetailKind::Derived);
            let d = detail.derived.unwrap();
            let input = |id: &str| d.inputs.iter().find(|row| row.id == id).unwrap().value;
            let reconstructed = match suffix {
                "TakenDamage" => kinds
                    .into_iter()
                    .map(|src| {
                        input(&format!("{src}EnemyDamage")).max(0.0)
                            * input(&format!("{src}TakenAsFraction"))
                    })
                    .sum(),
                "TakenHit" => {
                    (input("ConvertedIncomingDamage")
                        * input("ResistTakenMultiplier")
                        * input("ReductionTakenMultiplier")
                        + input("FlatDamageTaken"))
                    .max(0.0)
                        * input("AfterReductionMultiplier")
                }
                "TakenHitMult" => input("MitigatedHit") / input("ConvertedIncomingDamage"),
                _ => unreachable!(),
            };
            close(reconstructed, expected);
        }
        close(t.enemy_damage[i], expected_enemy[i]);
        close(t.taken_damage[i], expected_converted[i]);
        close(t.taken_hit[i], expected_hit[i]);
    }
    for (stat, expected) in [
        ("totalEnemyDamage", 2880.0),
        ("totalTakenDamage", 2880.0),
        ("totalTakenHit", 1707.3),
        ("FireDamageReduction", 20.0),
        ("ColdDamageReduction", 30.0),
        ("LightningDamageReduction", 40.0),
        // Worst of two makes 40% block 16%, and 20% not-hit 4%.
        ("ConfiguredDamageChance", 87.2),
        ("ConfiguredNotHitChance", 4.0),
        ("DamageTakenOnBlock", 20.0),
    ] {
        close(display_value(out, stat), expected);
        let detail = stat_detail(&session, stat).unwrap();
        assert_eq!(detail.kind, DetailKind::Derived);
        let d = detail.derived.unwrap();
        let input = |id: &str| d.inputs.iter().find(|row| row.id == id).unwrap().value;
        let reconstructed = match stat {
            "ConfiguredDamageChance" => {
                100.0
                    * input("BlockEffectMultiplier")
                    * input("DeflectMultiplier")
                    * (1.0 - input("AverageAvoidChance") / 100.0)
            }
            "ConfiguredNotHitChance" => {
                100.0
                    * (input("NotHitBeforeUnlucky") / 100.0)
                        .clamp(0.0, 1.0)
                        .powf(input("UnluckyWorstOf"))
            }
            "DamageTakenOnBlock" => 100.0 - input("BlockEffect"),
            _ if stat.ends_with("DamageReduction") => {
                ((input("ArmourReduction") + input("FlatReduction")).min(input("ReductionCap"))
                    - input("EnemyOverwhelm"))
                .clamp(0.0, input("ReductionCap"))
            }
            _ => d.inputs.iter().map(|row| row.value).sum(),
        };
        close(reconstructed, expected);
    }
    // The existing EHP calculation still uses pre-modifier enemy damage.
    close(out.total_enemy_damage_in, 2400.0);
    close(
        display_value(out, "TotalEHP"),
        t.mitigated_hits / (1.0 - 4.0 / 100.0) * 2400.0,
    );
}

#[test]
fn ehp_zero_incoming_multiplier_is_zero_even_with_flat_damage() {
    use pobr_core::{Modifier, display_value};
    use pobr_data::prelude::ModType;
    let mut session = CalculationSession::new(MinimalInput::default());
    session.add_modifiers([Modifier::number("FireDamageTaken", ModType::Base, 10.0)]);
    session.perform_minimal().unwrap();
    close(display_value(session.output(), "FireTakenHit"), 10.0);
    close(display_value(session.output(), "FireTakenDamage"), 0.0);
    close(display_value(session.output(), "FireTakenHitMult"), 0.0);
    let detail = session.derived_detail("FireTakenHitMult").unwrap();
    assert_eq!(detail.formula, "0 * ConvertedIncomingDamage");
    let input = detail
        .inputs
        .iter()
        .find(|(id, _)| id == "ConvertedIncomingDamage")
        .unwrap()
        .1;
    close(0.0 * input, 0.0);
}

#[test]
fn further_ehp_fields_expose_mitigation_and_pool_terms() {
    use pobr_build::breakdown::detail::detail_kind;
    use pobr_core::{Modifier, display_value};
    use pobr_data::prelude::ModType;

    let mut session = CalculationSession::new(MinimalInput {
        base_life: 1000.0,
        base_mana: 500.0,
        ..Default::default()
    });
    session.add_enemy_modifiers([
        Modifier::flag("NeverCrit"),
        Modifier::number("EnemyFireDamage", ModType::Base, 100.0),
        Modifier::number("EnemyFirePen", ModType::Base, 10.0),
        Modifier::number("EnemyColdPen", ModType::Base, 10.0),
        Modifier::number("FireDamage", ModType::Inc, 50.0),
    ]);
    for (name, kind, value) in [
        ("FireResist", ModType::Base, 50.0),
        ("ColdResist", ModType::Base, -20.0),
        ("LightningResist", ModType::Base, 120.0),
        ("ChaosResist", ModType::Base, 30.0),
        ("AttackDamageTaken", ModType::Inc, 20.0),
        ("SpellDamageTaken", ModType::Inc, -10.0),
        ("DamageTaken", ModType::More, -10.0),
        ("DamageReductionMax", ModType::Base, 80.0),
        ("FireDamageReductionMax", ModType::Base, 50.0),
        ("FireDamageReduction", ModType::Base, 20.0),
        ("DamageTakenFromManaBeforeLife", ModType::Base, 40.0),
        ("LifeLossPrevented", ModType::Base, 20.0),
    ] {
        session.add_modifiers([Modifier::number(name, kind, value)]);
    }
    session.perform_minimal().unwrap();
    let out = session.output();
    // Attack 1.2 × 0.9 = 1.08 and spell 0.9 × 0.9 = 0.81 average to 0.945.
    // Fire: 10 penetration leaves 40% resistance (0.6) and 20% reduction (0.8).
    // Cold's negative resistance is not penetrated: 1.2. Lightning's 120 is
    // capped at 75 for the hit but its total stays 120.
    // MoM 40%: 500 Mana carries 40% of 1250 damage, Life the remaining 250
    // (1500); against the 1250 Life hit pool (20% loss prevented) it is 1750.
    let expected = [
        ("FireEnemyDamageMult", 1.5),
        ("ColdEnemyDamageMult", 1.0),
        ("FireEnemyPen", 10.0),
        ("PhysicalEnemyPen", 0.0),
        ("FireResistTakenHitMulti", 0.6),
        ("ColdResistTakenHitMulti", 1.2),
        ("LightningResistTakenHitMulti", 0.25),
        ("ChaosResistTakenHitMulti", 0.7),
        ("PhysicalResistTakenHitMulti", 1.0),
        ("FireAttackTakenHitMult", 1.08),
        ("FireSpellTakenHitMult", 0.81),
        ("FireAfterReductionTakenHitMulti", 0.945),
        ("FireBaseTakenHitMult", 0.6 * 0.8 * 0.945),
        ("DamageReductionMax", 80.0),
        ("FireDamageReductionMax", 50.0),
        ("PhysicalDamageReductionMax", 80.0),
        ("LightningResistTotal", 120.0),
        ("ColdResistTotal", -20.0),
        ("LifeHitPool", 1250.0),
        ("FireManaEffectiveLife", 1500.0),
        ("FireMoMHitPool", 1750.0),
        ("FireTotalHitPool", 1750.0),
    ];
    for (stat, value) in expected {
        close(display_value(out, stat), value);
        if stat.ends_with("EnemyDamageMult") || stat.ends_with("EnemyPen") {
            assert_eq!(detail_kind(stat), DetailKind::None);
            continue;
        }
        let detail = stat_detail(&session, stat).unwrap();
        if stat.ends_with("AttackTakenHitMult") || stat.ends_with("SpellTakenHitMult") {
            assert_eq!(detail.kind, DetailKind::Sources);
            let names: Vec<_> = detail.sources.iter().map(|s| s.name.as_str()).collect();
            // Groups with contributing modifiers: the shared 10% less and the layer.
            assert!(names.contains(&"DamageTaken"), "{names:?}");
            let layer = if stat.contains("Attack") {
                "AttackDamageTaken"
            } else {
                "SpellDamageTaken"
            };
            assert!(names.contains(&layer));
            continue;
        }
        assert_eq!(detail.kind, DetailKind::Derived, "{stat}");
        let d = detail.derived.unwrap();
        let input = |id: &str| {
            d.inputs
                .iter()
                .find(|row| row.id == id)
                .map(|row| row.value)
        };
        let get = |id: &str| input(id).unwrap();
        let reconstructed = if stat.ends_with("ResistTakenHitMulti") {
            let resist = get("Resistance");
            let after = if resist > 0.0 {
                (resist - get("EnemyPenetration")).max(0.0)
            } else {
                resist
            };
            1.0 - after / 100.0
        } else if stat.ends_with("AfterReductionTakenHitMulti") {
            get("HitDamageTakenMultiplier") * get("DeflectMultiplier")
        } else if stat.ends_with("BaseTakenHitMult") {
            get("ResistTakenMultiplier")
                * get("ReductionTakenMultiplier")
                * get("AfterReductionMultiplier")
        } else if stat.ends_with("DamageReductionMax") {
            let global = input("DamageReductionMax")
                .or(input("HighestDamageReductionMax"))
                .or(input("DamageReductionCap"))
                .unwrap();
            match input("HighestFireDamageReductionMax") {
                Some(cap) => cap.min(global),
                None => global,
            }
        } else if stat.ends_with("ResistTotal") {
            ((get("BaseResistance") + get("FlatResistance"))
                * ((1.0 + get("IncreasedResistance") / 100.0) * get("MoreResistance")).max(0.0))
            .trunc()
        } else if stat == "LifeHitPool" {
            let (life, max, prevented, below) = (
                get("Life"),
                get("MaximumLife"),
                get("LifeLossPrevented"),
                get("LifeLossBelowHalfPrevented"),
            );
            (life - max / 2.0).max(0.0) / (1.0 - prevented / 100.0)
                + life.min(max / 2.0) / (1.0 - below / 100.0) / (1.0 - prevented / 100.0)
        } else {
            // Pools add their retained stages.
            d.inputs.iter().map(|row| row.value).sum()
        };
        close(reconstructed, value);
    }
    // Reflect is not modelled.
    assert!(
        !pobr_core::display_catalog()
            .iter()
            .any(|d| d.id.as_str() == "FireTakenReflect")
    );
}

/// The defensive damage-over-time views, with hand-computed numbers:
/// resistance against damage over time (including a "+#% Chaos Resistance
/// against Damage Over Time" line, game stat
/// `additional_chaos_resistance_against_damage_over_time_%`), additional
/// damage reduction (not Armour), over-time damage taken, the non-hit pool
/// (no hit-only Life loss prevention, with Runic Ward) and the survival time.
#[test]
fn defensive_damage_over_time_views_reconstruct_from_their_terms() {
    use pobr_core::display_value;

    let data = data();
    let inputs = r#"<Input name="enemySpeed" number="1000"/>"#;
    let session = calc(
        &data,
        inputs,
        &[
            "+40% to Fire Resistance",
            "+20% to Chaos Resistance",
            "+15% Chaos Resistance against Damage Over Time",
            "20% additional Physical Damage Reduction",
            "+2000 to Armour",
            "10% increased Damage taken",
        ],
    );
    let out = session.output();
    // Fire resistance applies to damage over time as to hits (50 in this
    // fixture, no over-time-only line); Chaos adds the over-time-only 15.
    close(out.fire_resistance, 50.0);
    close(display_value(out, "FireResistOverTime"), 50.0);
    close(out.chaos_resistance, 20.0);
    close(display_value(out, "ChaosResistOverTime"), 35.0);
    // Physical: 20% additional reduction, Armour ignored, 10% increased taken.
    close(display_value(out, "PhysicalTakenDotMult"), 0.8 * 1.1);
    close(display_value(out, "FireTakenDotMult"), 0.5 * 1.1);
    close(display_value(out, "ChaosTakenDotMult"), 0.65 * 1.1);
    // Recoverable Life only: no Mana, Energy Shield or Runic Ward.
    let life = out.life_unreserved;
    close(display_value(out, "FireTotalPool"), life);
    close(display_value(out, "FireDotEHP"), life / (0.5 * 1.1));
    // 1000 ms per enemy action at full action speed.
    close(
        display_value(out, "EHPSurvivalTime"),
        out.total_number_of_hits * 1.0,
    );

    for stat in [
        "FireResistOverTime",
        "ChaosResistOverTime",
        "PhysicalTakenDotMult",
        "FireTakenDotMult",
        "ChaosTakenDotMult",
        "FireTotalPool",
        "FireDotEHP",
        "PhysicalDotEHP",
        "EHPSurvivalTime",
    ] {
        let detail = stat_detail(&session, stat).unwrap();
        assert_eq!(detail.kind, DetailKind::Derived, "{stat}");
        let d = detail.derived.unwrap();
        let get = |id: &str| {
            d.inputs
                .iter()
                .find(|row| row.id == id)
                .map(|row| row.value)
                .unwrap_or_else(|| panic!("{stat}: no {id}"))
        };
        let reconstructed = if stat.ends_with("ResistOverTime") {
            let total = ((get("BaseResistance") + get("FlatResistance"))
                * ((1.0 + get("IncreasedResistance") / 100.0) * get("MoreResistance")).max(0.0))
            .trunc();
            total.min(get("MaximumResistance")).max(get("ResistFloor"))
        } else if stat.ends_with("TakenDotMult") {
            ((1.0 - get("Resistance") / 100.0)
                * (1.0 - get("DamageReduction") / 100.0)
                * get("OverTimeDamageTaken"))
            .max(0.0)
        } else if stat.ends_with("TotalPool") {
            get("ManaEffectiveLife") + get("EnergyShieldInPool") + get("RunicWardInPool")
        } else if stat.ends_with("DotEHP") {
            get("TotalPool") / get("TakenDotMult")
        } else {
            get("TotalNumberOfHits") * get("EnemyActionTime") / 1000.0 / get("EnemyActionSpeed")
        };
        close(reconstructed, display_value(out, stat));
    }
}

/// Runic Ward is in the non-hit pool; hit-only Life loss prevention is not,
/// and Chaos Inoculation makes Chaos damage over time zero. Without a
/// configured enemy attack / cast time there is no survival time.
#[test]
fn damage_over_time_pool_has_ward_without_hit_only_layers() {
    use pobr_core::display_value;

    let data = data();
    // The exporter's 700 ms placeholder is an assumption, not a configured time.
    let plain = calc(
        &data,
        r#"<Placeholder name="enemySpeed" number="700"/>"#,
        &["+500 to maximum Runic Ward"],
    );
    let out = plain.output();
    close(out.ward, 500.0);
    close(
        display_value(out, "FireTotalPool"),
        out.life_unreserved + 500.0,
    );
    close(display_value(out, "EHPSurvivalTime"), 0.0);

    let prevented = calc(
        &data,
        "",
        &[
            "20% of Life Loss from Hits is prevented, then that much Life is lost over 4 seconds instead",
        ],
    );
    let out = prevented.output();
    // The hit pool grows by the prevented share; the non-hit pool does not.
    close(
        display_value(out, "FireTotalHitPool"),
        out.life_unreserved / 0.8,
    );
    close(display_value(out, "FireTotalPool"), out.life_unreserved);

    // "Immune to Chaos Damage": no Chaos damage over time is taken.
    let mut session = CalculationSession::new(MinimalInput {
        base_life: 1000.0,
        ..Default::default()
    });
    session.add_modifiers([pobr_core::Modifier::flag("ChaosInoculation")]);
    session.perform_minimal().unwrap();
    close(display_value(session.output(), "ChaosTakenDotMult"), 0.0);
    assert!(display_value(session.output(), "ChaosDotEHP").is_infinite());
    close(display_value(session.output(), "FireTakenDotMult"), 1.0);
}
