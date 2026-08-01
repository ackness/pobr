//! `overlay/gem_quality_stats.json` loader——宝石品质 stat 斜率
//! （`effect_id → [{stat, per_quality_rate}]`），schema 见
//! [`pobr_data::catalog::skills`] 的 `GemQualityStatsDef` 段。
//!
//! 数据来源：vendor PoB2 `Data/Skills/*.lua` 的 `qualityStats` 字段，
//! 由 `sync-pob-catalog extract-lua --what gem-quality` 确定性抽取生成
//! （schema 标识 `gem_quality_stats/v1`）。本表是纯查表（无与 base 同形的
//! merge 语义），按域整体加载，消费侧（pobr-build `BuildData`）建索引。

use pobr_data::catalog::GemQualityStatsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载宝石品质 stat 表（恒走 `overlay/` 定位）。文件缺失（旧数据包无此
    /// overlay 域）返回 `Ok(None)`——消费侧行为 = 品质不产生 stat（向后兼容）；
    /// 其余 IO / 解析错误照常上抛，不静默。
    pub fn gem_quality_stats(&self) -> Result<Option<GemQualityStatsDef>, LoadError> {
        match self.load_json_at::<GemQualityStatsDef>(self.overlay_path("gem_quality_stats.json")) {
            Ok(def) => Ok(Some(def)),
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::GameData;

    /// 创建带唯一名的临时版本目录。
    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pobr-gamedata-gem-quality-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// overlay 文件缺失（旧数据包）→ Ok(None)，不报错。
    #[test]
    fn missing_overlay_file_yields_none() {
        let dir = temp_dir("missing");
        let loaded = GameData::new(&dir).gem_quality_stats().unwrap();
        assert!(loaded.is_none());
    }

    /// 正常加载：`_meta` 头部忽略，effects 列表按序读出。
    #[test]
    fn loads_effects_and_ignores_meta() {
        let dir = temp_dir("loads");
        std::fs::create_dir_all(dir.join("overlay")).unwrap();
        std::fs::write(
            dir.join("overlay/gem_quality_stats.json"),
            r#"{
              "_meta": { "schema": "gem_quality_stats/v1" },
              "effects": [
                { "effect_id": "CometPlayer",
                  "stats": [ { "stat": "base_spell_%_chance_to_echo", "per_quality_rate": 0.5 } ] }
              ]
            }"#,
        )
        .unwrap();
        let def = GameData::new(&dir)
            .gem_quality_stats()
            .unwrap()
            .expect("overlay 存在应加载");
        assert_eq!(def.effects.len(), 1);
        assert_eq!(def.effects[0].effect_id, "CometPlayer");
        assert_eq!(def.effects[0].stats[0].stat, "base_spell_%_chance_to_echo");
        assert_eq!(def.effects[0].stats[0].per_quality_rate, 0.5);
    }

    /// 非法 JSON → 报 Parse 错误，不静默。
    #[test]
    fn malformed_json_errors_out() {
        let dir = temp_dir("malformed");
        std::fs::create_dir_all(dir.join("overlay")).unwrap();
        std::fs::write(dir.join("overlay/gem_quality_stats.json"), "{ not json").unwrap();
        assert!(GameData::new(&dir).gem_quality_stats().is_err());
    }
}
