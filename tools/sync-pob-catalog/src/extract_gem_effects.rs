//! `extract-lua --what gem-effects`：vendor PoB2 `Data/Gems.lua` 的宝石→授予效果
//! 连边 → `data/<版本>/overlay/gem_effects.json`（M1-T5.1，缺口 18-G5 数据面 +
//! 契约 C5 的 `SkillGemDef.granted_effect_id` 数据来源）。
//!
//! **通道说明**：蓝图原定从 `.dat` 表 `GemEffects` 走 adapter → `base/`，但该表
//! 所在 bundle 在钉定补丁 4.5.0.3.4 已无法下载（M1-W0 核验，见 `pipeline/config.json`
//! 的 `_tablesUnavailableForPinnedPatch`），按 owner 裁决「生产工具定层」
//! （00-index §4.2-1）：extract-lua 抽取 → **overlay/**。vendor `Data/Gems.lua` 本就
//! 是该表的导出产物（`Export/Scripts/skills.lua:898-925`），抽取为忠实转录。后续
//! `.dat` 表通道恢复则迁回 `base/`（迁移 commit byte 等价）。
//!
//! 职责切分与 [`crate::extract_lua`] 一致：Lua 引导脚本（`extract_gem_effects.lua`，
//! 编译期内嵌）只负责忠实抽取并以 JSONL 输出；Rust 侧统一做排序（gem_id 升序）、
//! 重复 gem_id 校验与整体文档序列化，保证同输入重跑 **byte-stable**。

use std::io;

use pobr_data::catalog::GemEffectDef;
use serde::{Deserialize, Serialize};

use crate::extract_lua::{
    ExtractLuaArgs, OverlayMeta, invoke_luajit_jsonl, read_vendor_version, resolve_version_file,
};

/// 引导脚本内容（经 stdin 注入 luajit，二进制自包含、不依赖运行目录）
const BOOTSTRAP_LUA: &str = include_str!("extract_gem_effects.lua");

/// 当前 overlay 文档 schema 标识（字段演化时递增）
pub const GEM_EFFECTS_SCHEMA: &str = "gem_effects/v1";

/// 引导脚本输出的单行 JSONL：一个宝石变体的效果连边。
#[derive(Debug, Clone, Deserialize)]
pub struct GemEffectRow {
    /// 宝石基底 id（vendor `gameId`）。
    pub gem_id: String,
    /// 效果变体 id（vendor `variantId`）。
    pub variant_id: String,
    /// 授予的主效果 id。
    pub granted_effect: String,
    /// 附加授予效果（`additionalGrantedEffectId1..N` 序）。
    #[serde(default)]
    pub additional_granted_effects: Vec<String>,
    /// 主效果的附加 statSet（`additionalStatSet1..N` 序）。
    #[serde(default)]
    pub additional_stat_sets: Vec<String>,
}

/// 完整 overlay 文档（生成侧；消费侧 schema 见
/// [`pobr_data::catalog::GemEffectsDef`]，serde 形状一致防字段漂移）。
#[derive(Debug, Serialize, Deserialize)]
pub struct GemEffectsDoc {
    /// 头部元信息（serde 落为 `_meta`，置于文件最前）
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// 宝石→效果连边表，按 gem_id 升序。
    pub gems: Vec<GemEffectDef>,
}

/// 执行抽取，返回最终（byte-stable 的）JSON 文本。
pub fn run_extract_gem_effects(args: &ExtractLuaArgs) -> io::Result<String> {
    let rows: Vec<GemEffectRow> = invoke_luajit_jsonl(args, BOOTSTRAP_LUA)?;
    let meta = build_meta(args)?;
    assemble_gem_effects_document(meta, rows)
}

/// 组装最终文档：按 gem_id 升序 + 重复 gem_id 校验（当前数据 1 gem ↔ 1 变体；
/// 出现重复说明 vendor 数据形态变化，报错而非静默取一）+ serde_json 统一序列化。
pub fn assemble_gem_effects_document(
    meta: OverlayMeta,
    rows: Vec<GemEffectRow>,
) -> io::Result<String> {
    let mut gems: Vec<GemEffectDef> = rows
        .into_iter()
        .map(|row| GemEffectDef {
            gem_id: row.gem_id,
            variant_id: row.variant_id,
            granted_effect_id: row.granted_effect,
            additional_granted_effect_ids: row.additional_granted_effects,
            additional_stat_set_ids: row.additional_stat_sets,
        })
        .collect();
    gems.sort_by(|a, b| a.gem_id.cmp(&b.gem_id));
    if let Some(dup) = gems.windows(2).find(|w| w[0].gem_id == w[1].gem_id) {
        return Err(io::Error::other(format!(
            "gem_effects 抽取发现重复 gem_id `{}`（变体 {} / {}）——vendor 数据形态已变化，\
             消费侧 1:1 merge 假设失效，需要先调整 schema",
            dup[0].gem_id, dup[0].variant_id, dup[1].variant_id
        )));
    }
    let doc = GemEffectsDoc { meta, gems };
    let mut json = serde_json::to_string_pretty(&doc).expect("gem effects 文档序列化不应失败");
    json.push('\n');
    Ok(json)
}

/// 构建 `_meta`（与公共层同约定：regen_command 写 canonical 相对路径）。
fn build_meta(args: &ExtractLuaArgs) -> io::Result<OverlayMeta> {
    let (commit, subject) = read_vendor_version(&resolve_version_file(args))?;
    let mut regen = "cargo run -p sync-pob-catalog -- extract-lua --what gem-effects \
         --vendor-root vendor/PathOfBuilding-PoE2/src"
        .to_string();
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }
    Ok(OverlayMeta {
        schema: GEM_EFFECTS_SCHEMA.to_string(),
        generator: "sync-pob-catalog extract-lua".to_string(),
        vendor: "PathOfBuilding-PoE2".to_string(),
        vendor_commit: commit,
        vendor_commit_subject: subject,
        // 本目标恒读单文件（公共调用层的 --files 仅占位，不参与定位）。
        extracted_files: vec!["Data/Gems.lua".to_string()],
        regen_command: regen,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> OverlayMeta {
        OverlayMeta {
            schema: GEM_EFFECTS_SCHEMA.into(),
            generator: "test".into(),
            vendor: "PathOfBuilding-PoE2".into(),
            vendor_commit: "0".repeat(40),
            vendor_commit_subject: "subject".into(),
            extracted_files: vec!["Data/Gems.lua".into()],
            regen_command: "cargo run …".into(),
        }
    }

    fn row(gem: &str, variant: &str, granted: &str) -> GemEffectRow {
        GemEffectRow {
            gem_id: gem.into(),
            variant_id: variant.into(),
            granted_effect: granted.into(),
            additional_granted_effects: vec![],
            additional_stat_sets: vec![],
        }
    }

    /// 排序确定性：乱序输入 → gem_id 升序输出；同输入重跑 byte 一致。
    #[test]
    fn sorts_by_gem_id_and_is_byte_stable() {
        let rows = vec![row("B", "b", "BPlayer"), row("A", "a", "APlayer")];
        let one = assemble_gem_effects_document(meta(), rows).unwrap();
        let rows2 = vec![row("A", "a", "APlayer"), row("B", "b", "BPlayer")];
        let two = assemble_gem_effects_document(meta(), rows2).unwrap();
        assert_eq!(one, two);
        let a = one.find("\"gem_id\": \"A\"").unwrap();
        let b = one.find("\"gem_id\": \"B\"").unwrap();
        assert!(a < b);
    }

    /// 重复 gem_id（vendor 1:1 假设破裂）→ 报错不静默。
    #[test]
    fn duplicate_gem_id_errors_out() {
        let rows = vec![row("A", "a1", "APlayer"), row("A", "a2", "APlayerTwo")];
        assert!(assemble_gem_effects_document(meta(), rows).is_err());
    }

    /// 消费侧 schema（GemEffectsDef）能读回生成侧文档（serde 形状一致防漂移）。
    #[test]
    fn consumer_schema_roundtrip() {
        let mut entry = row("A", "a", "APlayer");
        entry.additional_granted_effects = vec!["AExtraPlayer".into()];
        entry.additional_stat_sets = vec!["AOnFrostbolt".into()];
        let json = assemble_gem_effects_document(meta(), vec![entry]).unwrap();
        let def: pobr_data::catalog::GemEffectsDef = serde_json::from_str(&json).unwrap();
        assert_eq!(def.gems.len(), 1);
        assert_eq!(def.gems[0].granted_effect_id, "APlayer");
        assert_eq!(def.gems[0].additional_granted_effect_ids, ["AExtraPlayer"]);
        assert_eq!(def.gems[0].additional_stat_set_ids, ["AOnFrostbolt"]);
    }
}
