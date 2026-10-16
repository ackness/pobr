//! Monster curses use their own stats and caster, sharing target curse slots.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::OutputTable;
use pobr_gamedata::{GameData, data_version, repo_data_root};

const SHAMAN: &str = "Metadata/Monsters/SerpentClanMonster/SerpentClanCaster";
const RIBRATTLE: &str = "Metadata/Monsters/PitifulFabrications/Canopy/PitifulFabrication02";

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version()))).unwrap()
}

fn calculate(
    data: &BuildData,
    summon: &str,
    monster: &str,
    toggles: &str,
    curse: &str,
    extra: &[&str],
) -> OutputTable {
    calculate_supported(data, summon, monster, toggles, curse, extra, "")
}

fn calculate_supported(
    data: &BuildData,
    summon: &str,
    monster: &str,
    toggles: &str,
    curse: &str,
    extra: &[&str],
    support: &str,
) -> OutputTable {
    let main = if monster == SHAMAN {
        "UnearthPlayer"
    } else {
        "EssenceDrainPlayer"
    };
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Witch" mainSocketGroup="1"/>
<Skills>
<Skill enabled="true"><Gem skillId="{main}" level="20" enabled="true"/></Skill>
<Skill enabled="true"><Gem skillId="{summon}" level="20" enabled="true" skillMinion="{monster}"/>{support}</Skill>
{curse}</Skills><Config>{toggles}</Config></PathOfBuilding2>"#
    );
    calculate_with_data(
        &parse_build(&xml).unwrap(),
        data,
        &DataOrchestratorOptions {
            mode_effective: true,
            enemy_tier: pobr_data::monster::EnemyTier::None,
            extra_modifier_texts: extra.iter().map(|text| text.to_string()).collect(),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn spectre_and_companion_curse_toggles_gate_the_monsters_own_curse() {
    let data = data();
    for (summon, toggle, unrelated) in [
        (
            "SummonSpectrePlayer",
            "raiseSpectreEnableCurses",
            "summonCompanionEnableCurses",
        ),
        (
            "SummonBeastPlayer",
            "summonCompanionEnableCurses",
            "raiseSpectreEnableCurses",
        ),
    ] {
        let off = format!(r#"<Input name="{toggle}" boolean="false"/>"#);
        let on = format!(
            r#"<Input name="{toggle}" boolean="true"/><Input name="{unrelated}" boolean="false"/>"#
        );
        let mods = [
            "Adds 100 to 100 Physical Damage to Spells",
            "Never deal Critical Hits",
        ];
        let plain = calculate(&data, summon, SHAMAN, &off, "", &mods);
        let cursed = calculate(&data, summon, SHAMAN, &on, "", &mods);
        assert!(
            plain
                .minions
                .iter()
                .any(|minion| minion.minion_id == SHAMAN)
        );
        assert!(plain.curse_slots.is_empty());
        assert_eq!(cursed.curse_slots, ["Vulnerability"]);
        // SerpentClanCurse: physical_damage_taken_+% = 30.
        assert!(
            cursed.dps > plain.dps,
            "plain={} cursed={}",
            plain.dps,
            cursed.dps
        );
        let scaled_player = calculate(
            &data,
            summon,
            SHAMAN,
            &on,
            "",
            &[
                "Adds 100 to 100 Physical Damage to Spells",
                "Never deal Critical Hits",
                "100% increased Effect of your Curses",
            ],
        );
        assert_eq!(
            scaled_player.dps, cursed.dps,
            "the player's curse effect cannot scale a minion's curse"
        );
    }
}

#[test]
fn ribrattle_applies_three_curses_and_the_strongest_despair_wins() {
    let data = data();
    let off = r#"<Input name="raiseSpectreEnableCurses" boolean="false"/>"#;
    let low =
        r#"<Skill enabled="true"><Gem skillId="DespairPlayer" level="1" enabled="true"/></Skill>"#;
    let high =
        r#"<Skill enabled="true"><Gem skillId="DespairPlayer" level="20" enabled="true"/></Skill>"#;
    let base = calculate(&data, "SummonSpectrePlayer", RIBRATTLE, off, "", &[]);
    let monster = calculate(&data, "SummonSpectrePlayer", RIBRATTLE, "", "", &[]);
    assert_eq!(monster.enemy_curse_limit, 3.0);
    assert_eq!(monster.curse_slots.len(), 3);
    for name in ["Despair", "Temporal Chains", "Enfeeble"] {
        assert!(monster.curse_slots.iter().any(|slot| slot == name));
    }
    assert!(monster.dps > base.dps);
    let weaker_player = calculate(
        &data,
        "SummonSpectrePlayer",
        RIBRATTLE,
        "",
        low,
        &["50% reduced Effect of your Curses"],
    );
    assert_eq!(weaker_player.dps, monster.dps, "same curse cannot stack");
    let stronger_player = calculate(
        &data,
        "SummonSpectrePlayer",
        RIBRATTLE,
        "",
        high,
        &["100% increased Effect of your Curses"],
    );
    let player_only = calculate(
        &data,
        "SummonSpectrePlayer",
        RIBRATTLE,
        off,
        high,
        &["100% increased Effect of your Curses"],
    );
    assert!(stronger_player.dps > monster.dps);
    assert_eq!(stronger_player.dps, player_only.dps);
    assert_eq!(
        stronger_player
            .curse_slots
            .iter()
            .filter(|name| *name == "Despair")
            .count(),
        1
    );
}

#[test]
fn a_support_on_the_summoning_gem_scales_each_compatible_minion_curse_once() {
    let data = data();
    let bare = calculate(&data, "SummonSpectrePlayer", RIBRATTLE, "", "", &[]);
    let support = r#"<Gem skillId="SupportCurseEffectPlayer" level="1" enabled="true"/>"#;
    let supported = calculate_supported(
        &data,
        "SummonSpectrePlayer",
        RIBRATTLE,
        "",
        "",
        &[],
        support,
    );
    assert!(supported.dps > bare.dps);
    // Heightened Curse grants 25% effect: Despair's -30 becomes -37
    // under the ordinary curse rounding rule, rather than -45 from
    // counting the selected minion skill's support twice.
    let plain = calculate(
        &data,
        "SummonSpectrePlayer",
        RIBRATTLE,
        r#"<Input name="raiseSpectreEnableCurses" boolean="false"/>"#,
        "",
        &[],
    );
    let delta = bare.dps - plain.dps;
    assert!((supported.dps - plain.dps - delta * 37.0 / 30.0).abs() < 1e-7);
}
