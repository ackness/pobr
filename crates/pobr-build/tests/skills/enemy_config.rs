//! Raw incoming-hit inputs must replace the preset and reach EHP/max-hit output.
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
        let breakdown = stat_breakdown(&small, "TotalEHP").unwrap();
        let id = format!("config.enemy{kind}Damage");
        let entry = breakdown
            .mods
            .iter()
            .find(|m| m.origin_id.as_deref() == Some(&id))
            .unwrap();
        assert!(entry.active);
        assert_eq!(entry.origin_kind.as_deref(), Some("EnemyConfig"));
        assert_eq!(entry.value, Some(100.0));
        assert!(
            entry
                .source_text
                .as_ref()
                .unwrap()
                .contains(&format!("enemy{kind}Damage = 100"))
        );
        assert!(
            breakdown
                .mods
                .iter()
                .any(|m| m.mod_type == "BASE" && !m.active)
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
        assert!(
            stat_breakdown(&configured, &stat)
                .unwrap()
                .mods
                .iter()
                .any(|m| m.origin_id.as_deref() == Some(&format!("config.enemy{kind}Pen")))
        );
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
