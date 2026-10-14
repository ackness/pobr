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
    assert!(
        level.reason.as_deref().unwrap().contains("gem levels"),
        "{level:#?}"
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

    // Stored, and no query asks for it.
    let rarity = line(
        &ledger,
        "item.amulet.explicit",
        "16% increased Rarity of Items found",
    );
    assert_eq!(rarity.status, LineStatus::Unread);

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
