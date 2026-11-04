//! Parser and renderer over whole packs. The engine's own cases (ported from
//! the zh-CN translator unchanged) are in `engine.rs`.

use super::*;

/// A synthetic Latin-script locale written only in ASCII: nothing about its
/// characters tells it apart from English, so the parser must recognize it
/// by its templates and names.
fn latin_pack() -> LocalePack {
    LocalePack::from_files(
        "xx-XX",
        [
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
