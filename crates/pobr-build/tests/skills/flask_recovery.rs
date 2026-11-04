//! Reverie item lines and the real Azmeri Brew node reach the utility path.

use pobr_build::usage::{LineStatus, ModifierStatus, line_usage};
use pobr_build::{DataOrchestratorOptions, diagnose, parse_build};

#[test]
fn reverie_and_azmeri_use_equipped_flask_bases_and_preserve_recipient_diagnostic() {
    let data = crate::test_data();
    for rarity in ["NORMAL", "UNIQUE"] {
        for selected in [false, true] {
            for azmeri in [false, true] {
                let nodes = if azmeri { "47312" } else { "" };
                let name = if rarity == "UNIQUE" {
                    "Fixture Unique\n"
                } else {
                    ""
                };
                let xml = format!(
                    r#"<PathOfBuilding2>
<Build level="80" className="Huntress" ascendClassName="Amazon"/>
<Config><Input name="resistancePenalty" number="0"/></Config>
<Tree activeSpec="1"><Spec nodes="{nodes}" treeVersion="0_5"/></Tree>
<Items activeItemSet="1">
<Item id="1">Rarity: UNIQUE
Reverie
Shaman Mantle
Implicits: 0
Cannot use Life Flasks
Non-Unique Life Flasks apply their Effects constantly
Recovery from Life Flasks cannot be Instant
Recovery from your Life Flasks cannot be applied to anything other than you
45% less Life Flask Recovery
</Item>
<Item id="2">Rarity: {rarity}
{name}Ultimate Life Flask
Implicits: 0
50% of Recovery applied Instantly
+40% to Fire Resistance during effect
</Item>
<Item id="3">Rarity: NORMAL
Ultimate Mana Flask
Implicits: 0
50% of Recovery applied Instantly
</Item>
<ItemSet id="1">
<Slot name="Body Armour" itemId="1"/>
<Slot name="Flask 1" itemId="2" active="{selected}"/>
<Slot name="Flask 2" itemId="3" active="true"/>
</ItemSet></Items>
</PathOfBuilding2>"#
                );
                let build = parse_build(&xml).unwrap();
                let diagnosis =
                    diagnose(&build, data, &DataOrchestratorOptions::default()).unwrap();
                let out = diagnosis.session.output();
                let near = |stat: &str, expected: f64| {
                    let actual = out.utility_stats[stat];
                    assert!(
                        (actual - expected).abs() < 1e-8,
                        "{stat}: {actual} != {expected}, {rarity}, selected={selected}, azmeri={azmeri}"
                    );
                };
                // Real 0.5.5.5 bases: Ultimate Life = 920 / 3s, Mana = 310 / 3s.
                near("Flask1Recovery", 506.0);
                near("Flask1RecoveryPerSecond", 506.0 / 3.0);
                near("Flask1InstantRecovery", 0.0);
                near("Flask1Uses", 0.0);
                near("Flask2Uses", 7.0);
                near("Flask1ManaRecovery", if azmeri { 506.0 } else { 0.0 });
                near("Flask1ManaInstantRecovery", 0.0);
                near("Flask2LifeRecovery", if azmeri { 310.0 } else { 0.0 });
                near(
                    "Flask2LifeInstantRecovery",
                    if azmeri { 155.0 } else { 0.0 },
                );
                assert_eq!(
                    diagnosis.session.config().condition("UsingLifeFlask"),
                    rarity != "UNIQUE"
                );
                // Campaign rewards also contribute FireResistance. Isolate
                // the flask's merged payload rather than the character total.
                let flask_fire: f64 = diagnosis
                    .session
                    .mods_named("FireResistance")
                    .iter()
                    .filter(|modifier| {
                        modifier
                            .origin
                            .as_ref()
                            .is_some_and(|origin| origin.source_id.id == "flask.flask1")
                    })
                    .filter_map(|modifier| modifier.value.as_number())
                    .sum();
                assert_eq!(flask_fire, if rarity == "UNIQUE" { 0.0 } else { 40.0 });
                let usage = line_usage(&diagnosis, &build, data);
                let carrier = usage
                    .lines
                    .iter()
                    .filter(|row| row.source_id.as_deref() == Some("flask.flask1"))
                    .flat_map(|row| &row.modifiers)
                    .find(|modifier| {
                        matches!(modifier.name.as_str(), "FlaskBuff" | "InactiveFlaskBuff")
                    })
                    .expect("flask payload");
                assert_eq!(
                    carrier.status,
                    if rarity == "UNIQUE" {
                        ModifierStatus::Inactive
                    } else {
                        ModifierStatus::Applied
                    },
                    "{rarity} selected={selected}: {carrier:?}"
                );
                let row = usage.lines.iter().find(|row| row.text.as_deref() == Some("Recovery from your Life Flasks cannot be applied to anything other than you")).unwrap();
                assert_eq!(row.status, LineStatus::Unmodelled, "{row:?}");
            }
        }
    }
}

#[test]
fn unselected_flask_without_constant_effect_is_not_an_applied_payload() {
    let data = crate::test_data();
    let build = parse_build(
        r#"<PathOfBuilding2>
<Build level="80" className="Witch"/>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
Ultimate Life Flask
Implicits: 0
+40% to Fire Resistance during effect
</Item><ItemSet id="1"><Slot name="Flask 1" itemId="1" active="false"/></ItemSet></Items>
</PathOfBuilding2>"#,
    )
    .unwrap();
    let diagnosis = diagnose(&build, data, &Default::default()).unwrap();
    let usage = line_usage(&diagnosis, &build, data);
    let carrier = usage
        .lines
        .iter()
        .flat_map(|row| &row.modifiers)
        .find(|modifier| modifier.name == "InactiveFlaskBuff")
        .unwrap();
    assert_eq!(carrier.status, ModifierStatus::Inactive, "{carrier:?}");
    assert!(carrier.read_gate.is_some());
    assert!(carrier.used_in.is_empty());
    assert!(!diagnosis.session.config().condition("UsingLifeFlask"));
}
