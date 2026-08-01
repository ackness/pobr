//! Quick-import recognition: figures out the input type from pasted content and routes
//! it to the right importer.
//!
//! Covers the input types PoB desktop's "Import" dialog accepts:
//! - **Build Code**: URL-safe Base64 + zlib compressed Build XML (PoB share code).
//! - **Build XML**: uncompressed `<PathOfBuilding...>` text (XML pasted directly).
//! - **pobb.in / pastebin links**: share service URLs that need fetching before they can
//!   be decoded (this crate does no network I/O — it only recognizes the link and
//!   extracts the paste key for the caller to fetch).
//! - **Raw Item Text**: a single item's text as copied from the game (`Item Class:` /
//!   `Rarity:` header).
//!
//! Pure functions + heuristics, no network requests. Checks run strongest-signal-first
//! to avoid misclassification.

/// The recognized import type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportKind {
    /// PoB Build Code (compressed + Base64).
    BuildCode,
    /// Uncompressed Build XML.
    BuildXml,
    /// A share link (pobb.in / pastebin / etc.), carrying the extracted paste key.
    ShareLink { service: ShareService, key: String },
    /// Item text copied from the game.
    RawItem,
}

/// Known share services.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareService {
    /// pobb.in (dedicated Path of Building share host).
    PobbIn,
    /// pastebin.com.
    Pastebin,
}

/// Detects the import type from the input content. Returns `None` if it can't be recognized.
pub fn detect_import(input: &str) -> Option<ImportKind> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 1) Share link: URL shape is the easiest to recognize, so check it first.
    if let Some(link) = detect_share_link(trimmed) {
        return Some(link);
    }

    // 2) Uncompressed Build XML: starts with `<?xml` or `<PathOfBuilding`.
    if is_build_xml(trimmed) {
        return Some(ImportKind::BuildXml);
    }

    // 3) Raw item text: contains an in-game item header marker.
    if is_raw_item(trimmed) {
        return Some(ImportKind::RawItem);
    }

    // 4) Build Code: whatever's left is treated as a build code if it looks like
    //    Base64 that could decode to a zlib header.
    if looks_like_build_code(trimmed) {
        return Some(ImportKind::BuildCode);
    }

    None
}

fn detect_share_link(input: &str) -> Option<ImportKind> {
    let lower = input.to_ascii_lowercase();
    let is_url = lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("pobb.in/")
        || lower.starts_with("www.");
    if !is_url {
        return None;
    }

    if let Some(key) = extract_path_key(input, "pobb.in/") {
        return Some(ImportKind::ShareLink {
            service: ShareService::PobbIn,
            key,
        });
    }
    if let Some(key) = extract_path_key(input, "pastebin.com/") {
        return Some(ImportKind::ShareLink {
            service: ShareService::Pastebin,
            key,
        });
    }

    None
}

/// Extracts the first path segment after `host/` as the key (strips query string /
/// trailing slash / `raw/` prefix).
fn extract_path_key(input: &str, host_marker: &str) -> Option<String> {
    let lower = input.to_ascii_lowercase();
    let pos = lower.find(host_marker)?;
    let after = &input[pos + host_marker.len()..];
    // Strip query string and anchor.
    let after = after.split(['?', '#']).next().unwrap_or("");
    // Common pastebin raw/ prefix.
    let after = after.strip_prefix("raw/").unwrap_or(after);
    let key = after.trim_matches('/').split('/').next().unwrap_or("");
    if key.is_empty() {
        None
    } else {
        Some(key.to_string())
    }
}

fn is_build_xml(input: &str) -> bool {
    let head = input.trim_start();
    head.starts_with("<?xml") || head.starts_with("<PathOfBuilding")
}

/// In-game item text signature: `Item Class:` / `Rarity:` header, or the classic
/// `--------` separator plus a name block.
fn is_raw_item(input: &str) -> bool {
    let head: String = input.lines().take(4).collect::<Vec<_>>().join("\n");
    head.contains("Item Class:")
        || head.contains("Rarity:")
        || (input.contains("--------") && input.contains("Rarity"))
}

/// Build Code heuristic: after stripping whitespace, the whole string falls within the
/// Base64 alphabet and is long enough.
///
/// Doesn't actually decompress here (kept as a cheap heuristic with no allocation
/// beyond string filtering); whether it's actually valid is decided by
/// [`crate::build_code::decode_pob_code`].
fn looks_like_build_code(input: &str) -> bool {
    let cleaned: String = input.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if cleaned.len() < 16 {
        return false;
    }
    cleaned
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '-' | '_' | '='))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_build_xml() {
        assert_eq!(
            detect_import("<?xml version=\"1.0\"?><PathOfBuilding2/>"),
            Some(ImportKind::BuildXml)
        );
        assert_eq!(
            detect_import("<PathOfBuilding><Build/></PathOfBuilding>"),
            Some(ImportKind::BuildXml)
        );
    }

    #[test]
    fn detects_pobbin_link() {
        assert_eq!(
            detect_import("https://pobb.in/abcDEF123"),
            Some(ImportKind::ShareLink {
                service: ShareService::PobbIn,
                key: "abcDEF123".to_string()
            })
        );
        assert_eq!(
            detect_import("pobb.in/xyz?foo=1"),
            Some(ImportKind::ShareLink {
                service: ShareService::PobbIn,
                key: "xyz".to_string()
            })
        );
    }

    #[test]
    fn detects_pastebin_link_with_raw() {
        assert_eq!(
            detect_import("https://pastebin.com/raw/AbC123"),
            Some(ImportKind::ShareLink {
                service: ShareService::Pastebin,
                key: "AbC123".to_string()
            })
        );
    }

    #[test]
    fn detects_raw_item() {
        let item = "Item Class: Bows\nRarity: Rare\nDeath Song\nSpine Bow";
        assert_eq!(detect_import(item), Some(ImportKind::RawItem));
    }

    #[test]
    fn detects_build_code() {
        assert_eq!(
            detect_import("eNrtfWtznLjS8OedX0G56jxf4iRISFzy"),
            Some(ImportKind::BuildCode)
        );
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(detect_import(""), None);
        assert_eq!(detect_import("   "), None);
        assert_eq!(detect_import("!!! @@@ ###"), None);
    }
}
