//! Raw incoming-hit inputs must replace the preset and reach EHP/max-hit output.
use pobr_build::breakdown::detail::{DetailKind, stat_detail};
use pobr_build::breakdown::stat_breakdown;
use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data_session, parse_build};
use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_gamedata::{GameData, current_data_dir};

fn data() -> BuildData {
    BuildData::load(&GameData::new(current_data_dir())).unwrap()
}

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
