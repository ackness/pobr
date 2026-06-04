//! Build Code 编解码集成测试，含真实 PoB2 share code 的字节级兼容验证。

use pobr_build::build_code::{decode_pob_code, encode_pob_code};
use pobr_build::import_detect::{ImportKind, detect_import};

/// 真实 PoB2 build code（从 examples/demo-bd-test 复制的 ninja deadeye build）。
const DEADEYE_CODE: &str = include_str!("../../../examples/demo-bd-test/ninja-bd-deadeye.txt");

#[test]
fn decodes_real_pob2_build_code() {
    let xml = decode_pob_code(DEADEYE_CODE.trim()).expect("decode real PoB2 code");
    // 字节级兼容：解出的 XML 必须含 PoB 根标签（PoE2 为 <PathOfBuilding2>，前缀匹配）。
    assert!(
        xml.contains("<PathOfBuilding"),
        "decoded XML must contain <PathOfBuilding root, got prefix: {:?}",
        &xml[..xml.len().min(80)]
    );
    // 进一步确认是合理的 Build 文档。
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
    // 语义往返：我们的 deflate 等级可能与 PoB 不同（字节不必相同），但解出的 XML 必须一致。
    assert_eq!(xml, xml2);
}

#[test]
fn handles_trailing_whitespace_in_fixture() {
    // fixture 末尾可能带换行 / 空白；解码须容忍（不 trim 也应可解）。
    let with_ws = format!("\n  {}\n\t", DEADEYE_CODE.trim());
    let xml = decode_pob_code(&with_ws).expect("decode with surrounding whitespace");
    assert!(xml.contains("<PathOfBuilding"));
}
