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
fn conditional_offence_limits_report_the_missing_models() {
    let data = data();
    let cases = [
        (
            "Physical Spell Critical Hits build Pin",
            "physical_spell_critical_pin_unmodelled",
        ),
        ("15% increased Stun Buildup", "stun_buildup_unmodelled"),
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

    // Stored, and no query asks for it; it changes drops, not a stat a
    // calculation reports, so the line is registered as having no effect.
    let rarity = line(
        &ledger,
        "item.amulet.explicit",
        "16% increased Rarity of Items found",
    );
    assert_eq!(rarity.status, LineStatus::NoEffect);
    assert_eq!(rarity.modifiers[0].status, ModifierStatus::Unread);

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

/// A loot line changes no number the calculation reports: "15% increased
/// Rarity of Items found" (a Gold Ring implicit in the game) parses to
/// `LootRarity`, which no query reads, and the ledger calls the line
/// `no_effect` rather than `unread`, so the gap queue leaves it alone.
#[test]
fn a_loot_line_is_reported_as_having_no_calculated_effect() {
    let data = data();
    let ledger = ledger(
        &data,
        &["15% increased Rarity of Items found"],
        &["+10 to maximum Life"],
    );
    let rarity = line(
        &ledger,
        "item.bodyarmour.explicit",
        "15% increased Rarity of Items found",
    );
    assert_eq!(rarity.status, LineStatus::NoEffect, "{rarity:#?}");
    assert_eq!(rarity.modifiers.len(), 1, "{rarity:#?}");
    assert_eq!(rarity.modifiers[0].name, "LootRarity");
    assert_eq!(rarity.modifiers[0].status, ModifierStatus::Unread);
    let reason = rarity.reason.as_deref().unwrap();
    assert!(
        reason.contains("item rarity changes drops, not a calculated stat"),
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
            "LightRadius" => "20% increased Light Radius",
            "FireDamage" => "40% increased Fire Damage",
            "LightningDamage" => "40% increased Lightning Damage",
            "AilmentMagnitude" => "40% increased Magnitude of Ailments you inflict",
            "EnergyShieldRecharge" => "50% increased Energy Shield Recharge Rate",
            "EnergyShieldRechargeFaster" => "20% faster start of Energy Shield Recharge",
            "SelfShockEffect" => "40% reduced effect of Shock on you",
            "SelfChillEffect" => "40% reduced Effect of Chill on you",
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
            if active && name == "LightRadius" {
                options.extra_modifier_texts.push("Increases and Reductions to Light Radius also apply to Area of Effect at 50% of their value".into());
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
        (
            "20% increased Endurance, Frenzy and Power Charge Duration",
            LineStatus::Unmodelled,
        ),
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
