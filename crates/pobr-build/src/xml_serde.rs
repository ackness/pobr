//! PoB Build XML 的轻量解析（quick-xml）。
//!
//! 完整还原 PoB Build XML（含 items / tree / skills 全量字段）是个大工程；此处先实现
//! **可被计算编排消费的最小子集**：
//! - 校验根元素为 `<PathOfBuilding...>`（PoE1 是 `PathOfBuilding`，PoE2 是 `PathOfBuilding2`）；
//! - 抽取 `<Build>` 头部的 `level` / `className` / `ascendClassName`；
//! - 抽取 `viewMode`（若有）。
//!
//! 解析失败 / 缺关键节点会返回 [`XmlError`]，但缺可选字段（如 ascendancy）不报错。

use quick_xml::Reader;
use quick_xml::events::Event;

use pobr_data::build_config::ViewMode;

use crate::build::CharacterIdentity;
use crate::error::XmlError;

/// 从 PoB Build XML 解析出的最小头部信息。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedBuildHeader {
    pub identity: CharacterIdentity,
    pub view_mode: ViewMode,
    /// 检测到的 PoB 主版本（`PathOfBuilding` → 1，`PathOfBuilding2` → 2）。
    pub pob_major: u8,
}

/// 解析 Build XML 头部。校验根元素并抽取 `<Build>` / `viewMode`。
pub fn parse_build_header(xml: &str) -> Result<ParsedBuildHeader, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut header = ParsedBuildHeader::default();
    let mut root_seen = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name_bytes = e.name();
                let name = String::from_utf8_lossy(name_bytes.as_ref()).to_string();

                if !root_seen {
                    root_seen = true;
                    if name == "PathOfBuilding" {
                        header.pob_major = 1;
                    } else if name == "PathOfBuilding2" {
                        header.pob_major = 2;
                    } else {
                        return Err(XmlError::NotPobRoot(name));
                    }
                    apply_root_attrs(&e, &mut header)?;
                    continue;
                }

                if name == "Build" {
                    apply_build_attrs(&e, &mut header)?;
                    // 头部信息已足够，提前结束以避免遍历整份大文档。
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }

    if !root_seen {
        return Err(XmlError::MissingNode("PathOfBuilding root".into()));
    }

    Ok(header)
}

/// 校验给定 XML 的根元素是否为 PathOfBuilding（用于导入识别 / 快速校验）。
pub fn is_pob_xml(xml: &str) -> bool {
    parse_build_header(xml).is_ok()
}

fn apply_root_attrs(
    e: &quick_xml::events::BytesStart<'_>,
    header: &mut ParsedBuildHeader,
) -> Result<(), XmlError> {
    for attr in e.attributes().flatten() {
        let key = attr.key;
        if key.as_ref() == b"viewMode" {
            let value = decode_attr(&attr)?;
            header.view_mode = parse_view_mode(&value);
        }
    }
    Ok(())
}

fn apply_build_attrs(
    e: &quick_xml::events::BytesStart<'_>,
    header: &mut ParsedBuildHeader,
) -> Result<(), XmlError> {
    for attr in e.attributes().flatten() {
        let value = decode_attr(&attr)?;
        match attr.key.as_ref() {
            b"level" => {
                header.identity.level =
                    value.parse::<u32>().map_err(|_| XmlError::InvalidAttr {
                        attr: "level".into(),
                        value: value.clone(),
                    })?;
            }
            b"className" => header.identity.class_name = value,
            b"ascendClassName" => header.identity.ascendancy_name = value,
            _ => {}
        }
    }
    Ok(())
}

fn decode_attr(attr: &quick_xml::events::attributes::Attribute<'_>) -> Result<String, XmlError> {
    attr.unescape_value()
        .map(|v| v.into_owned())
        .map_err(|e| XmlError::Parse(e.to_string()))
}

fn parse_view_mode(value: &str) -> ViewMode {
    match value.to_ascii_uppercase().as_str() {
        "CALCS" => ViewMode::Calcs,
        "ITEMS" => ViewMode::Items,
        "TREE" => ViewMode::Tree,
        "SKILLS" => ViewMode::Skills,
        "CONFIG" => ViewMode::Config,
        "IMPORT" => ViewMode::Import,
        _ => ViewMode::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<PathOfBuilding2>
    <Build level="98" className="Ranger" ascendClassName="Deadeye" viewMode="TREE">
    </Build>
</PathOfBuilding2>"#;

    #[test]
    fn parses_header() {
        let header = parse_build_header(SAMPLE).expect("parse");
        assert_eq!(header.pob_major, 2);
        assert_eq!(header.identity.level, 98);
        assert_eq!(header.identity.class_name, "Ranger");
        assert_eq!(header.identity.ascendancy_name, "Deadeye");
        assert_eq!(header.view_mode, ViewMode::Tree);
    }

    #[test]
    fn poe1_root_recognized() {
        let xml = r#"<PathOfBuilding><Build level="1" className="Witch"/></PathOfBuilding>"#;
        let header = parse_build_header(xml).expect("parse");
        assert_eq!(header.pob_major, 1);
        assert_eq!(header.identity.class_name, "Witch");
    }

    #[test]
    fn rejects_non_pob_root() {
        let xml = "<NotPoB><Build/></NotPoB>";
        assert!(matches!(
            parse_build_header(xml),
            Err(XmlError::NotPobRoot(_))
        ));
    }

    #[test]
    fn is_pob_xml_helper() {
        assert!(is_pob_xml(SAMPLE));
        assert!(!is_pob_xml("<html></html>"));
    }
}
