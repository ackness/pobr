use pobr_core::calc::{
    AttributeInfusion, CalculationSession, MinimalInput, MinionOrigin, MinionSkill,
};
use pobr_core::{CalcConfig, ModTag, Modifier};
use pobr_data::{minion::minion_def_zombie, prelude::*, skill::SkillTypes};

const GAIN: &str =
    "Companions in your Presence Gain 20% of Damage as Extra Damage of a random Element";
const RESIST: &str = "Companions in your Presence have -20% to all Elemental Resistances";

fn session(pair: bool, exclusion: &str, effect: f64) -> CalculationSession {
    // Even with a Companion summoning skill selected on the player, these
    // aura modifiers must only reach that skill's minions.
    let mut cfg = CalcConfig::attack()
        .with_skill_types(SkillTypes::from_pob2_name("Companion").unwrap())
        .with_mode_buffs(true);
    if exclusion == "no_presence" {
        cfg.multipliers.insert("MinionPresenceCount".into(), 0.0);
    }
    let mut s = CalculationSession::new(MinimalInput {
        base_life: 1000.0,
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
        ..Default::default()
    })
    .with_config(cfg);
    if pair {
        for (text, id) in [(GAIN, "rune.gain"), (RESIST, "rune.resist")] {
            let parsed = crate::support::parse_mod(text).unwrap();
            assert!(parsed.is_complete(), "{text}: {parsed:?}");
            s.add_modifiers(parsed.mods.into_iter().map(|mut m| {
                m = m.with_source("Stoic Sceptre rune").with_origin(
                    ModifierSource::new(SourceId::new(SourceKind::Item, id)).with_raw_text(text),
                );
                if exclusion == "inactive_grant" {
                    m.tags.push(ModTag::condition("InactiveRune", false));
                }
                m
            }));
        }
    }
    if exclusion == "cannot_share" {
        s.add_modifiers([Modifier::flag("SelfAurasCannotAffectAllies")]);
    }
    // The existing ExtraAura convention scales with recipient effect only.
    s.add_modifiers([Modifier::number("BuffEffectOnSelf", ModType::Inc, 100.0)]);
    for companion in [true, false] {
        let summoner_types = if companion {
            SkillTypes::MINION | SkillTypes::from_pob2_name("Companion").unwrap()
        } else {
            SkillTypes::MINION
        };
        let granted = s.minion_modifiers_for(summoner_types);
        if !companion {
            assert!(granted.is_empty());
        }
        let mut local = vec![Modifier::number("BuffEffectOnSelf", ModType::Inc, effect)];
        if exclusion == "hidden" && companion {
            local.push(Modifier::flag("HiddenMonster"));
        }
        let mut def = minion_def_zombie();
        def.fire_resist = 40.0;
        def.cold_resist = 45.0;
        def.lightning_resist = 50.0;
        s.add_minion_from_def(
            &def,
            20,
            1,
            granted,
            local,
            AttributeInfusion::default(),
            MinionOrigin {
                is_companion: companion,
                skill: MinionSkill {
                    id: "MinionMeleeStep".into(),
                    // The damaging skill is not itself a Companion skill.
                    types: vec!["Attack".into(), "Melee".into()],
                    ..Default::default()
                },
                ..Default::default()
            },
        );
    }
    s.perform_minimal().unwrap();
    s
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-8, "{actual} != {expected}");
}

#[test]
fn rune_pair_grants_average_elemental_damage_and_resistance_only_to_companions() {
    let base = session(false, "", 0.0);
    let s = session(true, "", 0.0);
    let companion = s.minion_output(0).unwrap();
    close(companion.dps, base.minion_output(0).unwrap().dps * 1.2);
    let physical = companion
        .damage_components
        .iter()
        .find(|part| part.damage_type == DamageType::Physical)
        .unwrap();
    for ty in [DamageType::Fire, DamageType::Cold, DamageType::Lightning] {
        let extra = companion
            .damage_components
            .iter()
            .find(|part| part.damage_type == ty)
            .unwrap();
        close(extra.min, physical.min * 0.2 / 3.0);
        close(extra.max, physical.max * 0.2 / 3.0);
    }
    for (stat, value) in [
        ("FireResist", companion.fire_resistance),
        ("ColdResist", companion.cold_resistance),
        ("LightningResist", companion.lightning_resistance),
    ] {
        close(
            value,
            base.minion_stat_formula(0, stat).unwrap().value - 20.0,
        );
        close(s.minion_stat_formula(0, stat).unwrap().value, value);
    }
    close(
        s.minion_stat_formula(0, "TotalDPS").unwrap().value,
        companion.dps,
    );
    close(s.output().dps, base.output().dps);
    close(s.output().fire_resistance, base.output().fire_resistance);
    close(
        s.minion_output(1).unwrap().dps,
        base.minion_output(1).unwrap().dps,
    );
    close(
        s.minion_output(1).unwrap().fire_resistance,
        base.minion_output(1).unwrap().fire_resistance,
    );
    for (stat, value, id, text) in [
        ("DamageGainAsRandom", 20.0, "rune.gain", GAIN),
        ("ElementalResist", -20.0, "rune.resist", RESIST),
    ] {
        let mods = s.minion_stat_mods(0, stat);
        assert_eq!(mods.len(), 1);
        assert!(mods[0].active);
        assert_eq!(mods[0].value, Some(value));
        let origin = mods[0].modifier.origin.as_ref().unwrap();
        assert_eq!(origin.source_id, SourceId::new(SourceKind::Item, id));
        assert_eq!(origin.raw_text.as_deref(), Some(text));
        assert!(
            mods[0]
                .modifier
                .source
                .as_deref()
                .is_some_and(|source| source.eq_ignore_ascii_case(text))
        );
        assert!(s.minion_stat_mods(1, stat).is_empty());
        assert!(s.mods_named(stat).is_empty());
    }
}

#[test]
fn rune_pair_obeys_presence_ally_block_hidden_and_inactive_grant() {
    let base = session(false, "", 0.0);
    for exclusion in ["no_presence", "cannot_share", "hidden", "inactive_grant"] {
        let s = session(true, exclusion, 0.0);
        for index in [0, 1] {
            close(
                s.minion_output(index).unwrap().dps,
                base.minion_output(index).unwrap().dps,
            );
            close(
                s.minion_output(index).unwrap().fire_resistance,
                base.minion_output(index).unwrap().fire_resistance,
            );
            assert!(s.minion_stat_mods(index, "DamageGainAsRandom").is_empty());
            assert!(s.minion_stat_mods(index, "ElementalResist").is_empty());
        }
        close(s.output().dps, base.output().dps);
    }
}

#[test]
fn rune_pair_uses_existing_recipient_aura_effect_once() {
    let base = session(false, "", 25.0);
    let s = session(true, "", 25.0);
    close(
        s.minion_output(0).unwrap().dps,
        base.minion_output(0).unwrap().dps * 1.25,
    );
    close(
        s.minion_output(0).unwrap().fire_resistance,
        base.minion_output(0).unwrap().fire_resistance - 25.0,
    );
    close(s.minion_stat_totals(0, "DamageGainAsRandom").0, 25.0);
    close(s.minion_stat_totals(0, "ElementalResist").0, -25.0);
    close(
        s.minion_stat_formula(0, "TotalDPS").unwrap().value,
        s.minion_output(0).unwrap().dps,
    );
}
