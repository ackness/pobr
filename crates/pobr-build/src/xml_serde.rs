//! Lightweight PoB Build XML parsing (quick-xml).
//!
//! Fully reconstructing PoB Build XML (all item / tree / skill fields) is a big
//! undertaking; this only implements the **minimal subset the calc orchestrator needs**:
//! - validates the root element is `<PathOfBuilding2>` (PoE2-only; PoB2 upstream has
//!   fully dropped the PoE1 `PathOfBuilding` root);
//! - extracts `level` / `className` / `ascendClassName` from the `<Build>` header;
//! - extracts `viewMode` (if present).
//!
//! Parse failures / missing required nodes return [`XmlError`], but missing optional
//! fields (e.g. ascendancy) are not an error.

use quick_xml::Reader;
use quick_xml::events::Event;

use pobr_data::build_config::ViewMode;

use crate::build::CharacterIdentity;
use crate::error::XmlError;

/// Minimal header info parsed from a PoB Build XML.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedBuildHeader {
    pub identity: CharacterIdentity,
    pub view_mode: ViewMode,
}

/// Parses the Build XML header. Validates the root element and extracts `<Build>` / `viewMode`.
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
                    // PoE2-only: only `PathOfBuilding2` is accepted. The PoE1
                    // `PathOfBuilding` root and any other root are rejected
                    // (matches PoB2 upstream).
                    if name != "PathOfBuilding2" {
                        return Err(XmlError::NotPobRoot(name));
                    }
                    apply_root_attrs(&e, &mut header)?;
                    continue;
                }

                if name == "Build" {
                    apply_build_attrs(&e, &mut header)?;
                    // The header is complete; stop early rather than walking the whole document.
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }

    if !root_seen {
        return Err(XmlError::MissingNode("PathOfBuilding2 root".into()));
    }

    Ok(header)
}

/// Checks whether a given XML's root element is PathOfBuilding (for import recognition / quick validation).
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
    // Deliberately avoid normalized_value: whitespace normalization collapses literal
    // newlines into spaces, but PoB stores multi-line mod text in attributes
    // (<Input string="a\nb">), where the newline is a line separator.
    let raw = String::from_utf8_lossy(&attr.value).into_owned();
    quick_xml::escape::unescape(&raw)
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
        assert_eq!(header.identity.level, 98);
        assert_eq!(header.identity.class_name, "Ranger");
        assert_eq!(header.identity.ascendancy_name, "Deadeye");
        assert_eq!(header.view_mode, ViewMode::Tree);
    }

    #[test]
    fn poe1_root_rejected() {
        // PoE2-only: the PoE1 `PathOfBuilding` root is no longer accepted (matches PoB2 upstream).
        let xml = r#"<PathOfBuilding><Build level="1" className="Witch"/></PathOfBuilding>"#;
        assert!(matches!(
            parse_build_header(xml),
            Err(XmlError::NotPobRoot(name)) if name == "PathOfBuilding"
        ));
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
