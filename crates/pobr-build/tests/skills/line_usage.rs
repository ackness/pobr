//! The ledger of a build's lines (`pobr_build::usage::line_usage`): every line
//! the calculation received is listed once with what became of it, and a line
//! that left no modifier says why.

use pobr_build::usage::{LineStatus, LineUsage, ModifierStatus, UsageLine};
use pobr_build::{BuildData, DataOrchestratorOptions, diagnose, parse_build};
use pobr_gamedata::{GameData, data_version, repo_data_root};

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data")
}

/// A Witch casting Fireball, wearing a body armour and an amulet with
/// `armour` and `amulet` lines, with node 20686 (“+5% to Quality of all
/// Skills”) allocated.
fn ledger(data: &BuildData, armour: &[&str], amulet: &[&str]) -> LineUsage {
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Witch" ascendClassName="None" mainSocketGroup="1"/>
<Tree activeSpec="1"><Spec nodes="20686"/></Tree>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1"><Skill enabled="true" mainActiveSkill="1"><Gem skillId="FireballPlayer" level="20" quality="0" enabled="true"/></Skill></SkillSet></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: RARE
Test Shell
Rusted Cuirass
Implicits: 0
{}
</Item><Item id="2">Rarity: RARE
Test Beads
Gold Amulet
Implicits: 0
{}
</Item><ItemSet id="1"><Slot name="Body Armour" itemId="1"/><Slot name="Amulet" itemId="2"/></ItemSet></Items>
</PathOfBuilding2>"#,
        armour.join("\n"),
        amulet.join("\n")
    );
    let build = parse_build(&xml).expect("parse synthetic build");
    let diagnosis = diagnose(&build, data, &DataOrchestratorOptions::default()).expect("diagnose");
    pobr_build::usage::line_usage(&diagnosis, &build, data)
}

/// The line `text` of `source`, which must be listed once.
fn line<'a>(ledger: &'a LineUsage, source: &str, text: &str) -> &'a UsageLine {
    let found: Vec<&UsageLine> = ledger
        .lines
        .iter()
        .filter(|line| {
            line.source_id.as_deref() == Some(source) && line.text.as_deref() == Some(text)
        })
        .collect();
    assert_eq!(found.len(), 1, "{source} | {text}: {found:#?}");
    found[0]
}

#[test]
fn config_coverage_reports_reads_even_without_a_configured_value() {
    let data = data();
    let usage = ledger(
        &data,
        &[],
        &["50% increased Damage with Hits against Frozen Enemies"],
    );
    // This item waits for Frozen even when that config checkbox is absent.
    assert!(usage.referenced_variables.contains("EnemyFrozen"));
    // The rage and trigger passes query absent inputs too; there need not
    // be a stored Multiplier:TriggerChance modifier to record the query.
    assert!(usage.condition_reads.contains("CanGainRage"));
    assert!(usage.multiplier_reads.contains("TriggerChance"));
    assert!(!usage.reads.contains_key("Multiplier:TriggerChance"));
}

#[test]
fn focused_thrust_requires_spear_projectile_hits_and_configured_distance() {
    let data = data();
    // Focused Thrust (9227): 40% increased Projectile Damage with Spears
    // while no enemies are within 3 m. enemyDistance uses 10 units per metre:
    // unset/20/29.9 is inactive, 30 and 40 are active. The other 75% melee
    // line is separate and must not leak this projectile bonus into melee.
    let run = |skill: &str, weapon: &str, distance: Option<f64>, line: &str, reference: bool| {
        let config = distance.map_or_else(String::new, |distance| {
            format!(r#"<Input name="enemyDistance" number="{distance}"/>"#)
        });
        let build = parse_build(&format!(
            r#"<PathOfBuilding2>
<Build level="80" className="Huntress" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="{skill}" level="20" enabled="true"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
{weapon}
Implicits: 0
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/></ItemSet></Items>
<Config>{config}</Config></PathOfBuilding2>"#
        ))
        .unwrap();
        let mut options = DataOrchestratorOptions {
            mode_effective: true,
            extra_modifier_texts: [line]
                .into_iter()
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect(),
            ..Default::default()
        };
        if reference {
            options = options.on_reference_basis();
        }
        let diagnosis = diagnose(&build, &data, &options).unwrap();
        let usage = pobr_build::usage::line_usage(&diagnosis, &build, &data);
        (diagnosis.session, usage)
    };
    for (amount, metres, distance) in [
        (40, 3.0, None),
        (40, 3.0, Some(20.0)),
        (40, 3.0, Some(29.9)),
        (40, 3.0, Some(30.0)),
        (40, 3.0, Some(40.0)),
        (60, 4.5, Some(40.0)),
        (60, 4.5, Some(45.0)),
    ] {
        let line = format!(
            "{amount}% increased Projectile Damage with Spears while there are no Enemies within {metres}m"
        );
        let (plain, _) = run("TwisterPlayer", "Hardwood Spear", distance, "", false);
        let (bonus, usage) = run("TwisterPlayer", "Hardwood Spear", distance, &line, false);
        let (reference, _) = run("TwisterPlayer", "Hardwood Spear", distance, &line, true);
        let active = distance.is_some_and(|d| d >= metres * 10.0);
        let expected_hit = if active {
            run(
                "TwisterPlayer",
                "Hardwood Spear",
                distance,
                &format!("{amount}% increased Projectile Damage with Hits"),
                false,
            )
            .0
            .output()
            .total_hit_avg
        } else {
            plain.output().total_hit_avg
        };
        assert!(plain.output().total_hit_avg > 0.0);
        assert!((bonus.output().total_hit_avg - expected_hit).abs() < 1e-6);
        assert_eq!(
            reference.output().total_hit_avg,
            plain.output().total_hit_avg
        );
        let damage = bonus
            .mod_db()
            .named("Damage")
            .find(|modifier| {
                modifier.tags.iter().any(|tag| {
                    matches!(
                        tag,
                        pobr_core::ModTag::MultiplierThreshold { var, .. } if var == "enemyDistance"
                    )
                })
            })
            .unwrap();
        assert!(!damage.matches(bonus.config()));
        let hit_cfg = bonus.config().clone().with_keyword_flags(
            bonus.config().keyword_flags | pobr_data::modifier::KeywordFlags::HIT,
        );
        assert_eq!(damage.matches(&hit_cfg), active);
        let row = usage
            .lines
            .iter()
            .find(|row| row.text.as_deref() == Some(&line))
            .unwrap();
        assert_eq!(row.modifiers.len(), 1, "{row:?}");
        let modifier = &row.modifiers[0];
        assert_eq!(modifier.name, "Damage");
        if active {
            assert!(bonus.output().total_hit_avg > plain.output().total_hit_avg);
            assert_eq!(modifier.status, ModifierStatus::Applied, "{row:?}");
            assert!(modifier.read_gate.is_none());
        } else {
            assert_eq!(modifier.status, ModifierStatus::Inactive, "{row:?}");
            assert_eq!(
                modifier
                    .read_gate
                    .as_ref()
                    .unwrap()
                    .config_option
                    .as_deref(),
                Some("enemyDistance")
            );
        }
    }
    let line = "40% increased Projectile Damage with Spears while there are no Enemies within 3m";
    for (skill, weapon) in [
        ("MeleePlayer", "Hardwood Spear"),
        ("IceShotPlayer", "Heavy Bow"),
        ("FireballPlayer", "Hardwood Spear"),
    ] {
        for distance in [20.0, 40.0] {
            let (plain, _) = run(skill, weapon, Some(distance), "", false);
            let (bonus, usage) = run(skill, weapon, Some(distance), line, false);
            assert_eq!(bonus.output().total_hit_avg, plain.output().total_hit_avg);
            let row = usage
                .lines
                .iter()
                .find(|row| row.text.as_deref() == Some(line))
                .unwrap();
            assert_eq!(row.modifiers[0].status, ModifierStatus::Inactive, "{row:?}");
            assert!(row.modifiers[0].read_gate.is_none(), "{row:?}");
        }
    }
}

#[test]
fn conditional_offence_limits_report_the_missing_models() {
    let data = data();
    let cases = [
        (
            "Pin Enemies which are Primed for Pinning",
            "primed_pin_unmodelled",
        ),
        ("25% increased Daze Duration", "daze_duration_unmodelled"),
        (
            "Attacks have 25% chance to Maim on Hit",
            "attack_maim_chance_unmodelled",
        ),
    ];
    let texts: Vec<_> = cases.iter().map(|(text, _)| *text).collect();
    let ledger = ledger(&data, &[], &texts);
    for (text, reason) in cases {
        let row = line(&ledger, "item.amulet.explicit", text);
        assert_eq!(row.status, LineStatus::Unmodelled, "{row:?}");
        assert!(row.reason.as_deref().unwrap().contains(reason), "{row:?}");
        assert!(row.modifiers.is_empty());
    }
}

#[test]
fn every_line_says_what_became_of_it() {
    let data = data();
    let ledger = ledger(
        &data,
        &[
            "+100 to Armour",
            "+50 to maximum Life",
            "5% increased Experience Gain",
            "Wields a feather that sings at dawn",
        ],
        &[
            "+1 to Level of all Spell Skills",
            "Adds 5 to 10 Physical Damage to Attacks",
            "16% increased Rarity of Items found",
            "Targets affected by Abyssal Wasting you inflict are Hindered",
            "+1 to Level of all Minion Skills",
            "Minions deal 10% increased Damage",
        ],
    );

    // Applied, with the stat it reaches and its value there.
    let life = line(&ledger, "item.bodyarmour.explicit", "+50 to maximum Life");
    assert_eq!(life.status, LineStatus::Applied);
    assert!(life.used_in.contains(&"Life"), "{life:#?}");
    assert_eq!(life.holder.as_deref(), Some("Rusted Cuirass"));
    assert_eq!(life.modifiers[0].applied, Some(50.0));

    // Taken before any modifier is stored, and read by what is named.
    let armour = line(&ledger, "item.bodyarmour.explicit", "+100 to Armour");
    assert_eq!(armour.status, LineStatus::Outside);
    assert!(
        armour.reason.as_deref().unwrap().contains("own defences"),
        "{armour:#?}"
    );
    let level = line(
        &ledger,
        "item.amulet.explicit",
        "+1 to Level of all Spell Skills",
    );
    assert_eq!(level.status, LineStatus::Outside);
    let reason = level.reason.as_deref().unwrap();
    assert!(reason.contains("gem levels"), "{level:#?}");
    assert!(
        reason.ends_with("raises the level of FireballPlayer"),
        "{level:#?}"
    );
    // A gem bonus of a kind no gem of the build is raises nothing.
    let minion_level = line(
        &ledger,
        "item.amulet.explicit",
        "+1 to Level of all Minion Skills",
    );
    assert_eq!(minion_level.status, LineStatus::Unread, "{minion_level:#?}");
    // A line for minions, and no minion to give it to.
    let minions = line(
        &ledger,
        "item.amulet.explicit",
        "Minions deal 10% increased Damage",
    );
    assert_eq!(minions.status, LineStatus::Unread, "{minions:#?}");
    assert_eq!(
        minions.modifiers[0].reason.as_deref(),
        Some("the build creates no minion to give it to")
    );
    // A node's gem quality line, which a rule also recognises.
    let quality = line(&ledger, "node.20686", "+5% to [Quality] of all Skills");
    assert_eq!(quality.status, LineStatus::Outside, "{quality:#?}");

    // Read, and left out for the requirement it does not meet: a spell is
    // not an attack.
    let attack = line(
        &ledger,
        "item.amulet.explicit",
        "Adds 5 to 10 Physical Damage to Attacks",
    );
    assert_eq!(attack.status, LineStatus::Inactive);
    for modifier in &attack.modifiers {
        assert_eq!(modifier.status, ModifierStatus::Inactive);
        assert_eq!(modifier.reason.as_deref(), Some("needs Attack"));
    }

    // Item rarity is a reported multiplier (`EffectiveLootRarityMod`), so the
    // line is read; quantity is the one that changes only drops.
    let rarity = line(
        &ledger,
        "item.amulet.explicit",
        "16% increased Rarity of Items found",
    );
    assert_eq!(rarity.status, LineStatus::Applied, "{rarity:#?}");
    assert_eq!(rarity.modifiers[0].status, ModifierStatus::Applied);

    // Recognised: registered as changing nothing a calculation reports, or
    // recognised by a rule that stores nothing, which is not modelled.
    let experience = line(
        &ledger,
        "item.bodyarmour.explicit",
        "5% increased Experience Gain",
    );
    assert_eq!(experience.status, LineStatus::NoEffect);
    let hindered = line(
        &ledger,
        "item.amulet.explicit",
        "Targets affected by Abyssal Wasting you inflict are Hindered",
    );
    assert_eq!(hindered.status, LineStatus::Unmodelled);

    // No rule parses it: listed with its exact source, and among the unparsed.
    let unknown = "Wields a feather that sings at dawn";
    assert_eq!(
        line(&ledger, "item.bodyarmour.explicit", unknown).status,
        LineStatus::Unparsed
    );
    assert!(ledger.unparsed.iter().any(|line| line.text == unknown));

    // The counts are of the lines listed.
    let n = &ledger.summary;
    assert_eq!(
        n.lines,
        n.lines_applied
            + n.lines_partial
            + n.lines_inactive
            + n.lines_unread
            + n.lines_outside
            + n.lines_no_effect
            + n.lines_unmodelled
            + n.lines_unparsed
            + n.lines_no_modifier
    );
    assert_eq!(n.lines, ledger.lines.len());

    // A config option sets conditions and multipliers. One nothing in this
    // build asks for has no effect here; it is not a line left unread.
    let repeat: Vec<&UsageLine> = ledger
        .lines
        .iter()
        .filter(|line| line.source_id.as_deref() == Some("config.repeatMode"))
        .collect();
    assert!(!repeat.is_empty(), "the default repeat mode is listed");
    for line in repeat {
        assert_eq!(line.status, LineStatus::NoEffect, "{line:#?}");
        let reason = line.modifiers[0].reason.as_deref().unwrap();
        assert!(reason.contains("no calculation stage asks"), "{reason}");
    }
}

/// "Rarity of Items found" feeds the `EffectiveLootRarityMod` view, so the line
/// is applied; "Quantity of Items found" (`LootQuantity`) still changes no
/// number the calculation reports, and the ledger calls that line `no_effect`
/// rather than `unread`, so the gap queue leaves it alone.
#[test]
fn a_rarity_line_is_applied_and_a_quantity_line_has_no_calculated_effect() {
    let data = data();
    let ledger = ledger(
        &data,
        &[
            "15% increased Rarity of Items found",
            "10% increased Quantity of Items found",
        ],
        &["+10 to maximum Life"],
    );
    let rarity = line(
        &ledger,
        "item.bodyarmour.explicit",
        "15% increased Rarity of Items found",
    );
    assert_eq!(rarity.status, LineStatus::Applied, "{rarity:#?}");
    assert_eq!(rarity.modifiers[0].name, "LootRarity");
    assert_eq!(
        pobr_build::breakdown::detail::detail_kind("EffectiveLootRarityMod"),
        pobr_build::breakdown::detail::DetailKind::Sources
    );
    let quantity = line(
        &ledger,
        "item.bodyarmour.explicit",
        "10% increased Quantity of Items found",
    );
    assert_eq!(quantity.status, LineStatus::NoEffect, "{quantity:#?}");
    assert_eq!(quantity.modifiers.len(), 1, "{quantity:#?}");
    assert_eq!(quantity.modifiers[0].name, "LootQuantity");
    assert_eq!(quantity.modifiers[0].status, ModifierStatus::Unread);
    let reason = quantity.reason.as_deref().unwrap();
    assert!(
        reason.contains("item quantity changes drops, not a calculated stat"),
        "{reason}"
    );
}

#[test]
fn thresholds_recovery_and_hit_debuff_lines_reach_display_outputs() {
    let data = data();
    let lines = [
        "40% increased Elemental Ailment Threshold",
        "Gain additional Ailment Threshold equal to 17% of maximum Energy Shield",
        "Recover 2% of maximum Life on Kill",
        "Recover 3% of maximum Mana on Kill",
        "Recover 4% of maximum Energy Shield on Kill",
        "50% increased Freeze Buildup",
        "10% chance to Daze on Hit",
        "50% increased effect of Fully Broken Armour",
    ];
    let mut armour = vec!["Adds 10 to 20 Cold Damage to Spells"];
    armour.extend(lines);
    let ledger = ledger(&data, &armour, &["+2000 to maximum Energy Shield"]);
    for text in lines {
        let row = line(&ledger, "item.bodyarmour.explicit", text);
        assert_eq!(row.status, LineStatus::Applied, "{row:#?}");
    }
    for id in [
        "AilmentThreshold",
        "LifeOnKill",
        "ManaOnKill",
        "EnergyShieldOnKill",
        "FreezeBuildupPct",
        "DazeChance",
        "FullyBrokenArmourEffect",
    ] {
        assert_eq!(
            pobr_build::breakdown::detail::detail_kind(id),
            pobr_build::breakdown::detail::DetailKind::Sources,
            "{id}"
        );
    }
}

/// The named registry is exhaustively exercised with game text. The slow is
/// 50% (the game's default maximum Chill); Mail Coat reduces potency by 30%.
/// Archon passives grant 10% movement, 20% resistance and 50% reduced effect.
/// Damage and ailment readers require a damaging skill, unlike Mana Remnants.
#[test]
fn every_registered_state_reader_is_inactive_off_and_applied_on() {
    use pobr_build::calc_orchestrator::READ_UNDER_A_STATE;
    let data = data();
    for &(name, ref gate) in READ_UNDER_A_STATE {
        let text = match name {
            "BleedFaster" | "IgniteFaster" | "PoisonFaster" => {
                "Damaging Ailments deal damage 20% faster"
            }
            "FireDamage" => "40% increased Fire Damage",
            "LightningDamage" => "40% increased Lightning Damage",
            "AilmentMagnitude" => "40% increased Magnitude of Ailments you inflict",
            "EnergyShieldRecharge" => "50% increased Energy Shield Recharge Rate",
            "EnergyShieldRechargeFaster" => "20% faster start of Energy Shield Recharge",
            "SelfShockEffect" => "40% reduced effect of Shock on you",
            "SelfChillEffectIsReversed" => "The Effect of Chill on you is reversed",
            "UnaffectedBySlows" => "Your speed is unaffected by Slows",
            "SlowEffectOnSelf" => "30% reduced Slowing Potency of Debuffs on You",
            "ArchonMovementSpeed" => "Archon Buffs also grant 10% increased Movement Speed",
            "ArchonElementalResistance" => {
                "Archon Buffs also grant +20% to all Elemental Resistances"
            }
            "ArchonEffect" => "50% reduced effect of Archon Buffs on you",
            "FlaskChargeOnKillChance" => "25% Chance to gain a Charge when you kill an enemy",
            _ => panic!("new registered reader {name} needs an off/on fixture"),
        };
        for active in [false, true] {
            let damage_reader = matches!(
                name,
                "FireDamage"
                    | "LightningDamage"
                    | "AilmentMagnitude"
                    | "BleedFaster"
                    | "IgniteFaster"
                    | "PoisonFaster"
            );
            let skill = if damage_reader && !active {
                "ManaRemnantsPlayer"
            } else if name == "LightningDamage" {
                "ArcPlayer"
            } else {
                "FireballPlayer"
            };
            let is_charge_on_kill = name == "FlaskChargeOnKillChance";
            let config = gate
                .config_option
                .as_deref()
                .map(|option| {
                    if option == "enemyKillsPerSecond" {
                        // Count-type kill rate: absent when off, 2/s when on.
                        if active {
                            r#"<Config><Input name="enemyKillsPerSecond" number="2"/></Config>"#.to_string()
                        } else {
                            String::new()
                        }
                    } else {
                        // An explicit 50% Chill with 40% reduced effect slows
                        // actions by 30%; the state alone supplies no magnitude.
                        let magnitude = if option == "conditionChilled" {
                            r#"<Input name="conditionChilledEffect" number="50"/>"#
                        } else {
                            ""
                        };
                        format!(
                            r#"<Config><Input name="{option}" boolean="{active}"/>{magnitude}</Config>"#
                        )
                    }
                })
                .unwrap_or_default();
            let build = if is_charge_on_kill {
                parse_build(&format!(
                    r#"<PathOfBuilding2>
<Build level="80" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="{skill}" level="20" enabled="true"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: MAGIC
Undiluted Ultimate Life Flask
Implicits: 0
{text}
</Item><ItemSet id="1"><Slot name="Flask 1" itemId="1" active="true"/></ItemSet></Items>
{config}</PathOfBuilding2>"#
                ))
                .unwrap()
            } else {
                parse_build(&format!(
                    r#"<PathOfBuilding2>
<Build level="80" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="{skill}" level="20" enabled="true"/></Skill></Skills>
{config}</PathOfBuilding2>"#
                ))
                .unwrap()
            };
            let mut options = DataOrchestratorOptions {
                extra_modifier_texts: if is_charge_on_kill {
                    Vec::new()
                } else {
                    vec![text.to_string()]
                },
                ..Default::default()
            };
            if active && matches!(name, "UnaffectedBySlows" | "SlowEffectOnSelf") {
                options
                    .extra_modifier_texts
                    .push("50% reduced Action Speed".to_string());
            }
            if active && matches!(name, "EnergyShieldRecharge" | "EnergyShieldRechargeFaster") {
                options
                    .extra_modifier_texts
                    .push("+100 to maximum Energy Shield".into());
            }
            if active && matches!(name, "BleedFaster" | "PoisonFaster") {
                options.extra_modifier_texts.extend([
                    "Adds 100 to 100 Physical Damage to Spells".into(),
                    "100% chance to Bleed on Hit".into(),
                    "100% chance to Poison on Hit".into(),
                ]);
            }
            let diagnosis = diagnose(&build, &data, &options).unwrap();
            let ledger = pobr_build::usage::line_usage(&diagnosis, &build, &data);
            let modifier = ledger
                .lines
                .iter()
                .flat_map(|line| &line.modifiers)
                .find(|modifier| {
                    if is_charge_on_kill {
                        modifier.name.ends_with(":FlaskChargeOnKillChance")
                    } else {
                        modifier.name == name
                    }
                })
                .unwrap();
            assert_eq!(
                modifier.status,
                if active {
                    ModifierStatus::Applied
                } else {
                    ModifierStatus::Inactive
                },
                "{name} active={active}: {modifier:?}"
            );
            assert_eq!(modifier.read_gate.is_some(), !active, "{name}");
            if active {
                assert!(
                    if is_charge_on_kill {
                        ledger
                            .reads
                            .keys()
                            .any(|key| key.ends_with(":FlaskChargeOnKillChance"))
                    } else {
                        ledger.reads.contains_key(name)
                    },
                    "{name}: reader must actually run"
                );
            } else {
                assert!(!diagnosis.unread_modifiers.iter().any(|modifier| {
                    if is_charge_on_kill {
                        modifier.name.ends_with(":FlaskChargeOnKillChance")
                    } else {
                        modifier.name == name
                    }
                }));
                assert!(modifier.reason.as_deref().unwrap().contains(&*gate.state));
            }
        }
    }
}

#[test]
fn curse_on_self_count_gates_per_curse_damage() {
    // Game stat damage_+%_per_active_curse_on_self (Warding Fetish passive
    // 31925): 30% per curse, so 2 curses is 60% increased Damage.
    let data = data();
    let text = "30% increased Damage per Curse on you";
    for curses in [None, Some(2.0)] {
        let config = curses
            .map(|n| {
                format!(r#"<Config><Input name="multiplierCursesOnSelf" number="{n}"/></Config>"#)
            })
            .unwrap_or_default();
        let build = parse_build(&format!(
            r#"<PathOfBuilding2>
<Build level="80" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="20" enabled="true"/></Skill></Skills>
{config}</PathOfBuilding2>"#
        ))
        .unwrap();
        let diagnosis = diagnose(
            &build,
            &data,
            &DataOrchestratorOptions {
                extra_modifier_texts: vec![text.to_string()],
                ..Default::default()
            },
        )
        .unwrap();
        let ledger = pobr_build::usage::line_usage(&diagnosis, &build, &data);
        let row = ledger
            .lines
            .iter()
            .find(|row| row.text.as_deref() == Some(text))
            .unwrap();
        let modifier = &row.modifiers[0];
        assert_eq!(modifier.name, "Damage", "{row:?}");
        match curses {
            None => {
                assert_eq!(modifier.status, ModifierStatus::Inactive, "{row:?}");
                assert_eq!(
                    modifier.reason.as_deref(),
                    Some("scaled to 0: Multiplier CurseOnSelf is 0"),
                    "{row:?}"
                );
                assert_eq!(
                    modifier
                        .read_gate
                        .as_ref()
                        .map(|gate| gate.config_option.as_deref()),
                    Some(Some("multiplierCursesOnSelf")),
                    "{modifier:?}"
                );
                let gate = modifier.read_gate.as_ref().unwrap();
                assert!(matches!(gate.state, std::borrow::Cow::Owned(_)));
                assert!(matches!(
                    gate.config_option,
                    Some(std::borrow::Cow::Owned(_))
                ));
                assert_eq!(
                    serde_json::to_value(gate).unwrap(),
                    serde_json::json!({
                        "state": "CurseOnSelf is 0",
                        "config_option": "multiplierCursesOnSelf"
                    })
                );
            }
            Some(_) => {
                assert_eq!(modifier.status, ModifierStatus::Applied, "{row:?}");
                assert_eq!(modifier.applied, Some(60.0), "{row:?}");
            }
        }
    }
}

#[test]
fn config_setters_include_conditions_already_known_to_the_engine() {
    let data = data();
    let text = "20% faster start of Energy Shield Recharge while affected by an Archon Buff";
    let ledger = ledger(&data, &["+200 to maximum Energy Shield"], &[text]);
    let row = line(&ledger, "item.amulet.explicit", text);
    assert_eq!(row.status, LineStatus::Inactive, "{row:?}");
    assert_eq!(
        row.modifiers[0].reason.as_deref(),
        Some("Condition AffectedByArchon not met")
    );
    assert_eq!(
        ledger
            .condition_configs
            .get("AffectedByArchon")
            .map(String::as_str),
        Some("conditionAffectedByArchon")
    );
    assert!(
        !ledger
            .conditions
            .iter()
            .any(|entry| entry.condition == "AffectedByArchon")
    );
}

#[test]
fn hit_requirements_report_the_unmet_enemy_condition() {
    let data = data();
    // Thin Ice: damage_+%_vs_frozen_enemies gives 50% increased hit damage.
    // Both the spell and the crossbow hit; Frozen is the missing requirement.
    let text = "50% increased Damage with Hits against Frozen Enemies";
    for skill in ["FrostboltPlayer", "MeleeCrossbowPlayer"] {
        let mut unfrozen_hit = 0.0;
        for frozen in [false, true] {
            let build = parse_build(&format!(
                r#"<PathOfBuilding2>
<Build level="80" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="{skill}" level="20" enabled="true"/></Skill></Skills>
<Items><Item id="1">Rarity: NORMAL
Makeshift Crossbow
Implicits: 0
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/></ItemSet></Items>
<Config><Input name="conditionEnemyFrozen" boolean="{frozen}"/></Config>
</PathOfBuilding2>"#
            ))
            .unwrap();
            let diagnosis = diagnose(
                &build,
                &data,
                &DataOrchestratorOptions {
                    mode_effective: true,
                    extra_modifier_texts: vec![text.to_string()],
                    ..Default::default()
                },
            )
            .unwrap();
            let hit = diagnosis.session.output().total_hit_avg;
            assert!(hit > 0.0, "{skill} produces hits");
            let ledger = pobr_build::usage::line_usage(&diagnosis, &build, &data);
            let row = ledger
                .lines
                .iter()
                .find(|row| row.text.as_deref() == Some(text))
                .unwrap();
            let modifier = &row.modifiers[0];
            if frozen {
                assert_eq!(modifier.status, ModifierStatus::Applied, "{row:?}");
                assert!(modifier.reason.is_none());
                assert!((hit - unfrozen_hit * 1.5).abs() < 1e-6);
            } else {
                unfrozen_hit = hit;
                assert_eq!(modifier.status, ModifierStatus::Inactive, "{row:?}");
                assert_eq!(
                    modifier.reason.as_deref(),
                    Some("Condition EnemyFrozen not met")
                );
                assert_eq!(
                    ledger
                        .condition_configs
                        .get("EnemyFrozen")
                        .map(String::as_str),
                    Some("conditionEnemyFrozen")
                );
            }
        }
    }
}

#[test]
fn build_and_calc_pass_conditions_are_declared_even_when_not_met() {
    let data = data();
    // Crystallised Immunities grants 100% avoidance for the majority colour;
    // this Fireball build has no supports, so none of its three states holds.
    let lines = [
        "Immune to Ignite if a majority of your Socketed Support Gems are Red",
        "Immune to Shock if a majority of your Socketed Support Gems are Green",
        "Immune to Chill if a majority of your Socketed Support Gems are Blue",
        "10% increased Damage while wielding a Bow",
        "Critical Hits deal 10% increased Damage",
        "40% increased Fire Damage while affected by Herald of Ash",
        "40% increased Lightning Damage while affected by Herald of Thunder",
    ];
    let ledger = ledger(&data, &["Adds 1 to 1 Lightning Damage to Spells"], &lines);
    for (text, condition) in lines.iter().zip([
        "MajorityRedSocketedSupports",
        "MajorityGreenSocketedSupports",
        "MajorityBlueSocketedSupports",
        "UsingBow",
        "CriticalStrike",
        "AffectedByHeraldofAsh",
        "AffectedByHeraldofThunder",
    ]) {
        let row = line(&ledger, "item.amulet.explicit", text);
        assert!(
            ledger.condition_states.contains(condition),
            "{condition}: {row:?}"
        );
        assert!(
            !ledger
                .conditions
                .iter()
                .any(|entry| entry.condition == condition)
        );
        if condition != "CriticalStrike" {
            assert_eq!(row.status, LineStatus::Inactive, "{row:?}");
            assert_eq!(
                row.modifiers[0].reason,
                Some(format!("Condition {condition} not met"))
            );
        }
    }
    assert!(!ledger.condition_states.contains("NothingSetsThis"));
}

#[test]
fn equipped_defence_lines_require_the_corresponding_offhand() {
    let data = data();
    let focus = "75% increased Energy Shield from Equipped Focus";
    let shield = "100% increased Armour, Evasion and Energy Shield from Equipped Shield";
    for (base, active_text, inactive_text, condition) in [
        ("Woven Focus", focus, shield, "UsingShield"),
        ("Splintered Tower Shield", shield, focus, "UsingFocus"),
    ] {
        // Every defence is nonzero to exercise every member of the Shield line.
        let build = parse_build(&format!(r#"<PathOfBuilding2>
<Build level="80" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="20" enabled="true"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: RARE
Test Offhand
{base}
Implicits: 0
+20 to Armour
+20 to Evasion Rating
+50 to maximum Energy Shield
</Item><ItemSet id="1"><Slot name="Weapon 2" itemId="1"/></ItemSet></Items>
</PathOfBuilding2>"#)).unwrap();
        let options = DataOrchestratorOptions {
            extra_modifier_texts: vec![focus.into(), shield.into()],
            ..Default::default()
        };
        let diagnosis = diagnose(&build, &data, &options).unwrap();
        let ledger = pobr_build::usage::line_usage(&diagnosis, &build, &data);
        for (text, status) in [
            (active_text, LineStatus::Applied),
            (inactive_text, LineStatus::Inactive),
        ] {
            let row = ledger
                .lines
                .iter()
                .find(|line| line.text.as_deref() == Some(text))
                .unwrap();
            assert_eq!(row.status, status, "{row:?}");
            if status == LineStatus::Inactive {
                for modifier in &row.modifiers {
                    assert_eq!(
                        modifier.reason.as_deref(),
                        Some(format!("Condition {condition} not met").as_str())
                    );
                }
            }
        }
        assert!(ledger.condition_states.contains(condition));
    }
}

#[test]
fn shield_defence_ledger_gates_missing_bases() {
    let data = data();
    let text = "100% increased Armour, Evasion and Energy Shield from Equipped Shield";
    fn run(data: &BuildData, text: &str, base: &str) -> LineUsage {
        let build = parse_build(&format!(r#"<PathOfBuilding2>
<Build level="80" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="20" enabled="true"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: RARE
Test Shield
{base}
Implicits: 0
</Item><ItemSet id="1"><Slot name="Weapon 2" itemId="1"/></ItemSet></Items>
</PathOfBuilding2>"#)).unwrap();
        let options = DataOrchestratorOptions {
            extra_modifier_texts: vec![text.to_string()],
            ..Default::default()
        };
        let diagnosis = diagnose(&build, data, &options).unwrap();
        pobr_build::usage::line_usage(&diagnosis, &build, data)
    }
    fn by_name<'a>(
        ledger: &'a LineUsage,
        text: &str,
        name: &str,
    ) -> &'a pobr_build::usage::UsageModifier {
        ledger
            .lines
            .iter()
            .find(|line| line.text.as_deref() == Some(text))
            .unwrap()
            .modifiers
            .iter()
            .find(|modifier| modifier.name == name)
            .unwrap()
    }

    // Armour-only shield (Splintered Tower Shield: 18 armour, no evasion or
    // energy shield): Armour applies, Evasion and Energy Shield are inactive
    // with the missing-base gate rather than dropped.
    let armour_only = run(&data, text, "Splintered Tower Shield");
    let armour = by_name(&armour_only, text, "Armour");
    assert_eq!(armour.status, ModifierStatus::Applied, "{armour:?}");
    for (name, state) in [
        ("Evasion", "the equipped shield has no base Evasion"),
        (
            "EnergyShield",
            "the equipped shield has no base Energy Shield",
        ),
    ] {
        let modifier = by_name(&armour_only, text, name);
        assert_eq!(modifier.status, ModifierStatus::Inactive, "{modifier:?}");
        let gate = modifier.read_gate.as_ref().unwrap();
        assert_eq!(&*gate.state, state, "{modifier:?}");
        assert!(gate.config_option.is_none(), "{modifier:?}");
        assert!(
            modifier.reason.as_deref().unwrap().contains(state),
            "{modifier:?}"
        );
    }

    // Armour/Evasion shield (Hardwood Targe: 10 armour, 7 evasion): Evasion
    // applies once the shield has that base.
    let mixed = run(&data, text, "Hardwood Targe");
    for name in ["Armour", "Evasion"] {
        let modifier = by_name(&mixed, text, name);
        assert_eq!(modifier.status, ModifierStatus::Applied, "{modifier:?}");
        assert!(modifier.read_gate.is_none(), "{modifier:?}");
    }
    let energy_shield = by_name(&mixed, text, "EnergyShield");
    assert_eq!(
        energy_shield.status,
        ModifierStatus::Inactive,
        "{energy_shield:?}"
    );
    assert_eq!(
        &*energy_shield.read_gate.as_ref().unwrap().state,
        "the equipped shield has no base Energy Shield"
    );
}

#[test]
fn outranked_max_loser_is_state_gated() {
    let data = data();
    // "Defend with N% of Armour" stores `ArmourDefense` BASE N-100; the hit-taken
    // stage takes the strongest (`MAX`), so 150% (BASE 50) applies and 120%
    // (BASE 20) loses without being dropped input.
    let build = parse_build(
        r#"<PathOfBuilding2>
<Build level="80" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="20" enabled="true"/></Skill></Skills>
</PathOfBuilding2>"#,
    )
    .unwrap();
    let options = DataOrchestratorOptions {
        extra_modifier_texts: vec![
            "Defend with 120% of Armour".into(),
            "Defend with 150% of Armour".into(),
        ],
        ..Default::default()
    };
    let diagnosis = diagnose(&build, &data, &options).unwrap();
    let ledger = pobr_build::usage::line_usage(&diagnosis, &build, &data);
    let row = |text: &str| {
        ledger
            .lines
            .iter()
            .find(|line| line.text.as_deref() == Some(text))
            .unwrap()
    };
    let winner = &row("Defend with 150% of Armour").modifiers[0];
    assert_eq!(winner.name, "ArmourDefense");
    assert_eq!(winner.status, ModifierStatus::Applied, "{winner:?}");
    assert!(winner.read_gate.is_none(), "{winner:?}");
    let loser = &row("Defend with 120% of Armour").modifiers[0];
    assert_eq!(loser.name, "ArmourDefense");
    assert_eq!(loser.status, ModifierStatus::Inactive, "{loser:?}");
    let gate = loser.read_gate.as_ref().unwrap();
    assert_eq!(
        &*gate.state, "a stronger modifier of the same name applies",
        "{loser:?}"
    );
    assert!(gate.config_option.is_none(), "{loser:?}");
    assert_eq!(
        loser.reason.as_deref(),
        Some("only the strongest ArmourDefense counts, and another is stronger"),
        "{loser:?}"
    );
}

#[test]
fn charm_recharge_event_stays_visible_as_unmodelled() {
    let data = data();
    let text = "Energy Shield Recharge starts on use";
    let build = parse_build(&format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Witch"/>
<Items activeItemSet="1"><Item id="1">Rarity: UNIQUE
Nascent Hope
Thawing Charm
Implicits: 1
Used when you become Frozen
{text}
</Item><ItemSet id="1"><Slot name="Charm 1" itemId="1" active="true"/></ItemSet></Items>
</PathOfBuilding2>"#
    ))
    .unwrap();
    let diagnosis = diagnose(&build, &data, &DataOrchestratorOptions::default()).unwrap();
    let ledger = pobr_build::usage::line_usage(&diagnosis, &build, &data);
    let row = line(&ledger, "flask.charm1", text);
    assert_eq!(row.status, LineStatus::Unmodelled, "{row:?}");
    assert!(row.modifiers.is_empty());
    assert!(
        row.reason
            .as_deref()
            .unwrap()
            .contains("charm_energy_shield_recharge_starts_when_used")
    );
}

#[test]
fn miscellaneous_lines_distinguish_unmodelled_no_effect_and_external_readers() {
    let data = data();
    let cases = [
        ("Grants 3 additional Skill Slots", LineStatus::NoEffect),
        ("25% increased Knockback Distance", LineStatus::NoEffect),
        ("Double Activation Delay of Curses", LineStatus::Unmodelled),
        ("25% increased Totem Life", LineStatus::Unmodelled),
        ("Grants 1 Passive Skill Point", LineStatus::Applied),
        (
            "Remembrancing 1234 songworthy deeds by the line of Vorana",
            LineStatus::Applied,
        ),
    ];
    let texts: Vec<_> = cases.iter().map(|(text, _)| *text).collect();
    let usage = ledger(&data, &[], &texts);
    for (text, expected) in cases {
        let row = line(&usage, "item.amulet.explicit", text);
        assert_eq!(row.status, expected, "{row:?}");
        if expected == LineStatus::Applied {
            assert!(
                row.modifiers
                    .iter()
                    .all(|modifier| modifier.status == ModifierStatus::Outside),
                "{row:?}"
            );
        }
    }
}

#[test]
fn consumed_power_charge_bonus_has_existing_config_controls() {
    let data = data();
    let text = "20% increased Critical Damage Bonus if you've consumed a Power Charge Recently";
    let run = |enabled: bool, minimum: u32| {
        let build = parse_build(&format!(r#"<PathOfBuilding2>
<Build level="80" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="20" enabled="true"/></Skill></Skills>
<Config><Input name="usePowerCharges" boolean="{enabled}"/><Input name="overridePowerCharges" number="1"/></Config>
</PathOfBuilding2>"#)).unwrap();
        let options = DataOrchestratorOptions {
            extra_modifier_texts: vec![text.into(), format!("+{minimum} to Minimum Power Charges")],
            ..Default::default()
        };
        let diagnosis = diagnose(&build, &data, &options).unwrap();
        let ledger = pobr_build::usage::line_usage(&diagnosis, &build, &data);
        let row = ledger
            .lines
            .iter()
            .find(|row| row.text.as_deref() == Some(text))
            .unwrap();
        let modifier = &row.modifiers[0];
        if enabled && minimum == 0 {
            assert_eq!(modifier.status, ModifierStatus::Applied, "{row:?}");
            assert_eq!(modifier.applied, Some(20.0), "{row:?}");
            assert!(modifier.read_gate.is_none(), "{row:?}");
        } else {
            assert_eq!(modifier.status, ModifierStatus::Inactive, "{row:?}");
            assert_eq!(
                modifier
                    .read_gate
                    .as_ref()
                    .unwrap()
                    .config_option
                    .as_deref(),
                Some("usePowerCharges"),
                "{row:?}"
            );
        }
        diagnosis.session.output().crit_multiplier
    };
    // critical_hit_damage_bonus_+%_if_consumed_power_charge_recently = 20.
    // Existing snapshot convention: one removable charge enables the bonus;
    // minimum charges cannot be consumed, and the bonus does not scale by count.
    let off = run(false, 0);
    assert!((run(true, 0) - (1.0 + (off - 1.0) * 1.2)).abs() < 1e-9);
    assert_eq!(run(true, 1), off);
}

#[test]
fn every_charge_multiplier_input_has_an_off_on_ledger_fixture() {
    use pobr_core::calc::survivability::{CHARGE_MULTIPLIER_INPUTS, ChargeKind};
    let data = data();
    for &(kind, _, _) in CHARGE_MULTIPLIER_INPUTS {
        let (charge, use_option, override_option) = match kind {
            ChargeKind::Power => ("Power", "usePowerCharges", "overridePowerCharges"),
            ChargeKind::Frenzy => ("Frenzy", "useFrenzyCharges", "overrideFrenzyCharges"),
            ChargeKind::Endurance => (
                "Endurance",
                "useEnduranceCharges",
                "overrideEnduranceCharges",
            ),
        };
        // Generalized parser fixtures for both count paths. At 2 charges a
        // per-charge 20% bonus is 40%; the recently-consumed bonus caps at 20%.
        let article = if kind == ChargeKind::Endurance {
            "an"
        } else {
            "a"
        };
        let texts = [
            format!("20% increased Critical Damage Bonus per {charge} Charge"),
            format!(
                "20% increased Critical Damage Bonus if you've consumed {article} {charge} Charge Recently"
            ),
        ];
        for enabled in [false, true] {
            let build = parse_build(&format!(r#"<PathOfBuilding2>
<Build level="80" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="20" enabled="true"/></Skill></Skills>
<Config><Input name="{use_option}" boolean="{enabled}"/><Input name="{override_option}" number="2"/></Config>
</PathOfBuilding2>"#)).unwrap();
            let diagnosis = diagnose(
                &build,
                &data,
                &DataOrchestratorOptions {
                    extra_modifier_texts: texts.to_vec(),
                    ..Default::default()
                },
            )
            .unwrap();
            let ledger = pobr_build::usage::line_usage(&diagnosis, &build, &data);
            for (text, amount) in texts.iter().zip([40.0, 20.0]) {
                let row = ledger
                    .lines
                    .iter()
                    .find(|row| row.text.as_deref() == Some(text))
                    .unwrap();
                let modifier = &row.modifiers[0];
                if enabled {
                    assert_eq!(modifier.status, ModifierStatus::Applied, "{row:?}");
                    assert_eq!(modifier.applied, Some(amount), "{row:?}");
                    assert!(modifier.read_gate.is_none(), "{row:?}");
                } else {
                    assert_eq!(modifier.status, ModifierStatus::Inactive, "{row:?}");
                    assert_eq!(
                        modifier
                            .read_gate
                            .as_ref()
                            .unwrap()
                            .config_option
                            .as_deref(),
                        Some(use_option),
                        "{row:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn utility_followups_keep_missing_models_visible_and_crafting_no_effect() {
    let data = data();
    let ledger = ledger(
        &data,
        &[],
        &[
            "Your Totem Limit is doubled",
            "10% chance to Blind Enemies on Hit with Attacks",
            "Corrupting will always result in change",
        ],
    );
    for text in [
        "Your Totem Limit is doubled",
        "10% chance to Blind Enemies on Hit with Attacks",
    ] {
        assert_eq!(
            line(&ledger, "item.amulet.explicit", text).status,
            LineStatus::Unmodelled
        );
    }
    assert_eq!(
        line(
            &ledger,
            "item.amulet.explicit",
            "Corrupting will always result in change"
        )
        .status,
        LineStatus::NoEffect
    );
}

#[test]
fn stun_buildup_lines_have_a_reader_and_mauling_stuns_uses_the_distance_gate() {
    let data = data();
    let text = "40% increased Stun Buildup against enemies within 2 metres";
    for (distance, expected) in [
        (10, ModifierStatus::Applied),
        (20, ModifierStatus::Applied),
        (21, ModifierStatus::Inactive),
    ] {
        let build = parse_build(&format!(r#"<PathOfBuilding2>
<Build level="80" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="20" enabled="true"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: UNIQUE
Facebreaker
Stocky Mitts
Implicits: 0
50% increased Stun Buildup
</Item><ItemSet id="1"><Slot name="Gloves" itemId="1"/></ItemSet></Items>
<Config><Input name="enemyDistance" number="{distance}"/></Config></PathOfBuilding2>"#)).unwrap();
        let options = DataOrchestratorOptions {
            mode_effective: true,
            extra_modifier_texts: vec!["15% increased Stun Buildup".into(), text.into()],
            ..Default::default()
        };
        let diagnosis = diagnose(&build, &data, &options).unwrap();
        assert!(diagnosis.session.output().heavy_stun_buildup_avg > 0.0);
        let usage = pobr_build::usage::line_usage(&diagnosis, &build, &data);
        let plain = usage
            .lines
            .iter()
            .find(|row| row.text.as_deref() == Some("15% increased Stun Buildup"))
            .unwrap();
        assert_eq!(
            plain.modifiers[0].status,
            ModifierStatus::Applied,
            "{plain:?}"
        );
        let facebreaker = line(&usage, "item.gloves.explicit", "50% increased Stun Buildup");
        assert_eq!(
            facebreaker.modifiers[0].status,
            ModifierStatus::Applied,
            "{facebreaker:?}"
        );
        let row = usage
            .lines
            .iter()
            .find(|row| row.text.as_deref() == Some(text))
            .unwrap();
        assert_eq!(row.modifiers[0].status, expected, "{row:?}");
        if expected == ModifierStatus::Inactive {
            assert_eq!(
                row.modifiers[0]
                    .read_gate
                    .as_ref()
                    .unwrap()
                    .config_option
                    .as_deref(),
                Some("enemyDistance")
            );
        }
    }
}

#[test]
fn heavy_stun_uses_loaded_boss_poise_and_the_dazed_configuration() {
    let data = data();
    assert_eq!(data.constants.boss_poise_thresholds.pinnacle, 1700);
    let run = |dazed| {
        let build = parse_build(&format!(r#"<PathOfBuilding2>
<Build level="83" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="20" enabled="true"/></Skill></Skills>
<Config><Input name="conditionEnemyDazed" boolean="{dazed}"/></Config>
</PathOfBuilding2>"#)).unwrap();
        let options = DataOrchestratorOptions {
            mode_effective: true,
            enemy_tier: pobr_data::monster::EnemyTier::Pinnacle,
            enemy_level: 83,
            ..Default::default()
        };
        let diagnosis = diagnose(&build, &data, &options).unwrap();
        let detail = diagnosis
            .session
            .derived_detail("HeavyStunBuildupAvg")
            .unwrap();
        let threshold = detail
            .inputs
            .iter()
            .find(|(k, _)| k == "EnemyPoiseThreshold")
            .unwrap()
            .1;
        // Official level 83 poise 248540, unique rarity 6x, Xesht 17x.
        assert_eq!(threshold, 25_351_080.0);
        diagnosis.session.output().heavy_stun_buildup_avg
    };
    let ordinary = run(false);
    assert!(ordinary > 0.0);
    assert!((run(true) - ordinary * 1.5).abs() < 1e-9);
}

#[test]
fn pin_source_lines_have_a_reader_and_only_enabled_types_build_pin() {
    let data = data();
    let weapon_line = "All Damage from Hits with this Weapon Contributes to Pin Buildup";
    let run = |weapon_lines: &str, extra: &[&str]| {
        let build = parse_build(&format!(
            r#"<PathOfBuilding2>
<Build level="80" className="Huntress" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="MeleePlayer" level="20" enabled="true"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: UNIQUE
Pin Test Spear
Hardwood Spear
Implicits: 0
{weapon_lines}
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/></ItemSet></Items>
</PathOfBuilding2>"#
        ))
        .unwrap();
        let options = DataOrchestratorOptions {
            mode_effective: true,
            extra_modifier_texts: extra.iter().map(|line| line.to_string()).collect(),
            ..Default::default()
        };
        let diagnosis = diagnose(&build, &data, &options).unwrap();
        let usage = pobr_build::usage::line_usage(&diagnosis, &build, &data);
        (diagnosis.session.output().pin_buildup_avg, usage)
    };
    // Nothing enables Pin: the hit builds none.
    let (none, _) = run("", &[]);
    assert_eq!(none, 0.0);
    // Physical Damage is Pinning (base_physical_damage_can_pin) and the increase
    // are both consumed by PinBuildupAvg.
    let extra = ["Physical Damage is Pinning", "30% increased Pin Buildup"];
    let (physical, usage) = run("", &extra);
    assert!(physical > 0.0, "{physical}");
    for text in extra {
        let row = usage
            .lines
            .iter()
            .find(|row| row.text.as_deref() == Some(text))
            .unwrap();
        assert_eq!(
            row.modifiers[0].status,
            ModifierStatus::Applied,
            "{text}: {row:?}"
        );
    }
    let (plain, _) = run("", &["Physical Damage is Pinning"]);
    // The boss poise threshold is large, so the shared 9-decimal rounding of the
    // per-damage percentage limits the ratio to about 1e-4.
    assert!((physical / plain - 1.3).abs() < 1e-3, "{physical} {plain}");
    // The weapon's own line enables every damage type of that hand's hits.
    let (weapon, usage) = run(weapon_line, &[]);
    assert!(weapon >= plain, "{weapon} {plain}");
    let row = line(&usage, "item.weapon1.explicit", weapon_line);
    assert_eq!(row.modifiers[0].status, ModifierStatus::Applied, "{row:?}");
    // A spell-only critical source on an attack is read but has no effect.
    let spell_line = "Physical Spell Critical Hits build Pin";
    let (spell, usage) = run("", &[spell_line]);
    assert_eq!(spell, 0.0);
    let row = usage
        .lines
        .iter()
        .find(|row| row.text.as_deref() == Some(spell_line))
        .unwrap();
    assert_ne!(row.modifiers[0].status, ModifierStatus::Unread, "{row:?}");
}

#[test]
fn self_debuff_modifier_views_read_each_previously_unmodelled_line() {
    let data = data();
    let texts = [
        "30% reduced effect of Curses on you",
        "50% reduced Duration of Bleeding on You",
        "20% reduced Duration of Ailments on You",
        "Immune to Corrupted Blood",
        "Immune to Maim",
        "Immune to Hinder",
    ];
    let usage = ledger(&data, &[], &texts);
    for text in texts {
        let row = line(&usage, "item.amulet.explicit", text);
        assert_eq!(row.status, LineStatus::Applied, "{row:?}");
        assert!(
            row.modifiers
                .iter()
                .all(|m| m.status == ModifierStatus::Applied),
            "{row:?}"
        );
    }
}

/// A flask line the parser read only in part applies nothing and is listed as
/// unparsed. These three lines parse to a wrong modifier plus leftover text:
/// "Removes 15% of Life Recovered from Mana when used" to -15 `FlaskRecovery`,
/// "88% more Recovery if used while on Low Mana" to 88% more
/// `FlaskLifeRecoveryLowLife` (Low Life, not Low Mana), and the Chaos recoup line to
/// +50 `ChaosDamage`.
#[test]
fn partially_parsed_flask_lines_are_unparsed_and_apply_nothing() {
    let data = data();
    let lines = [
        "Removes 15% of Life Recovered from Mana when used",
        "88% more Recovery if used while on Low Mana",
        "50% of Chaos damage you prevent when Hit Recouped as Life and Mana during effect",
    ];
    let build = parse_build(&format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="20" enabled="true"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: MAGIC
Undiluted Ultimate Life Flask
Implicits: 0
{}
</Item><ItemSet id="1"><Slot name="Flask 1" itemId="1" active="true"/></ItemSet></Items>
</PathOfBuilding2>"#,
        lines.join("\n")
    ))
    .unwrap();
    let diagnosis = diagnose(&build, &data, &DataOrchestratorOptions::default()).unwrap();
    let ledger = pobr_build::usage::line_usage(&diagnosis, &build, &data);
    for text in lines {
        let row = line(&ledger, "flask.flask1", text);
        assert_eq!(row.status, LineStatus::Unparsed, "{row:?}");
        assert!(row.modifiers.is_empty(), "{row:?}");
        assert!(
            diagnosis
                .unparsed_lines
                .iter()
                .any(|unparsed| unparsed == text),
            "{text} is in the unsupported list: {:?}",
            diagnosis.unparsed_lines
        );
    }
}

/// Free modifier texts (`extra_modifier_texts`) follow the same rule: the partly read
/// line "1% more Life Flask Recovery" is reported as unsupported and is not stored as a
/// 1% more maximum Life modifier; the whole-line "1% more maximum Life" is.
#[test]
fn partially_parsed_extra_text_is_unparsed_and_applies_nothing() {
    let data = data();
    let text = "1% more Life Flask Recovery";
    let build = parse_build(
        r#"<PathOfBuilding2>
<Build level="80" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="20" enabled="true"/></Skill></Skills>
</PathOfBuilding2>"#,
    )
    .unwrap();
    let with = |line: Option<&str>| {
        let options = DataOrchestratorOptions {
            extra_modifier_texts: line.map(str::to_string).into_iter().collect(),
            ..Default::default()
        };
        let session = pobr_build::calculate_with_data_session(&build, &data, &options).unwrap();
        (
            pobr_core::display_value(session.output(), "Life"),
            session.unsupported_modifier_texts().to_vec(),
        )
    };
    let (base_life, base_unsupported) = with(None);
    assert!(base_life > 0.0);
    assert!(base_unsupported.is_empty(), "{base_unsupported:?}");

    let (life, unsupported) = with(Some(text));
    assert_eq!(life, base_life, "the partial line changed Life");
    assert_eq!(unsupported, vec![text.to_string()]);

    // The same wording read completely does apply: 1% more Life.
    let (more_life, unsupported) = with(Some("1% more maximum Life"));
    assert!(unsupported.is_empty(), "{unsupported:?}");
    assert!(
        (more_life / base_life - 1.01).abs() < 0.001,
        "{more_life} / {base_life}"
    );
}
