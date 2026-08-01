//! `overlay/gem_effects.json` loader——宝石→授予效果连边
//! （`gem_id → {granted_effect_id, additional_granted_effect_ids, additional_stat_set_ids}`），
//! schema 见 [`pobr_data::catalog::skills`] 的 `GemEffectDef` 段（
//! 数据面 + 契约 C5 的 `SkillGemDef` 连边来源）。
//!
//! 数据来源：vendor PoB2 `Data/Gems.lua`（`.dat` `GemEffects` 表的导出产物，该表
//! bundle 在钉定补丁不可下载），由 `sync-pob-catalog extract-lua --what gem-effects`
//! 确定性抽取生成（schema 标识 `gem_effects/v1`）。
//!
//! 消费方式：[`crate::GameData::skill_gems`] 在加载 base 宝石表后按 `gem_id` merge
//! 填充 `SkillGemDef::granted_effect_id` / `additional_granted_effect_ids`；
//! pobr-build `BuildData` 另按 `granted_effect_id` 建索引供 meta 展开（T5.6）。

use pobr_data::catalog::GemEffectsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载宝石→效果连边表（恒走 `overlay/` 定位）。文件缺失（旧数据包无此
    /// overlay 域）返回 `Ok(None)`——消费侧行为 = 连边字段保持空（向后兼容）；
    /// 其余 IO / 解析错误照常上抛，不静默。
    pub fn gem_effects(&self) -> Result<Option<GemEffectsDef>, LoadError> {
        match self.load_json_at::<GemEffectsDef>(self.overlay_path("gem_effects.json")) {
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
            "pobr-gamedata-gem-effects-{tag}-{}",
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
        let loaded = GameData::new(&dir).gem_effects().unwrap();
        assert!(loaded.is_none());
    }

    /// 正常加载：`_meta` 头部忽略，gems 列表按序读出。
    #[test]
    fn loads_gems_and_ignores_meta() {
        let dir = temp_dir("loads");
        std::fs::create_dir_all(dir.join("overlay")).unwrap();
        std::fs::write(
            dir.join("overlay/gem_effects.json"),
            r#"{
              "_meta": { "schema": "gem_effects/v1" },
              "gems": [
                { "gem_id": "Metadata/Items/Gems/SkillGemIceNova",
                  "variant_id": "IceNova",
                  "granted_effect_id": "IceNovaPlayer",
                  "additional_stat_set_ids": ["IceNovaPlayerOnFrostbolt"] }
              ]
            }"#,
        )
        .unwrap();
        let def = GameData::new(&dir)
            .gem_effects()
            .unwrap()
            .expect("overlay 存在应加载");
        assert_eq!(def.gems.len(), 1);
        assert_eq!(def.gems[0].granted_effect_id, "IceNovaPlayer");
        assert!(def.gems[0].additional_granted_effect_ids.is_empty());
        assert_eq!(
            def.gems[0].additional_stat_set_ids,
            ["IceNovaPlayerOnFrostbolt"]
        );
    }

    /// 非法 JSON → 报 Parse 错误，不静默。
    #[test]
    fn malformed_json_errors_out() {
        let dir = temp_dir("malformed");
        std::fs::create_dir_all(dir.join("overlay")).unwrap();
        std::fs::write(dir.join("overlay/gem_effects.json"), "{ not json").unwrap();
        assert!(GameData::new(&dir).gem_effects().is_err());
    }
}
