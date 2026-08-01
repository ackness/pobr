//! 数据包信封（manifest）schema：描述某个 PoE2 版本下入库了哪些域与语言。
//!
//! v2 起 `domains` 按三层物理目录分段（`base`/`overlay`/`generated`，
//! 见P1）；反序列化兼容 v1 的扁平数组形（视为全部归 `base`）。

use serde::{Deserialize, Serialize};

/// 当前 catalog schema 版本。结构不兼容变更时 +1。
///
/// v2：manifest `domains` 由扁平数组改为 [`DomainSections`] 三段。
pub const CATALOG_SCHEMA_VERSION: u32 = 2;

/// 数据包信封：描述某个 PoE2 版本下入库了哪些域与语言。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataManifest {
    pub schema_version: u32,
    /// CDN 补丁版本号，如 `4.5.0.3.4`（公开版 0.5.0）。
    pub poe_version: String,
    /// 已生成 i18n 边车的语言标签，如 `["zh-TW"]`（英文为 canonical，不计入）。
    pub languages: Vec<String>,
    /// 已生成的数据域文件名（不含扩展名），按三层目录分段。
    pub domains: DomainSections,
}

/// manifest v2 的三段 domains（对应 `data/<版本>/{base/, overlay/, generated/}`）。
///
/// 序列化恒为 v2 对象形；反序列化兼容 v1 扁平数组（全部视为 `base` 段）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DomainSections {
    /// `base/`：.dat 全自动再生的域（pipeline + adapter 产出，禁手改）。
    pub base: Vec<String>,
    /// `overlay/`：vendor Lua 抽取的人工策展域（extract-lua 产出，只许工具再生）。
    pub overlay: Vec<String>,
    /// `generated/`：可由 base + overlay 确定性派生的缓存域（precompile 产出）。
    pub generated: Vec<String>,
}

/// 反序列化中间形：v1 扁平数组 或 v2 三段对象。
#[derive(Deserialize)]
#[serde(untagged)]
enum DomainSectionsRepr {
    /// v1：扁平 `["base_items", ...]`，全部视为 base 段。
    Flat(Vec<String>),
    /// v2：`{"base": [...], "overlay": [...], "generated": [...]}`。
    Sections {
        base: Vec<String>,
        #[serde(default)]
        overlay: Vec<String>,
        #[serde(default)]
        generated: Vec<String>,
    },
}

impl<'de> Deserialize<'de> for DomainSections {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(match DomainSectionsRepr::deserialize(deserializer)? {
            DomainSectionsRepr::Flat(base) => Self {
                base,
                overlay: Vec::new(),
                generated: Vec::new(),
            },
            DomainSectionsRepr::Sections {
                base,
                overlay,
                generated,
            } => Self {
                base,
                overlay,
                generated,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v1 manifest（扁平 domains 数组）应可反序列化，且全部归入 base 段。
    #[test]
    fn deserializes_v1_flat_domains_as_base() {
        let json = r#"{
            "schema_version": 1,
            "poe_version": "4.5.0.3.4",
            "languages": ["zh-TW"],
            "domains": ["base_items", "mods"]
        }"#;
        let manifest: DataManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.domains.base, vec!["base_items", "mods"]);
        assert!(manifest.domains.overlay.is_empty());
        assert!(manifest.domains.generated.is_empty());
    }

    /// v2 manifest（三段 domains 对象）应按段反序列化。
    #[test]
    fn deserializes_v2_sectioned_domains() {
        let json = r#"{
            "schema_version": 2,
            "poe_version": "4.5.0.3.4",
            "languages": ["zh-TW"],
            "domains": {
                "base": ["base_items"],
                "overlay": ["skill_stat_map"],
                "generated": ["parsed_mods"]
            }
        }"#;
        let manifest: DataManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.schema_version, 2);
        assert_eq!(manifest.domains.base, vec!["base_items"]);
        assert_eq!(manifest.domains.overlay, vec!["skill_stat_map"]);
        assert_eq!(manifest.domains.generated, vec!["parsed_mods"]);
    }

    /// v2 缺省 overlay/generated 段应回退为空（向前兼容部分写出）。
    #[test]
    fn v2_missing_sections_default_to_empty() {
        let json = r#"{
            "schema_version": 2,
            "poe_version": "4.5.0.3.4",
            "languages": [],
            "domains": { "base": ["stats"] }
        }"#;
        let manifest: DataManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.domains.base, vec!["stats"]);
        assert!(manifest.domains.overlay.is_empty());
        assert!(manifest.domains.generated.is_empty());
    }

    /// 序列化恒为 v2 三段对象形，且往返等价。
    #[test]
    fn serializes_v2_shape_roundtrip() {
        let manifest = DataManifest {
            schema_version: CATALOG_SCHEMA_VERSION,
            poe_version: crate::DATA_VERSION.into(),
            languages: vec!["zh-TW".into()],
            domains: DomainSections {
                base: vec!["base_items".into()],
                overlay: vec![],
                generated: vec![],
            },
        };
        let json = serde_json::to_string(&manifest).unwrap();
        assert!(json.contains(r#""domains":{"base":["base_items"],"overlay":[],"generated":[]}"#));
        let back: DataManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, manifest);
    }
}
