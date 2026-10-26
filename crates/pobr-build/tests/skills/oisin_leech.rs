use pobr_build::{Build, DataOrchestratorOptions, calculate_with_data_session, parse_build};
use pobr_core::calc::{CalculationSession, CompletedCalculationSession, MinimalInput};
use pobr_core::{CalcConfig, Modifier, display_value};
use pobr_data::prelude::*;

const SUPPORT: &str = "SupportOisinsOathPlayer";
const STAT: &str = "mana_leech_from_elemental_instead";
const LEECH: &str = "Leech 8.7% of Physical Attack Damage as Mana";

fn build(host: &str, support_group: usize, support_enabled: bool) -> Build {
    let group = |index, skill| {
        let support = if support_group == index {
            format!(r#"<Gem skillId="{SUPPORT}" level="1" enabled="{support_enabled}"/>"#)
        } else {
            String::new()
        };
        format!(
            r#"<Skill enabled="true"><Gem skillId="{skill}" level="20" enabled="true"/>{support}</Skill>"#
        )
    };
    parse_build(&format!(
        r#"<PathOfBuilding2>
<Build level="90" className="Huntress" mainSocketGroup="1"/>
<Skills>{}{}</Skills>
<Items activeItemSet="1">
<Item id="1">Rarity: NORMAL
Heavy Bow
</Item>
<Item id="2">Rarity: RARE
Recovery Test
Gold Ring
Implicits: 0
{LEECH}
</Item>
<ItemSet id="1"><Slot name="Weapon 1" itemId="1"/><Slot name="Ring 1" itemId="2"/></ItemSet>
</Items></PathOfBuilding2>"#,
        group(1, host),
        group(2, "IceShotPlayer"),
    ))
    .unwrap()
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

fn controlled_hit(
    flags: &[Modifier],
    sources: &[Modifier],
    cfg: CalcConfig,
    damage: [f64; 5],
) -> CompletedCalculationSession {
    let mut session = CalculationSession::new(MinimalInput {
        base_life: 10000.0,
        base_mana: 10000.0,
        base_hit_min: damage[0],
        base_hit_max: damage[0],
        base_action_rate: 2.0,
        ..Default::default()
    })
    .with_config(cfg);
    session.add_modifiers(flags.iter().chain(sources).cloned());
    session.add_modifiers([
        Modifier::flag("NeverCrit"),
        Modifier::number("EnergyShield", ModType::Base, 10000.0),
    ]);
    for (ty, amount) in ["Fire", "Cold", "Lightning", "Chaos"]
        .into_iter()
        .zip(&damage[1..])
    {
        for bound in ["Min", "Max"] {
            session.add_modifiers([Modifier::number(
                format!("{ty}Damage{bound}"),
                ModType::Base,
                *amount,
            )]);
        }
    }
    session.finish().unwrap()
}

fn assert_mana_instance(session: &CompletedCalculationSession, amount: f64) {
    close(
        display_value(session.output(), "ManaLeechGainPerHit"),
        amount,
    );
    let detail = session.derived_detail("ManaLeechRate").unwrap();
    // With no eligible percentage, the panel retains only its zero rate.
    let input = |name| {
        detail
            .inputs
            .iter()
            .find(|(id, _)| id == name)
            .map_or(0.0, |(_, value)| *value)
    };
    close(input("PrimaryLeechedAmount"), amount);
    let rate = amount.min(input("PrimaryLeechRateCap"));
    close(display_value(session.output(), "ManaLeechRate"), rate);
    close(display_value(session.output(), "ManaLeechGainRate"), rate);
}

#[test]
fn native_oisin_replaces_physical_mana_leech_only_for_its_enabled_supported_skill() {
    let data = crate::test_data();
    assert!(
        data.effect_stats(SUPPORT, 1, 0, None)
            .base
            .iter()
            .any(|s| s.stat == STAT && s.value == 1.0)
    );
    let options = DataOrchestratorOptions {
        base_input: MinimalInput {
            base_mana: 10000.0,
            ..Default::default()
        },
        inject_character_base: false,
        mode_effective: false,
        extra_modifier_texts: [
            "Never deal Critical Hits",
            "Adds 200 to 200 Fire Damage to Attacks",
            "Adds 300 to 300 Cold Damage to Attacks",
            "Adds 400 to 400 Lightning Damage to Attacks",
            "Adds 500 to 500 Chaos Damage to Attacks",
        ]
        .map(str::to_owned)
        .to_vec(),
        ..Default::default()
    };
    let calculate = |build: &Build| calculate_with_data_session(build, data, &options).unwrap();
    let native = calculate(&build("IceShotPlayer", 1, true));
    let baseline = calculate(&build("IceShotPlayer", 0, true));
    let source_id = format!("{SUPPORT}.{STAT}");
    let flags: Vec<_> = native
        .all_mods()
        .into_iter()
        .filter(|m| {
            m.origin
                .as_ref()
                .is_some_and(|o| o.source_id.id == source_id)
        })
        .cloned()
        .collect();
    assert_eq!(flags.len(), 2);
    for name in [
        "ManaLeechBasedOnElementalDamage",
        "Condition:NoManaLeechFromPhysicalDamage",
    ] {
        let modifier = flags.iter().find(|m| m.name.as_str() == name).unwrap();
        assert_eq!(modifier.mod_type, ModType::Flag);
        let origin = modifier.origin.as_ref().unwrap();
        assert_eq!(origin.source_id.kind, SourceKind::SupportGem);
        assert!(origin.raw_text.as_ref().unwrap().contains(STAT));
    }
    assert!(
        native
            .unsupported_modifier_texts()
            .iter()
            .all(|s| !s.contains(STAT))
    );
    assert_eq!(
        native.output().damage_components,
        baseline.output().damage_components
    );
    let parts = &native.output().damage_components;
    let physical: f64 = parts
        .iter()
        .filter(|c| c.damage_type == DamageType::Physical)
        .map(|c| c.avg())
        .sum();
    let elemental: f64 = parts
        .iter()
        .filter(|c| c.damage_type.is_elemental())
        .map(|c| c.avg())
        .sum();
    let chaos: f64 = parts
        .iter()
        .filter(|c| c.damage_type == DamageType::Chaos)
        .map(|c| c.avg())
        .sum();
    assert!(physical > 0.0 && elemental > 0.0 && chaos > 0.0);
    let scale = (40000.0 / (physical + elemental + chaos)).min(1.0);
    close(
        native.output().mana_leech_gain_per_hit,
        elemental * scale * 0.087,
    );
    close(
        baseline.output().mana_leech_gain_per_hit,
        physical * scale * 0.087,
    );
    let leech: Vec<_> = native
        .all_mods()
        .into_iter()
        .filter(|m| {
            m.origin
                .as_ref()
                .is_some_and(|o| o.raw_text.as_deref() == Some(LEECH))
        })
        .cloned()
        .collect();
    assert_eq!(leech.len(), 1);
    assert_eq!(leech[0].flags, ModFlags::ATTACK);

    // These negative cases use the actual imported groups, not synthetic flags.
    for other in [
        build("IceShotPlayer", 1, false),
        build("IceShotPlayer", 2, true),
    ] {
        let gated = calculate(&other);
        close(
            gated.output().mana_leech_gain_per_hit,
            baseline.output().mana_leech_gain_per_hit,
        );
        assert!(gated.all_mods().iter().all(|m| {
            m.origin
                .as_ref()
                .is_none_or(|o| o.source_id.id != source_id)
        }));
    }
    let incompatible = calculate(&build("WindDancerPlayer", 1, true));
    assert!(incompatible.all_mods().iter().all(|m| {
        m.origin
            .as_ref()
            .is_none_or(|o| o.source_id.id != source_id)
    }));
    let spell = calculate(&build("FireballPlayer", 1, true));
    assert!(
        spell
            .mod_db()
            .flag(spell.config(), "ManaLeechBasedOnElementalDamage")
    );
    close(spell.output().mana_leech_gain_per_hit, 0.0); // Its ring still grants Attack leech only.

    // Reuse the actual mapped flags and parsed item source with controlled hit
    // components so the per-hit, sustained, panel and detail arithmetic is exact.
    let mixed = [1000.0, 2000.0, 3000.0, 4000.0, 5000.0];
    for (damage, expected) in [
        ([1000.0, 0.0, 0.0, 0.0, 0.0], 0.0),
        ([0.0, 2000.0, 3000.0, 4000.0, 0.0], 783.0),
        (mixed, 783.0),
        ([0.0, 0.0, 0.0, 0.0, 5000.0], 0.0),
        // Whole hit = 160k, including ineligible Physical and Chaos. Scale all
        // types by 1/4: 40k elemental * 1/4 * 8.7% = 870 Mana, not 3480.
        ([20000.0, 20000.0, 10000.0, 10000.0, 100000.0], 870.0),
    ] {
        assert_mana_instance(
            &controlled_hit(&flags, &leech, CalcConfig::attack(), damage),
            expected,
        );
    }
    assert_mana_instance(
        &controlled_hit(&flags, &leech, CalcConfig::spell(), mixed),
        0.0,
    );
    assert_mana_instance(
        &controlled_hit(&flags, &[], CalcConfig::attack(), mixed),
        0.0,
    );
    for (name, percent, expected) in [
        ("ManaLeech", 8.7, 783.0),
        ("PhysicalDamageManaLeech", 8.7, 783.0),
        ("DamageManaLeech", 1.0, 140.0), // Physical excluded; existing Chaos eligibility retained.
        ("ElementalDamageManaLeech", 1.0, 90.0),
        ("ChaosDamageManaLeech", 1.0, 50.0),
    ] {
        let source = [Modifier::number(name, ModType::Base, percent).with_flags(ModFlags::SPELL)];
        assert_mana_instance(
            &controlled_hit(&flags, &source, CalcConfig::spell(), mixed),
            expected,
        );
        assert_mana_instance(
            &controlled_hit(&flags, &source, CalcConfig::attack(), mixed),
            0.0,
        );
    }
    for ghost in [false, true] {
        let mut sources = leech.clone();
        sources.extend([
            Modifier::number("PhysicalDamageLifeLeech", ModType::Base, 2.0),
            Modifier::number("PhysicalDamageEnergyShieldLeech", ModType::Base, 1.0),
        ]);
        if ghost {
            sources.push(Modifier::flag("GhostReaver"));
        }
        let result = controlled_hit(&flags, &sources, CalcConfig::attack(), mixed);
        assert_mana_instance(&result, 783.0);
        close(
            result.output().life_leech_gain_per_hit,
            if ghost { 0.0 } else { 20.0 },
        );
        close(
            result.output().energy_shield_leech_gain_per_hit,
            if ghost { 30.0 } else { 10.0 },
        );
    }
}
