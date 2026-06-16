//! `overlay/skill_stat_map.json` loader——SkillStatMap 全局 + per-statSet 覆盖
//! （stat id → modifier 构造器映射），schema 见 [`pobr_data::catalog::stat_map`]
//! （M1-T2，缺口 18-G3 / 15-G2）。
//!
//! 数据来源：vendor PoB2 `Data/SkillStatMap.lua` + `Data/Skills/*.lua` 各
//! statSet 的 `statMap` 字段，由 `sync-pob-catalog extract-lua --what stat-map`
//! 确定性抽取生成（schema 标识 `skill_stat_map/v1`）。本表按域整体加载；
//! 消费侧 = `pobr-core::rules::stat_map_engine`（merge 公式 + ModName 翻译 +
//! tag 支持性判定，本 loader 零语义）。

use pobr_data::catalog::stat_map::SkillStatMapDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载 SkillStatMap 映射表（恒走 `overlay/` 定位）。文件缺失（旧数据包无此
    /// overlay 域）返回 `Ok(None)`——消费侧行为 = 数据引擎不可用（双跑框架只能
    /// 走 Legacy，向后兼容）；其余 IO / 解析错误照常上抛，不静默。
    pub fn skill_stat_map(&self) -> Result<Option<SkillStatMapDef>, LoadError> {
        match self.load_json_at::<SkillStatMapDef>(self.overlay_path("skill_stat_map.json")) {
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
            "pobr-gamedata-stat-map-{tag}-{}",
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
        let loaded = GameData::new(&dir).skill_stat_map().unwrap();
        assert!(loaded.is_none());
    }

    /// 正常加载：`_meta` 头部忽略，global / per_stat_set 两段读出，
    /// merge 参数与 mod 构造器字段形状对齐 schema。
    #[test]
    fn loads_global_and_per_set_and_ignores_meta() {
        let dir = temp_dir("loads");
        std::fs::create_dir_all(dir.join("overlay")).unwrap();
        std::fs::write(
            dir.join("overlay/skill_stat_map.json"),
            r#"{
              "_meta": { "schema": "skill_stat_map/v1" },
              "global": {
                "total_cast_time_+_ms": {
                  "div": 1000.0,
                  "mods": [ { "kind": "mod", "name": "TotalCastTime", "mod_type": "BASE" } ]
                },
                "broken_entry": { "_unextractable": true }
              },
              "per_stat_set": {
                "IceNovaPlayer": {
                  "1": {
                    "damage_+%": {
                      "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC" } ]
                    }
                  }
                }
              }
            }"#,
        )
        .unwrap();
        let def = GameData::new(&dir)
            .skill_stat_map()
            .unwrap()
            .expect("overlay 存在应加载");
        let entry = &def.global["total_cast_time_+_ms"];
        assert_eq!(entry.div, Some(1000.0));
        assert_eq!(entry.mods[0].name.as_deref(), Some("TotalCastTime"));
        assert_eq!(entry.mods[0].mod_type.as_deref(), Some("BASE"));
        assert!(def.global["broken_entry"].unextractable);
        assert!(def.per_stat_set["IceNovaPlayer"]["1"].contains_key("damage_+%"));
    }

    /// 非法 JSON → 报 Parse 错误，不静默。
    #[test]
    fn malformed_json_errors_out() {
        let dir = temp_dir("malformed");
        std::fs::create_dir_all(dir.join("overlay")).unwrap();
        std::fs::write(dir.join("overlay/skill_stat_map.json"), "{ not json").unwrap();
        assert!(GameData::new(&dir).skill_stat_map().is_err());
    }

    /// 真实数据包冒烟：仓库内 4.5.0.3.4 的 overlay 能整体反序列化，规模符合
    /// vendor 量级（global 950+，per-set 覆盖 300+ 效果）。
    #[test]
    fn loads_repo_overlay_smoke() {
        let root = crate::current_data_dir();
        if !root.join("overlay/skill_stat_map.json").exists() {
            return; // 数据包未就位时跳过（不在 CI 数据缺失时误报）
        }
        let def = GameData::new(&root)
            .skill_stat_map()
            .unwrap()
            .expect("repo overlay 存在");
        assert!(def.global.len() >= 950, "global={}", def.global.len());
        assert!(
            def.per_stat_set.len() >= 300,
            "per_set effects={}",
            def.per_stat_set.len()
        );
    }
}
