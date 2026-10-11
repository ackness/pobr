//! Build Code codec integration tests, including byte-level compatibility checks against a real PoB2 share code.

use pobr_build::build_code::{decode_pob_code, encode_pob_code};
use pobr_build::import_detect::{ImportKind, detect_import};

/// Real PoB2 build code (copied from the ninja deadeye build in examples/demo-bd-test).
const DEADEYE_CODE: &str = include_str!("../../../../examples/demo-bd-test/ninja-bd-deadeye.txt");

#[test]
fn decodes_real_pob2_build_code() {
    let xml = decode_pob_code(DEADEYE_CODE.trim()).expect("decode real PoB2 code");
    // Byte-level compatibility: decoded XML must contain the PoB root tag (PoE2 uses <PathOfBuilding2>, prefix match).
    assert!(
        xml.contains("<PathOfBuilding"),
        "decoded XML must contain <PathOfBuilding root, got prefix: {:?}",
        &xml[..xml.len().min(80)]
    );
    // Further sanity check that this is a plausible Build document.
    assert!(xml.contains("<Build"));
    assert!(xml.contains("Deadeye"));
}

#[test]
fn detects_real_code_as_build_code() {
    assert_eq!(
        detect_import(DEADEYE_CODE.trim()),
        Some(ImportKind::BuildCode)
    );
}

#[test]
fn reencoding_decoded_xml_roundtrips() {
    let xml = decode_pob_code(DEADEYE_CODE.trim()).expect("decode");
    let recode = encode_pob_code(&xml).expect("encode");
    let xml2 = decode_pob_code(&recode).expect("decode re-encoded");
    // Semantic round-trip: our deflate level may differ from PoB's (bytes need not match), but the decoded XML must be identical.
    assert_eq!(xml, xml2);
}

#[test]
fn handles_trailing_whitespace_in_fixture() {
    // The fixture may have a trailing newline/whitespace; decoding must tolerate it (should still decode without trimming).
    let with_ws = format!("\n  {}\n\t", DEADEYE_CODE.trim());
    let xml = decode_pob_code(&with_ws).expect("decode with surrounding whitespace");
    assert!(xml.contains("<PathOfBuilding"));
}

#[test]
fn unidentified_only_groups_keep_source_slots_and_later_group_indices() {
    let xml = r#"<PathOfBuilding2><Build className="Sorceress" level="85" mainSocketGroup="2"/>
<Skills activeSkillSet="1"><SkillSet id="1">
<Skill enabled="true"><Gem nameSpec="Unresolved first" level="0" quality="0"/></Skill>
<Skill enabled="true"><Gem gemId="Metadata/Items/Gems/SkillGemFireball" skillId="FireballPlayer" level="20" quality="0"/></Skill>
</SkillSet></Skills></PathOfBuilding2>"#;
    let build = pobr_build::xml_build::parse_build(xml).unwrap();
    assert_eq!(build.socket_groups.len(), 2);
    assert_eq!(build.socket_groups[0].gem_skills[0].skill_id, "");
    assert_eq!(
        build.socket_groups[0].gem_skills[0].name_spec.as_deref(),
        Some("Unresolved first")
    );
    assert_eq!(build.socket_groups[0].gem_skills[0].gem_level, 0);
    assert_eq!(
        build.socket_groups[1].gem_skills[0].skill_id,
        "FireballPlayer"
    );
    assert_eq!(build.main_socket_group, Some(2));
}
