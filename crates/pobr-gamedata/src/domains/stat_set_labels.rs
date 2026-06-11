//! `overlay/stat_set_labels.json` loader——statSet 形态 label + vendor 导出序号
//! （`(skill, set_id) → {set_index, label}`），schema 见
//! [`pobr_data::catalog::skills`] 的 `StatSetLabelDef` 段（M1-T5.2 多 statSet）。
//!
//! 数据来源：vendor `Data/Skills/*.lua`（label 文本）join `Export/Skills/*.txt`
//! 模板（set id / 导出序号），由 `sync-pob-catalog extract-lua --what
//! stat-set-labels` 确定性抽取（`.dat` `Label` 列的 FK 目标表
//! `GrantedEffectLabels` 在钉定补丁不可下载）。
//!
//! 消费方式：[`crate::GameData::skill_stat_sets`] 在加载 base 多 set 域后按
//! `(effect_id, set_id)` merge 填充 `StatSetDef::label` / `vendor_set_index`。

use pobr_data::catalog::StatSetLabelsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载 statSet label 表（恒走 `overlay/` 定位）。文件缺失（旧数据包无此
    /// overlay 域）返回 `Ok(None)`——消费侧行为 = label / 导出序号保持空（向后
    /// 兼容）；其余 IO / 解析错误照常上抛，不静默。
    pub fn stat_set_labels(&self) -> Result<Option<StatSetLabelsDef>, LoadError> {
        match self.load_json_at::<StatSetLabelsDef>(self.overlay_path("stat_set_labels.json")) {
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

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pobr-gamedata-stat-set-labels-{tag}-{}",
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
        assert!(GameData::new(&dir).stat_set_labels().unwrap().is_none());
    }

    /// 正常加载：`_meta` 头部忽略，labels 列表按序读出。
    #[test]
    fn loads_labels_and_ignores_meta() {
        let dir = temp_dir("loads");
        std::fs::create_dir_all(dir.join("overlay")).unwrap();
        std::fs::write(
            dir.join("overlay/stat_set_labels.json"),
            r#"{
              "_meta": { "schema": "stat_set_labels/v1" },
              "labels": [
                { "skill": "IceNovaPlayer", "set_id": "IceNovaColdInfusedPlayer",
                  "set_index": 2, "label": "Cold-Infused" }
              ]
            }"#,
        )
        .unwrap();
        let def = GameData::new(&dir)
            .stat_set_labels()
            .unwrap()
            .expect("overlay 存在应加载");
        assert_eq!(def.labels.len(), 1);
        assert_eq!(def.labels[0].set_index, 2);
        assert_eq!(def.labels[0].label, "Cold-Infused");
    }
}
