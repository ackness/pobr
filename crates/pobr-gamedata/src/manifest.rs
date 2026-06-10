//! manifest（数据包信封）加载：v1 / v2 两形兼容。
//!
//! v2（[`pobr_data::catalog::CATALOG_SCHEMA_VERSION`] = 2）的 `domains` 为
//! `{base, overlay, generated}` 三段；v1 的扁平 `domains` 数组在反序列化层
//! （[`pobr_data::catalog::DomainSections`] 的 serde 实现）被视为全部归 `base`。
//! `manifest.json` 恒位于版本根，不参与 `base/` 定位。

use pobr_data::catalog::DataManifest;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载数据包信封（v1 扁平 domains 自动归入 `base` 段）。
    pub fn manifest(&self) -> Result<DataManifest, LoadError> {
        self.load_json_at(self.root().join("manifest.json"))
    }
}

#[cfg(test)]
mod tests {
    use crate::GameData;

    fn temp_manifest(tag: &str, json: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pobr-gamedata-manifest-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("manifest.json"), json).unwrap();
        dir
    }

    /// v1 manifest（扁平 domains）可加载，域全部归入 base 段。
    #[test]
    fn loads_v1_manifest() {
        let dir = temp_manifest(
            "v1",
            r#"{"schema_version":1,"poe_version":"4.5.0.0.0","languages":[],"domains":["stats","mods"]}"#,
        );
        let manifest = GameData::new(&dir).manifest().unwrap();
        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.domains.base, vec!["stats", "mods"]);
        assert!(manifest.domains.overlay.is_empty());
        assert!(manifest.domains.generated.is_empty());
    }

    /// v2 manifest（三段 domains）可加载。
    #[test]
    fn loads_v2_manifest() {
        let dir = temp_manifest(
            "v2",
            r#"{"schema_version":2,"poe_version":"4.5.0.0.0","languages":["zh-TW"],
                "domains":{"base":["stats"],"overlay":["skill_stat_map"],"generated":[]}}"#,
        );
        let manifest = GameData::new(&dir).manifest().unwrap();
        assert_eq!(manifest.schema_version, 2);
        assert_eq!(manifest.domains.base, vec!["stats"]);
        assert_eq!(manifest.domains.overlay, vec!["skill_stat_map"]);
        assert!(manifest.domains.generated.is_empty());
    }
}
