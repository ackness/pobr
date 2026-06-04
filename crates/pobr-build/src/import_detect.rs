//! 快捷导入识别：根据粘贴内容判断输入类型，路由到对应导入器。
//!
//! 覆盖 PoB 桌面端「Import」对话框接受的几类输入：
//! - **Build Code**：URL-safe Base64 + zlib 压缩的 Build XML（PoB share code）。
//! - **Build XML**：未压缩的 `<PathOfBuilding...>` 文本（直接粘贴 XML）。
//! - **pobb.in / pastebin 链接**：分享服务 URL，需先抓取再解码（本 crate 不做网络 I/O，
//!   仅识别并提取出 paste key 供上层处理）。
//! - **Raw Item Text**：从游戏内复制的单件物品文本（含 `Item Class:` / `Rarity:` 头）。
//!
//! 纯函数 + 启发式，不做网络请求。判别顺序由强到弱，避免误判。

/// 识别出的导入类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportKind {
    /// PoB Build Code（压缩 + Base64）。
    BuildCode,
    /// 未压缩的 Build XML。
    BuildXml,
    /// 分享链接（pobb.in / pastebin 等），携带解析出的 paste key。
    ShareLink { service: ShareService, key: String },
    /// 游戏内复制的物品文本。
    RawItem,
}

/// 已知的分享服务。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareService {
    /// pobb.in（Path of Building 专用分享）。
    PobbIn,
    /// pastebin.com。
    Pastebin,
}

/// 根据输入内容识别导入类型。返回 `None` 表示无法识别。
pub fn detect_import(input: &str) -> Option<ImportKind> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 1) 分享链接：URL 形态最易识别，优先。
    if let Some(link) = detect_share_link(trimmed) {
        return Some(link);
    }

    // 2) 未压缩 Build XML：以 `<?xml` 或 `<PathOfBuilding` 开头。
    if is_build_xml(trimmed) {
        return Some(ImportKind::BuildXml);
    }

    // 3) Raw item text：含游戏内物品头部标记。
    if is_raw_item(trimmed) {
        return Some(ImportKind::RawItem);
    }

    // 4) Build Code：剩余情况里若像 Base64 + 可解出 zlib header，则判为 build code。
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

/// 从 URL 中提取 `host/` 之后第一段路径作为 key（去掉查询串 / 末尾斜杠 / `raw/` 前缀）。
fn extract_path_key(input: &str, host_marker: &str) -> Option<String> {
    let lower = input.to_ascii_lowercase();
    let pos = lower.find(host_marker)?;
    let after = &input[pos + host_marker.len()..];
    // 去查询串与锚点。
    let after = after.split(['?', '#']).next().unwrap_or("");
    // pastebin 常见 raw/ 前缀。
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

/// 游戏内物品文本特征：`Item Class:` / `Rarity:` 头，或经典 `--------` 分隔线 + 名称块。
fn is_raw_item(input: &str) -> bool {
    let head: String = input.lines().take(4).collect::<Vec<_>>().join("\n");
    head.contains("Item Class:")
        || head.contains("Rarity:")
        || (input.contains("--------") && input.contains("Rarity"))
}

/// Build Code 启发式：去空白后整体落在 Base64 字母表内，且足够长。
///
/// 不在此处真正解压（保持纯启发式、零分配开销外的轻量判定）；最终是否合法由
/// [`crate::build_code::decode_pob_code`] 决定。
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
