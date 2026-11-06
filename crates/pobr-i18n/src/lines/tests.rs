//! Parser and renderer over whole packs. The engine's own cases (ported from
//! the zh-CN translator unchanged) are in `engine.rs`.

use super::*;
use crate::LocaleMeta;

/// A synthetic Latin-script locale written only in ASCII: nothing about its
/// characters tells it apart from English, so the parser must recognize it
/// by its templates and names.
fn latin_pack() -> LocalePack {
    LocalePack::from_files(
        "xx-XX",
        [
            (
                "_meta.json",
                br#"{"shares_english_script":true}"#.as_slice(),
            ),
            (
                "stat_lines.json",
                br#"[
                    {"src":"{0:+d} zum maximalen Leben","en":"{0:+d} to maximum Life"},
                    {"src":"{0}% erhoehter Schaden","en":"{0}% increased Damage"},
                    {"src":"Verteilt {0}","en":"Allocates {0}"},
                    {"src":"{0}% increased Damage taken","en":"{0}% increased Damage taken"}
                ]"#
                .as_slice(),
            ),
            (
                "base_items.json",
                br#"{"Metadata/Items/Rings/Ring1":"Saphirring"}"#.as_slice(),
            ),
            (
                "words.json",
                br#"{"Doomgrasp":"Verhaengnisgriff"}"#.as_slice(),
            ),
            (
                "passive_names.json",
                br#"{"Grace of the Ancestors":"Anmut der Ahnen"}"#.as_slice(),
            ),
        ],
    )
    .unwrap()
}

const BASES: [(&str, &str); 1] = [("Metadata/Items/Rings/Ring1", "Sapphire Ring")];

#[test]
fn a_latin_script_locale_parses_by_its_templates() {
    let pack = latin_pack();
    let parser = LineParser::new(&[&pack], BASES).unwrap();
    assert_eq!(
        parser.parse_line("+50 zum maximalen Leben").as_deref(),
        Some("+50 to maximum Life")
    );
    assert_eq!(
        parser.parse_line("12% erhoehter Schaden").as_deref(),
        Some("12% increased Damage")
    );
    assert_eq!(
        parser.parse_line("Saphirring").as_deref(),
        Some("Sapphire Ring")
    );
    assert_eq!(
        parser.parse_line("Verhaengnisgriff").as_deref(),
        Some("Doomgrasp")
    );
    // English lines are not the locale's, even where a template is shared.
    for english in [
        "+50 to maximum Life",
        "12% increased Damage taken",
        "Sapphire Ring",
        "Rarity: Rare",
    ] {
        assert!(!parser.is_localized(english), "{english}");
        assert_eq!(parser.parse_line(english), None, "{english}");
    }
    let text =
        "Rarity: Rare\nVerhaengnisgriff\nSaphirring\n+50 zum maximalen Leben\n+5 to Strength";
    assert_eq!(
        parser.parse_text(text),
        "Rarity: Rare\nDoomgrasp\nSapphire Ring\n+50 to maximum Life\n+5 to Strength"
    );
    let english = "Rarity: Rare\nSapphire Ring\r\n+50 to maximum Life\n";
    assert!(matches!(parser.parse_text(english), Cow::Borrowed(t) if t == english));
}

#[test]
fn a_latin_script_locale_renders_english_lines() {
    let pack = latin_pack();
    let renderer = LineRenderer::new(&[&pack], BASES).unwrap();
    assert_eq!(
        renderer.render_lines([
            "+50 to maximum Life".to_string(),
            "Sapphire Ring".to_string(),
            "Allocates Grace of the Ancestors".to_string(),
            "Unknown line".to_string(),
        ]),
        [
            "+50 zum maximalen Leben",
            "Saphirring",
            "Verteilt Anmut der Ahnen",
            "Unknown line"
        ]
    );
}

#[test]
fn a_pack_without_templates_builds_no_translator() {
    let mut pack = LocalePack::new("yy").unwrap();
    pack.set_names(
        NameKind::Word,
        [("Doomgrasp".into(), "Griff".into())].into(),
    );
    assert!(LineParser::new(&[&pack], BASES).is_none());
    assert!(LineRenderer::new(&[&pack], BASES).is_none());
}

#[test]
fn earlier_packs_in_the_chain_win() {
    let primary = LocalePack::from_files(
        "xx-YY",
        [
            (
                "stat_lines.json",
                br#"[{"src":"{0:+d} zum Leben","en":"{0:+d} to maximum Life"}]"#.as_slice(),
            ),
            ("words.json", br#"{"Doomgrasp":"Griff"}"#.as_slice()),
        ],
    )
    .unwrap();
    let fallback = latin_pack();
    let renderer = LineRenderer::new(&[&primary, &fallback], BASES).unwrap();
    assert_eq!(
        renderer.render_line("+5 to maximum Life").as_deref(),
        Some("+5 zum Leben")
    );
    assert_eq!(renderer.render_line("Doomgrasp").as_deref(), Some("Griff"));
    // What the primary pack lacks comes from the fallback.
    assert_eq!(
        renderer.render_line("Sapphire Ring").as_deref(),
        Some("Saphirring")
    );
    let parser = LineParser::new(&[&primary, &fallback], BASES).unwrap();
    assert_eq!(
        parser.parse_line("+5 zum Leben").as_deref(),
        Some("+5 to maximum Life")
    );
    assert_eq!(
        parser.parse_line("+5 zum maximalen Leben").as_deref(),
        Some("+5 to maximum Life")
    );
}

#[test]
fn a_shared_localized_base_name_resolves_the_same_way_every_time() {
    let pack = LocalePack::from_files(
        "xx",
        [
            (
                "stat_lines.json",
                br#"[{"src":"{0} Leben","en":"{0} Life"}]"#.as_slice(),
            ),
            (
                "base_items.json",
                br#"{"Garb/B":"Gewand","Garb/A":"Gewand"}"#.as_slice(),
            ),
        ],
    )
    .unwrap();
    for bases in [
        [("Garb/A", "Hermit Garb"), ("Garb/B", "Anchorite Garb")],
        [("Garb/B", "Anchorite Garb"), ("Garb/A", "Hermit Garb")],
    ] {
        let parser = LineParser::new(&[&pack], bases).unwrap();
        assert_eq!(
            parser.parse_line("Gewand").as_deref(),
            Some("Anchorite Garb")
        );
    }
}

#[test]
fn a_cjk_locale_is_recognized_by_its_own_characters() {
    let pack = LocalePack::from_files(
        "zz",
        [(
            "stat_lines.json",
            r#"[{"src":"{0:+d} 生命上限","en":"{0:+d} to maximum Life"}]"#.as_bytes(),
        )],
    )
    .unwrap();
    let parser = LineParser::new(&[&pack], []).unwrap();
    // Characters only the locale uses mark the line even when no template matches it.
    assert!(parser.is_localized("生命 很多"));
    assert_eq!(parser.parse_line("生命 很多"), None);
    assert!(!parser.is_localized("+50 to maximum Life"));
    assert_eq!(
        parser.parse_text("+50 生命上限\n+5 to Strength"),
        "+50 to maximum Life\n+5 to Strength"
    );
}

#[test]
fn a_latin_script_locale_that_does_not_say_so_is_recognized_by_its_own_letters() {
    let mut pack = latin_pack();
    pack.set_meta(LocaleMeta::default());
    pack.set_stat_lines(
        [
            pack.stat_lines().to_vec(),
            vec![StatLineTemplate {
                src: "{0}% erhöhter Schaden".into(),
                en: "{0}% increased Damage".into(),
                origin: None,
            }],
        ]
        .concat(),
    );
    let parser = LineParser::new(&[&pack], BASES).unwrap();
    assert_eq!(
        parser.parse_line("12% erhöhter Schaden").as_deref(),
        Some("12% increased Damage")
    );
    // Spelled only with letters English uses: not recognized without the declaration.
    assert_eq!(parser.parse_line("Saphirring"), None);
    assert!(!may_contain_localized_text("Saphirring", false));
    assert!(may_contain_localized_text("Saphirring", true));
    assert!(may_contain_localized_text("12% erhöhter Schaden", false));
}

#[test]
fn punctuation_alone_does_not_mark_a_line_and_unchanged_text_keeps_its_bytes() {
    let pack = LocalePack::from_files(
        "zz",
        [(
            "stat_lines.json",
            r#"[{"src":"“{0}”，生命：{1}？","en":"{0} {1} Life"}]"#.as_bytes(),
        )],
    )
    .unwrap();
    let parser = LineParser::new(&[&pack], []).unwrap();
    let flavour = "“Bow before me”，she said：why？";
    assert!(!parser.is_localized(flavour));
    assert!(!may_contain_localized_text(flavour, false));
    let text = "Rarity: Unique\r\n“Bow before me”\r\n";
    assert!(matches!(parser.parse_text(text), Cow::Borrowed(t) if t == text));
    // A localized line that no template recognizes leaves the text as it was.
    let text = "Rarity: Rare\r\n生命 很多\r\n";
    assert!(parser.is_localized("生命 很多"));
    assert!(matches!(parser.parse_text(text), Cow::Borrowed(t) if t == text));
}

/// A CJK pack whose stat descriptions include ones the game writes on
/// several lines (shapes taken from the zh-CN pack's `stat_lines.json`).
fn multiline_pack() -> LocalePack {
    let templates = serde_json::json!([
        {"src": "{0:+d} 生命上限", "en": "{0:+d} to maximum Life"},
        {"src": "能量护盾无法充能", "en": "Energy Shield does not Recharge"},
        // The first line of the next template is also a template of its own.
        {"src": "受到的所有伤害在扣除生命前优先从魔力扣除",
         "en": "All Damage is taken from Mana before Life"},
        {"src": "受到的所有伤害在扣除生命前优先从魔力扣除\n魔力回复速度总降 50%",
         "en": "All Damage is taken from Mana before Life\n50% less Mana Recovery Rate"},
        {"src": "每点敌人威力值使被阻挡的物理伤害的 {0}% 吸纳为能量护盾\n能量护盾不会充能\n能量护盾不能通过再生回复",
         "en": "{0}% of Physical Damage prevented Recouped as Energy Shield per enemy Power\nEnergy Shield does not Recharge\nYou cannot Recover Energy Shield from Regeneration"},
        // Two English lines, one localized line.
        {"src": "过去 8 秒内每发射一种不同的榴弹，获得相当于伤害 {0}% 的额外火焰伤害",
         "en": "Gain {0}% of Damage as Extra Fire Damage for\nevery different Grenade fired in the past 8 seconds"},
        // The description indents its second line; item text does not.
        {"src": "总共消耗 2000 点魔力后，能量护盾充能开始，每 2 秒最多触发一次",
         "en": "Energy Shield Recharge starts after spending a total of\n 2000 Mana, no more than once every 2 seconds"},
        // One English line, two localized lines.
        {"src": "力量的固有加成改为\n每 1 点力量 +{0} 命中值",
         "en": "Inherent bonus of Strength grants +{0} to Accuracy Rating per Strength instead"},
        // The same values on both lines (a placeholder repeated across lines).
        {"src": "附加 {0} - {1} 冰霜伤害", "en": "Adds {0} to {1} Cold Damage"},
        {"src": "附加 {0} - {1} 冰霜伤害\n玩家及其召唤生物附加 {0} - {1} 冰霜伤害",
         "en": "{0} to {1} added Cold Damage\nPlayers and their Minions have {0} to {1} added Cold Damage"},
        // A name capture, alone and on the first of two lines.
        {"src": "{0}范围内的天赋可以配置", "en": "Passives in Radius of {0} can be Allocated"},
        {"src": "{}范围内的天赋可以在\n未连结至天赋树的情况下配置",
         "en": "Passives in Radius of {} can be Allocated\nwithout being connected to your tree"}
    ]);
    LocalePack::from_files(
        "zz",
        [
            ("stat_lines.json", templates.to_string().into_bytes()),
            (
                "passive_names.json",
                r#"{"Iron Reflexes":"霸体"}"#.as_bytes().to_vec(),
            ),
        ],
    )
    .unwrap()
}

fn render(renderer: &LineRenderer, lines: &[&str]) -> Vec<String> {
    renderer.render_lines(lines.iter().map(|line| line.to_string()))
}

#[test]
fn adjacent_lines_of_one_description_render_together() {
    let pack = multiline_pack();
    let renderer = LineRenderer::new(&[&pack], []).unwrap();
    // Only a line that can begin a multi-line template is joined onward.
    assert_eq!(
        renderer.span_from("All Damage is taken from Mana before Life"),
        2
    );
    assert_eq!(
        renderer.span_from(
            "12% of Physical Damage prevented Recouped as Energy Shield per enemy Power"
        ),
        3
    );
    assert_eq!(renderer.span_from("+5 to maximum Life"), 1);
    assert_eq!(
        renderer.span_from("Passives in Radius of Iron Reflexes can be Allocated"),
        2
    );
    assert_eq!(renderer.span_from("50% less Mana Recovery Rate"), 1);
    // Two lines, the first of which also translates alone: the longer match
    // wins, and the translation has as many lines, so each keeps its own.
    assert_eq!(
        render(
            &renderer,
            &[
                "+5 to maximum Life",
                "All Damage is taken from Mana before Life",
                "50% less Mana Recovery Rate",
                "+6 to maximum Life",
            ]
        ),
        [
            "+5 生命上限",
            "受到的所有伤害在扣除生命前优先从魔力扣除",
            "魔力回复速度总降 50%",
            "+6 生命上限",
        ]
    );
    // Three lines, with a value; the middle line alone also translates.
    assert_eq!(
        render(
            &renderer,
            &[
                "5% of Physical Damage prevented Recouped as Energy Shield per enemy Power",
                "Energy Shield does not Recharge",
                "You cannot Recover Energy Shield from Regeneration",
            ]
        ),
        [
            "每点敌人威力值使被阻挡的物理伤害的 5% 吸纳为能量护盾",
            "能量护盾不会充能",
            "能量护盾不能通过再生回复",
        ]
    );
    // A name capture on the first line; that line alone is a template too.
    assert_eq!(
        render(
            &renderer,
            &[
                "Passives in Radius of Iron Reflexes can be Allocated",
                "without being connected to your tree",
                "Passives in Radius of Iron Reflexes can be Allocated",
            ]
        ),
        [
            "霸体范围内的天赋可以在",
            "未连结至天赋树的情况下配置",
            "霸体范围内的天赋可以配置",
        ]
    );
}

#[test]
fn a_joined_translation_with_another_line_count_goes_to_the_first_line() {
    let pack = multiline_pack();
    let renderer = LineRenderer::new(&[&pack], []).unwrap();
    assert_eq!(
        render(
            &renderer,
            &[
                "Gain 4% of Damage as Extra Fire Damage for",
                "every different Grenade fired in the past 8 seconds",
                "Energy Shield Recharge starts after spending a total of",
                "2000 Mana, no more than once every 2 seconds",
            ]
        ),
        [
            "过去 8 秒内每发射一种不同的榴弹，获得相当于伤害 4% 的额外火焰伤害",
            "",
            "总共消耗 2000 点魔力后，能量护盾充能开始，每 2 秒最多触发一次",
            "",
        ]
    );
    // The other way: two localized lines read as one English line.
    let parser = LineParser::new(&[&pack], []).unwrap();
    let typed = ["力量的固有加成改为", "每 1 点力量 +5 命中值"];
    assert_eq!(
        parser.parse_lines(&typed),
        [
            Some(
                "Inherent bonus of Strength grants +5 to Accuracy Rating per Strength instead"
                    .into()
            ),
            Some(String::new()),
        ]
    );
    assert_eq!(
        parser.parse_text("Rarity: Rare\n力量的固有加成改为\n每 1 点力量 +5 命中值\n+5 生命上限"),
        "Rarity: Rare\nInherent bonus of Strength grants +5 to Accuracy Rating per Strength instead\n+5 to maximum Life"
    );
}

#[test]
fn a_line_holding_a_line_break_translates_whole() {
    let pack = multiline_pack();
    let renderer = LineRenderer::new(&[&pack], []).unwrap();
    for line in [
        "All Damage is taken from Mana before Life\n50% less Mana Recovery Rate",
        "All Damage is taken from Mana before Life\r\n50% less Mana Recovery Rate",
        "All Damage is taken from Mana before Life \n  50% less Mana Recovery Rate",
    ] {
        assert_eq!(
            renderer.render_line(line).as_deref(),
            Some("受到的所有伤害在扣除生命前优先从魔力扣除\n魔力回复速度总降 50%"),
            "{line:?}"
        );
    }
    assert_eq!(
        render(
            &renderer,
            &[
                "Energy Shield Recharge starts after spending a total of\n2000 Mana, no more than once every 2 seconds",
                "+5 to maximum Life",
            ]
        ),
        [
            "总共消耗 2000 点魔力后，能量护盾充能开始，每 2 秒最多触发一次",
            "+5 生命上限",
        ]
    );
    // An element holding a line break is not joined with its neighbour.
    assert_eq!(
        render(
            &renderer,
            &[
                "All Damage is taken from Mana before Life",
                "50% less Mana Recovery Rate\n+5 to maximum Life",
            ]
        ),
        [
            "受到的所有伤害在扣除生命前优先从魔力扣除",
            "50% less Mana Recovery Rate\n+5 to maximum Life",
        ]
    );
}

#[test]
fn unrelated_neighbours_are_not_joined() {
    let pack = multiline_pack();
    let renderer = LineRenderer::new(&[&pack], []).unwrap();
    // Each line translates alone; no template covers them together.
    assert_eq!(
        render(
            &renderer,
            &["+5 to maximum Life", "Energy Shield does not Recharge"]
        ),
        ["+5 生命上限", "能量护盾无法充能"]
    );
    // The lines of a description out of order, or apart, are not it.
    assert_eq!(
        render(
            &renderer,
            &[
                "50% less Mana Recovery Rate",
                "All Damage is taken from Mana before Life",
                "",
                "50% less Mana Recovery Rate",
            ]
        ),
        [
            "50% less Mana Recovery Rate",
            "受到的所有伤害在扣除生命前优先从魔力扣除",
            "",
            "50% less Mana Recovery Rate",
        ]
    );
    // A text spanning lines never matches a one-line template (nor a name).
    assert_eq!(
        renderer.render_line("Energy Shield does\nnot Recharge"),
        None
    );
    assert_eq!(renderer.render_line("+5 to maximum\nLife"), None);
    assert!(render(&renderer, &[]).is_empty());
}

#[test]
fn localized_lines_of_one_description_parse_together() {
    let pack = multiline_pack();
    let parser = LineParser::new(&[&pack], []).unwrap();
    let typed = [
        "受到的所有伤害在扣除生命前优先从魔力扣除",
        "魔力回复速度总降 50%",
        "+5 生命上限",
    ];
    assert_eq!(
        parser.parse_lines(&typed),
        [
            Some("All Damage is taken from Mana before Life".into()),
            Some("50% less Mana Recovery Rate".into()),
            Some("+5 to maximum Life".into()),
        ]
    );
    // A lone first line keeps its own translation; an unknown line stays `None`.
    assert_eq!(
        parser.parse_lines(&["受到的所有伤害在扣除生命前优先从魔力扣除", "不认识"]),
        [
            Some("All Damage is taken from Mana before Life".into()),
            None
        ]
    );
    // One localized line read as two English lines (as before).
    assert_eq!(
        parser.parse_text(
            "过去 8 秒内每发射一种不同的榴弹，获得相当于伤害 4% 的额外火焰伤害\r\n+5 to Strength"
        ),
        "Gain 4% of Damage as Extra Fire Damage for\nevery different Grenade fired in the past 8 seconds\n+5 to Strength"
    );
    assert_eq!(
        parser.parse_text("受到的所有伤害在扣除生命前优先从魔力扣除\n魔力回复速度总降 50%"),
        "All Damage is taken from Mana before Life\n50% less Mana Recovery Rate"
    );
}

#[test]
fn runs_cover_every_line_once_in_order() {
    let lines = ["a", "b", "c", "d", "e"];
    // Only "b\nc" is recognized as a whole.
    let runs = translate_runs(
        &lines,
        |_| 4,
        |text| (text == "b\nc").then(|| "B\nC".to_string()),
    );
    assert_eq!(
        runs.iter()
            .map(|run| (run.start, run.len))
            .collect::<Vec<_>>(),
        [(0, 1), (1, 2), (3, 1), (4, 1)]
    );
    assert_eq!(
        spread_runs(&runs, lines.len()),
        [None, Some("B".into()), Some("C".into()), None, None]
    );
    // A one-line stretch that renders as two lines keeps them in its slot.
    let runs = translate_runs(&["x"], |_| 3, |_| Some("X\nY".to_string()));
    assert_eq!(spread_runs(&runs, 1), [Some("X\nY".to_string())]);
}

#[test]
fn a_repeated_placeholder_joins_only_lines_with_the_same_value() {
    let pack = multiline_pack();
    let parser = LineParser::new(&[&pack], []).unwrap();
    // Different values: two separate lines, the first read alone (the
    // second has no template of its own), never one with the second's values.
    assert_eq!(
        parser.parse_lines(&[
            "附加 5 - 10 冰霜伤害",
            "玩家及其召唤生物附加 3 - 4 冰霜伤害"
        ]),
        [Some("Adds 5 to 10 Cold Damage".into()), None]
    );
    assert_eq!(
        parser.parse_lines(&[
            "附加 5 - 10 冰霜伤害",
            "玩家及其召唤生物附加 5 - 10 冰霜伤害"
        ]),
        [
            Some("5 to 10 added Cold Damage".into()),
            Some("Players and their Minions have 5 to 10 added Cold Damage".into()),
        ]
    );
    let renderer = LineRenderer::new(&[&pack], []).unwrap();
    assert_eq!(
        renderer.render_line(
            "5 to 10 added Cold Damage\nPlayers and their Minions have 3 to 4 added Cold Damage"
        ),
        None
    );
}

/// A pack with official rows and the translation repository's supplement
/// rows (`origin`), as `stat_lines.json` carries them.
fn supplement_pack() -> LocalePack {
    let templates = serde_json::json!([
        {"src": "{0:+d} 生命上限", "en": "{0:+d} to maximum Life"},
        {"src": "每 2% 格挡几率获得相当于伤害 {0}% 的火焰伤害",
         "en": "Gain {0}% of damage as Fire damage per 2% Chance to Block", "origin": "agent"},
        {"src": "受到的所有伤害在扣除生命前优先从魔力扣除\n魔力回复速度总降 50%",
         "en": "All Damage is taken from Mana before Life\n50% less Mana Recovery Rate", "origin": "human"},
        {"src": "配置 {0}", "en": "Allocates {0}"},
        {"src": "{0}% 某某", "en": "{0}% Something", "origin": "agent"},
        {"src": "官方 {0}", "en": "Official {0}", "origin": "official"}
    ]);
    LocalePack::from_files(
        "zz",
        [
            ("stat_lines.json", templates.to_string().into_bytes()),
            (
                "passive_names.json",
                r#"{"Iron Reflexes":"霸体"}"#.as_bytes().to_vec(),
            ),
        ],
    )
    .unwrap()
}

#[test]
fn a_rendered_line_carries_the_origin_of_its_template() {
    use pobr_data::catalog::TextOrigin::{Agent, Human};
    let pack = supplement_pack();
    let renderer = LineRenderer::new(&[&pack], []).unwrap();
    assert_eq!(
        renderer.render_line_with_origin("+5 to maximum Life"),
        Some(("+5 生命上限".into(), None))
    );
    assert_eq!(
        renderer.render_line_with_origin("Gain 3% of damage as Fire damage per 2% Chance to Block"),
        Some((
            "每 2% 格挡几率获得相当于伤害 3% 的火焰伤害".into(),
            Some(Agent)
        ))
    );
    // An explicit "official" is the same as none.
    assert_eq!(
        renderer.render_line_with_origin("Official 5"),
        Some(("官方 5".into(), None))
    );
    // A name capture is official; a value another pair translates brings its origin.
    assert_eq!(
        renderer.render_line_with_origin("Allocates Iron Reflexes"),
        Some(("配置 霸体".into(), None))
    );
    assert_eq!(
        renderer.render_line_with_origin("Allocates 5% Something"),
        Some(("配置 5% 某某".into(), Some(Agent)))
    );
    assert_eq!(renderer.render_line_with_origin("Unknown line"), None);
    // Batches: one origin per output, a joined run's on each of its lines,
    // none for a line kept in English; the texts are render_lines' own.
    let lines = [
        "+5 to maximum Life",
        "All Damage is taken from Mana before Life",
        "50% less Mana Recovery Rate",
        "Gain 3% of damage as Fire damage per 2% Chance to Block",
        "Unknown line",
    ]
    .map(String::from);
    let detailed = renderer.render_lines_with_origins(lines.clone());
    assert_eq!(
        detailed
            .iter()
            .map(|(_, origin)| *origin)
            .collect::<Vec<_>>(),
        [None, Some(Human), Some(Human), Some(Agent), None]
    );
    assert_eq!(
        detailed
            .into_iter()
            .map(|(text, _)| text)
            .collect::<Vec<_>>(),
        renderer.render_lines(lines)
    );
}

#[test]
fn a_continuation_line_has_no_origin_of_its_own() {
    let templates = serde_json::json!([
        {"src": "过去 8 秒内每发射一种不同的榴弹，获得相当于伤害 {0}% 的额外火焰伤害",
         "en": "Gain {0}% of Damage as Extra Fire Damage for\nevery different Grenade fired in the past 8 seconds",
         "origin": "agent"}
    ]);
    let pack = templates_pack("zz", templates);
    let renderer = LineRenderer::new(&[&pack], []).unwrap();
    assert_eq!(
        renderer.render_lines_with_origins(
            [
                "Gain 4% of Damage as Extra Fire Damage for",
                "every different Grenade fired in the past 8 seconds",
            ]
            .map(String::from)
        ),
        [
            (
                "过去 8 秒内每发射一种不同的榴弹，获得相当于伤害 4% 的额外火焰伤害".to_string(),
                Some(pobr_data::catalog::TextOrigin::Agent)
            ),
            (String::new(), None),
        ]
    );
}

/// Templates whose value slot sits at the line's edge, as in the zh-CN pack.
fn edge_slot_pack() -> LocalePack {
    let templates = serde_json::json!([
        {"src": "持续性伤害总降 {0}%", "en": "{0}% less Damage over Time"},
        {"src": "受到的持续伤害总降 20%", "en": "Take 20% less Damage over time"},
        {"src": "有 {0} 个插槽", "en": "Has {0} Sockets"},
        {"src": "伤害提高 {0}%", "en": "{0}% increased Damage"},
        {"src": "召唤生物伤害提高 {0}%", "en": "Minions deal {0}% increased Damage"},
        {"src": "所有{1}技能等级 +{0}", "en": "+{0} to Level of all {1} Skills"},
        {"src": "配置 {0}", "en": "Allocates {0}"}
    ]);
    LocalePack::from_files(
        "zz",
        [
            ("stat_lines.json", templates.to_string().into_bytes()),
            ("words.json", r#"{"Minion":"召唤生物"}"#.as_bytes().to_vec()),
            (
                "passive_names.json",
                r#"{"Iron Reflexes":"霸体"}"#.as_bytes().to_vec(),
            ),
        ],
    )
    .unwrap()
}

#[test]
fn a_rendered_line_never_carries_untranslated_words_in_a_value() {
    let pack = edge_slot_pack();
    let renderer = LineRenderer::new(&[&pack], []).unwrap();
    // A slot at the line's edge would capture the rest of a longer line;
    // that line has no template, so it stays English whole.
    for line in [
        "Take 30% less Damage over time",
        "Has 4 Augment Sockets",
        "+(1-2) to Level of all Mark Skills",
        "Allocates Unknown Passive",
    ] {
        assert_eq!(renderer.render_line(line), None, "{line}");
    }
    // A longer template that the edge slot hid is found instead.
    assert_eq!(
        renderer
            .render_line("Minions deal (10-12)% increased Damage")
            .as_deref(),
        Some("召唤生物伤害提高 (10-12)%")
    );
    // Values that translate, and values without words, still fill a slot.
    assert_eq!(
        renderer
            .render_line("Take 20% less Damage over time")
            .as_deref(),
        Some("受到的持续伤害总降 20%")
    );
    assert_eq!(
        renderer.render_line("Has 4 Sockets").as_deref(),
        Some("有 4 个插槽")
    );
    assert_eq!(
        renderer.render_line("(10-12)% increased Damage").as_deref(),
        Some("伤害提高 (10-12)%")
    );
    assert_eq!(
        renderer
            .render_line("+(1-2) to Level of all Minion Skills")
            .as_deref(),
        Some("所有召唤生物技能等级 +(1-2)")
    );
    assert_eq!(
        renderer.render_line("Allocates Iron Reflexes").as_deref(),
        Some("配置 霸体")
    );
}

#[test]
fn a_parsed_line_requires_its_word_captures_to_translate() {
    let mut pack = templates_pack(
        "zh-CN",
        serde_json::json!([
            {"src": "有 {0} 个插槽", "en": "Has {0} Sockets"},
            {"src": "伤害提高 {0}%", "en": "{0}% increased Damage"},
            {"src": "羁绊：{0}", "en": "Bonded: {0}"},
            {"src": "配置 {0}", "en": "Allocates {0}"}
        ]),
    );
    pack.set_names(
        NameKind::Word,
        [("Iron Reflexes".into(), "霸体".into())].into(),
    );
    let parser = LineParser::new(&[&pack], []).unwrap();
    for (line, expected) in [
        ("有 4 个插槽", "Has 4 Sockets"),
        ("配置 霸体", "Allocates Iron Reflexes"),
        ("羁绊：伤害提高 12%", "Bonded: 12% increased Damage"),
        // Unformatted string slots still preserve ranges without words.
        ("伤害提高 (10-12)%", "(10-12)% increased Damage"),
    ] {
        assert_eq!(parser.parse_line(line).as_deref(), Some(expected), "{line}");
    }
    for line in [
        "有 4 Augment 个插槽",
        "有 4 深渊 个插槽",
        "伤害提高 Minions deal (10-12)%",
        "羁绊：12% increased Damage",
        "配置 Unknown Passive",
    ] {
        assert_eq!(parser.parse_line(line), None, "{line}");
        assert_eq!(parser.translate_line(line), None, "{line}");
        assert_eq!(parser.parse_text(line), line, "{line}");
    }
}

#[test]
fn a_literal_plus_before_a_signed_slot_renders_one_sign() {
    // This row is unchanged in the prepared and staged zh-CN packs.
    let pack = templates_pack(
        "zh-CN",
        serde_json::json!([{
            "src": "地图内有腐化地图的几率 +{0:+d}%",
            "en": "Your Maps have {0:+d}% chance to contain a Vaal Side Area"
        }]),
    );
    let renderer = LineRenderer::new(&[&pack], []).unwrap();
    let parser = LineParser::new(&[&pack], []).unwrap();
    for (value, expected) in [
        ("+5", "+5"),
        ("+0", "+0"),
        ("-5", "-5"),
        ("5", "+5"),
        ("+(1-2)", "+(1-2)"),
        ("-(1-2)", "-(1-2)"),
    ] {
        let line = format!("Your Maps have {value}% chance to contain a Vaal Side Area");
        assert_eq!(
            renderer.render_line_with_origin(&line),
            Some((format!("地图内有腐化地图的几率 {expected}%"), None)),
            "{line}"
        );
        let rendered = renderer.render_line(&line).unwrap();
        assert_eq!(
            parser.parse_line(&rendered),
            Some(format!(
                "Your Maps have {expected}% chance to contain a Vaal Side Area"
            )),
            "{rendered}"
        );
    }
    assert_eq!(
        parser.parse_line("地图内有腐化地图的几率 ++5%"),
        Some("Your Maps have +5% chance to contain a Vaal Side Area".into())
    );
}

#[test]
fn only_a_literal_plus_before_a_signed_slot_is_optional() {
    let pack = templates_pack(
        "zh-CN",
        serde_json::json!([
            {"src": "无符号 +{0:d}%", "en": "Unsigned {0:d}%"},
            {"src": "数值 {0:+d}%", "en": "Signed {0:+d}%"},
            {"src": "混合 {0:d}% 和 +{1:+d}%", "en": "Mixed {0:d}% and {1:+d}%"}
        ]),
    );
    let parser = LineParser::new(&[&pack], []).unwrap();
    assert_eq!(parser.parse_line("无符号 +5%"), Some("Unsigned 5%".into()));
    assert_eq!(parser.parse_line("数值 +5%"), Some("Signed +5%".into()));
    for line in [
        "无符号 5%",
        "无符号 -5%",
        "无符号 +(1-2)%",
        "数值 +(1-2)%",
        "混合 (1-2)% 和 +5%",
    ] {
        assert_eq!(parser.parse_line(line), None, "{line}");
    }
}

#[test]
fn a_bonded_line_renders_through_the_packs_own_templates() {
    let templates = serde_json::json!([
        {"src": "羁绊： {0}", "en": "Bonded: {0}"},
        {"src": "羁绊： {0:d} 生命上限", "en": "Bonded: {0:d} to maximum Life"},
        {"src": "{0:d} 魔力上限", "en": "{0:d} to maximum Mana"},
        {"src": "伤害提高 {0}%", "en": "{0}% increased Damage"}
    ]);
    let pack = templates_pack("zz", templates);
    let renderer = LineRenderer::new(&[&pack], []).unwrap();
    let render = |line: &str| renderer.render_line(line);
    // A whole-line row first; otherwise the prefix template around the effect's own rendering.
    assert_eq!(
        render("Bonded: +40 to maximum Life").as_deref(),
        Some("羁绊： +40 生命上限")
    );
    assert_eq!(
        render("Bonded: +25 to maximum Mana").as_deref(),
        Some("羁绊： +25 魔力上限")
    );
    assert_eq!(
        render("Bonded: 12% increased Damage").as_deref(),
        Some("羁绊： 伤害提高 12%")
    );
    // An effect the pack cannot render keeps the whole line English.
    assert_eq!(render("Bonded: Unknown effect"), None);
    // The prefix alone, as the Web asks for it.
    assert_eq!(render("Bonded: {0}").as_deref(), Some("羁绊： {0}"));
}

fn templates_pack(code: &str, templates: serde_json::Value) -> LocalePack {
    LocalePack::from_files(
        code,
        [("stat_lines.json", templates.to_string().into_bytes())],
    )
    .unwrap()
}

#[test]
fn an_official_template_wins_over_a_supplement_listed_before_it() {
    use pobr_data::catalog::TextOrigin::Human;
    // The file sorts by (src, en): a supplement can come first.
    let pack = templates_pack(
        "zz",
        serde_json::json!([
            {"src": "我方补译 {0}% 生命", "en": "{0}% increased maximum Life", "origin": "human"},
            {"src": "我方补译 {0} 魔力", "en": "{0} to maximum Mana", "origin": "human"},
            {"src": "生命上限 {0}", "en": "Supplement {0} Life", "origin": "agent"},
            {"src": "生命上限 {0}", "en": "{0} to maximum Life"},
            {"src": "生命上限提高 {0}%", "en": "{0}% increased maximum Life"},
            {"src": "[{0}] 我方", "en": "Ours {0}", "origin": "agent"},
            {"src": "[{0}] 官方", "en": "Ours {0}"}
        ]),
    );
    let renderer = LineRenderer::new(&[&pack], []).unwrap();
    assert_eq!(
        renderer.render_line_with_origin("33% increased maximum Life"),
        Some(("生命上限提高 33%".into(), None))
    );
    // The string channel too (same anchor, official first).
    assert_eq!(
        renderer.render_line_with_origin("Ours ★"),
        Some(("[★] 官方".into(), None))
    );
    // A line only a supplement renders still takes it.
    assert_eq!(
        renderer.render_line_with_origin("+5 to maximum Mana"),
        Some(("我方补译 +5 魔力".into(), Some(Human)))
    );
    // The parser: a supplement sharing an official src never changes how it parses.
    let parser = LineParser::new(&[&pack], BASES).unwrap();
    assert_eq!(
        parser.parse_line("生命上限 +5").as_deref(),
        Some("+5 to maximum Life")
    );
}

#[test]
fn an_official_string_template_wins_over_a_supplement_with_a_longer_anchor() {
    let pack = templates_pack(
        "zz",
        serde_json::json!([
            {"src": "配置 {0}", "en": "Allocates {0}"},
            {"src": "我方配置 {0}", "en": "Allocates the {0}", "origin": "agent"},
            {"src": "官方值", "en": "the Bar"},
            {"src": "补值", "en": "Bar"}
        ]),
    );
    let renderer = LineRenderer::new(&[&pack], []).unwrap();
    assert_eq!(
        renderer.render_line_with_origin("Allocates the Bar"),
        Some(("配置 官方值".into(), None))
    );
}

#[test]
fn official_templates_of_the_whole_chain_rank_before_its_supplements() {
    let primary = templates_pack(
        "zz",
        serde_json::json!([
            {"src": "我方补译 {0}% 生命", "en": "{0}% increased maximum Life", "origin": "agent"}
        ]),
    );
    let fallback = templates_pack(
        "yy",
        serde_json::json!([{"src": "生命上限提高 {0}%", "en": "{0}% increased maximum Life"}]),
    );
    let renderer = LineRenderer::new(&[&primary, &fallback], []).unwrap();
    assert_eq!(
        renderer.render_line_with_origin("33% increased maximum Life"),
        Some(("生命上限提高 33%".into(), None))
    );
}

#[test]
fn a_rejected_candidate_leaves_no_origin_on_the_one_accepted_after_it() {
    let pack = templates_pack(
        "zz",
        serde_json::json!([
            {"src": "{0}% 某某", "en": "{0}% Something", "origin": "agent"},
            {"src": "授予全部 {0}% 某物", "en": "Grant all {0}% Something"},
            {"src": "全部授予 {1} {0}", "en": "Grant all {0} to {1}"},
            {"src": "{0}，给泽德", "en": "{0} to Zed"}
        ]),
    );
    let renderer = LineRenderer::new(&[&pack], []).unwrap();
    // "Grant all {0} to {1}" anchors longer and is tried first: its {0} renders
    // through the agent pair, but "Zed" does not translate, so it is rejected.
    assert_eq!(
        renderer.render_line_with_origin("Grant all 5% Something to Zed"),
        Some(("授予全部 5% 某物，给泽德".into(), None))
    );
}

/// Templates as the zh-CN pack writes them for lines the unique-item catalog
/// shows with roll ranges.
fn range_pack() -> LocalePack {
    templates_pack(
        "zz",
        serde_json::json!([
            {"src": "所有召唤生物技能等级 {0:+d}", "en": "{0:+d} to Level of all Minion Skills"},
            {"src": "+ {0} 维金灌注上限", "en": "+{0} to maximum Verisium Infusions"},
            {"src": "火焰抗性 {0:+d}%", "en": "{0:+d}% to Fire Resistance"},
            {"src": "{0:d} 生命上限", "en": "{0:d} to maximum Life"},
            {"src": "附加 {0} - {1} 基础火焰伤害", "en": "Adds {0} to {1} Fire Damage"},
            {"src": "范围内的核心天赋还会获得：{0}", "en": "Notable Passive Skills in Radius also grant {0}"},
            {"src": "战矛的攻击速度提高 {0:d}%", "en": "{0:d}% increased Attack Speed with Spears"},
            {"src": "冻结阈值提高 {0:d}%", "en": "{0:d}% increased Freeze Threshold", "origin": "agent"},
            {"src": "伤害提高 {0}%", "en": "{0}% increased Damage"}
        ]),
    )
}

#[test]
fn a_range_display_renders_through_its_rolled_lines_template() {
    use pobr_data::catalog::TextOrigin::Agent;
    let pack = range_pack();
    let renderer = LineRenderer::new(&[&pack], []).unwrap();
    let render = |line: &str| renderer.render_line_with_origin(line);
    // A slot with a format spec takes the range as it takes the rolled number,
    // sign included: the line's own `+` is shown once.
    for (line, expected) in [
        (
            "+2 to Level of all Minion Skills",
            "所有召唤生物技能等级 +2",
        ),
        (
            "+(1-2) to Level of all Minion Skills",
            "所有召唤生物技能等级 +(1-2)",
        ),
        ("+(10-20)% to Fire Resistance", "火焰抗性 +(10-20)%"),
        ("+(-10-10)% to Fire Resistance", "火焰抗性 +(-10-10)%"),
        ("+(0.5-1.5)% to Fire Resistance", "火焰抗性 +(0.5-1.5)%"),
        ("+(40-60) to maximum Life", "+(40-60) 生命上限"),
        // A sign the template writes itself stays outside the value.
        (
            "+(1-2) to maximum Verisium Infusions",
            "+ (1-2) 维金灌注上限",
        ),
        (
            "Adds (1-2) to (3-4) Fire Damage",
            "附加 (1-2) - (3-4) 基础火焰伤害",
        ),
    ] {
        assert_eq!(render(line), Some((expected.into(), None)), "{line}");
    }
    // A captured line holding a range renders through its own template, and
    // brings that template's origin.
    assert_eq!(
        render(
            "Notable Passive Skills in Radius also grant (1-2)% increased Attack Speed with Spears"
        ),
        Some((
            "范围内的核心天赋还会获得：战矛的攻击速度提高 (1-2)%".into(),
            None
        ))
    );
    assert_eq!(
        render("Notable Passive Skills in Radius also grant (2-4)% increased Freeze Threshold"),
        Some((
            "范围内的核心天赋还会获得：冻结阈值提高 (2-4)%".into(),
            Some(Agent)
        ))
    );
    // The parser is unchanged: input text holds rolled values.
    let parser = LineParser::new(&[&pack], BASES).unwrap();
    assert_eq!(
        parser.parse_line("所有召唤生物技能等级 +2").as_deref(),
        Some("+2 to Level of all Minion Skills")
    );
    assert_eq!(parser.parse_line("所有召唤生物技能等级 +(1-2)"), None);
}

#[test]
fn a_range_never_lets_a_mixed_line_through() {
    let pack = range_pack();
    let renderer = LineRenderer::new(&[&pack], []).unwrap();
    for line in [
        // A `{0}%` slot never takes words, with a range beside them or not.
        "Bonded: (1-2)% increased Damage",
        // A captured line whose own template is missing keeps the line English.
        "Notable Passive Skills in Radius also grant (1-2)% increased Wibble",
        // Only a whole range fills a numeric slot.
        "(40-60) Wibble to maximum Life",
        "+(40-60 to maximum Life",
        "+(40-) to maximum Life",
        "+(-60) to maximum Life",
    ] {
        assert_eq!(renderer.render_line(line), None, "{line}");
    }
    // A plain `{0}%` slot takes a range through the rolled line's skeleton
    // bucket, ahead of the string channel that took it before.
    assert_eq!(
        renderer.render_line("(1-2)% increased Damage").as_deref(),
        Some("伤害提高 (1-2)%")
    );
}

#[test]
fn a_range_on_the_first_line_of_a_description_joins_the_next_line() {
    let pack = templates_pack(
        "zz",
        serde_json::json!([
            {"src": "对元素异常状态的敌人的暴击伤害加成 {0:+d}%",
             "en": "{0:+d}% to Critical Damage Bonus against Enemies that are affected\nby Elemental Ailments"}
        ]),
    );
    let renderer = LineRenderer::new(&[&pack], []).unwrap();
    for (value, shown) in [("+15", "+15"), ("+(10-20)", "+(10-20)")] {
        assert_eq!(
            render(
                &renderer,
                &[
                    &format!("{value}% to Critical Damage Bonus against Enemies that are affected"),
                    "by Elemental Ailments",
                ]
            ),
            [
                format!("对元素异常状态的敌人的暴击伤害加成 {shown}%"),
                String::new()
            ],
            "{value}"
        );
    }
}

#[test]
fn a_whole_line_supplement_wins_over_an_official_composition_for_ranges_too() {
    use pobr_data::catalog::TextOrigin::Agent;
    // A per-skill row is more specific than the generic composition: the
    // numeric channel comes before the string one, as for the rolled line.
    // (The translation library never gives a supplement an official row's English.)
    let pack = templates_pack(
        "zz",
        serde_json::json!([
            {"src": "获得技能：{0} 级 {1}", "en": "Grants Skill: Level {0} {1}"},
            {"src": "获得技能: 等级 {0:d} 电球", "en": "Grants Skill: Level {0:d} Spark", "origin": "agent"},
            {"src": "电球", "en": "Spark"}
        ]),
    );
    let renderer = LineRenderer::new(&[&pack], []).unwrap();
    for (line, expected) in [
        ("Grants Skill: Level 20 Spark", "获得技能: 等级 20 电球"),
        (
            "Grants Skill: Level (1-20) Spark",
            "获得技能: 等级 (1-20) 电球",
        ),
    ] {
        assert_eq!(
            renderer.render_line_with_origin(line),
            Some((expected.into(), Some(Agent))),
            "{line}"
        );
    }
}

#[test]
fn an_official_rolled_template_wins_over_a_supplement_written_with_parentheses() {
    // "+(1-2) to Foo" fits both a supplement whose literal holds the
    // parentheses (its own skeleton bucket) and the official rolled-line
    // template (the bucket without them): official first, as within a bucket.
    let pack = templates_pack(
        "zz",
        serde_json::json!([
            {"src": "补译 +({0}-{1}) 某物", "en": "+({0}-{1}) to Foo", "origin": "agent"},
            {"src": "某物 {0:+d}", "en": "{0:+d} to Foo"}
        ]),
    );
    let renderer = LineRenderer::new(&[&pack], []).unwrap();
    assert_eq!(
        renderer.render_line_with_origin("+(1-2) to Foo"),
        Some(("某物 +(1-2)".into(), None))
    );
}
