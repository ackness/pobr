//! `gen-mirage-configs` 子命令（pre-M5a 数据生产，M5a 蓝图 Track D2）：
//! 产 `overlay/mirage_configs.json`（5 类 mirage 配置）。
//!
//! vendor `Modules/CalcMirages.lua` 的五个分支是过程闭包，无法 luajit 序列化
//! ——配置数据**内嵌于本工具源码**，由工具落盘（满足「overlay 禁手改、只许
//! 工具再生」的字面要求；此取舍列 M5a 蓝图 §6 开放问题 2）。每条配置带
//! `vendor_ref` 行段锚点；vendor drift 由 `_meta.vendor_fingerprint`
//! （CalcMirages.lua 行数+字节数的粗粒度指纹）提醒——vendor bump 后重跑本
//! 命令，指纹变化即产生 byte diff，提示人工复核五分支语义。
//!
//! 真特殊分支逻辑走 `handler_id`（M5a 主波注册进 `pobr-core::rules::registry`，
//! 本波次零接线）。schema 见 [`pobr_data::catalog::triggers`]。

use std::fs;
use std::io;
use std::path::Path;

use serde::Serialize;

use pobr_data::catalog::triggers::{MirageConfigDef, MirageSourceFilterDef, MirageTriggerDef};

use crate::extract_lua::{ExtractLuaArgs, OverlayMeta, read_vendor_version, resolve_version_file};
use crate::extract_minions::to_pretty_json;

/// `_meta` 扩展：在通用 OverlayMeta 之上附加 vendor 粗粒度指纹。
#[derive(Debug, Serialize)]
struct MirageMeta {
    #[serde(flatten)]
    base: OverlayMeta,
    /// `Modules/CalcMirages.lua` 粗粒度指纹（`<行数>L:<字节数>B`）——
    /// vendor bump 后指纹变化 = 提示人工复核五分支语义（蓝图 D2 drift 提醒）。
    vendor_fingerprint: String,
}

#[derive(Debug, Serialize)]
struct MirageConfigsDoc {
    #[serde(rename = "_meta")]
    meta: MirageMeta,
    configs: Vec<MirageConfigDef>,
}

/// 5 条 mirage 配置（人工从 `Modules/CalcMirages.lua` 闭包转写，vendor commit
/// `2df5a74` 时点；行段锚点见各 `vendor_ref`）。
fn builtin_configs() -> Vec<MirageConfigDef> {
    let mut configs = vec![
        // Mirage Archer（CalcMirages.lua:63-119）：skillData.triggeredByMirageArcher；
        // 源 = 主技能本体（weaponData1.type == "Bow"）；count/less 三 stat 聚合；
        // 主技能进攻面板照算（calcMainSkillOffence = true）。
        MirageConfigDef {
            mirage_id: "mirage_archer".to_string(),
            trigger: MirageTriggerDef {
                skill_data_flag: Some("triggeredByMirageArcher".to_string()),
                granted_effect_name: None,
            },
            source_skill_filter: MirageSourceFilterDef {
                weapon_type: Some("Bow".to_string()),
                exclude_used_by_mirage: true,
                select: Some("main_skill".to_string()),
                ..Default::default()
            },
            count_stat: Some("MirageArcherMaxCount".to_string()),
            less_damage_stat: Some("MirageArcherLessDamage".to_string()),
            less_attack_speed_stat: Some("MirageArcherLessAttackSpeed".to_string()),
            cast_chance_stat: None,
            uses_stored_uses: true,
            calc_main_skill_offence: true,
            handler_id: None,
            vendor_ref: "Modules/CalcMirages.lua:63-119".to_string(),
        },
        // Saviour Mirage Warriors（:120-179）：授予效果名 "Reflection"；源 = 单手剑
        // 攻击技能里 DPS 最高者（GlobalCache 遍历）；双持同名武器时 count 减半
        // （handler）；mirage 输出整体替换主技能输出。
        MirageConfigDef {
            mirage_id: "saviour_mirage_warriors".to_string(),
            trigger: MirageTriggerDef {
                skill_data_flag: None,
                granted_effect_name: Some("Reflection".to_string()),
            },
            source_skill_filter: MirageSourceFilterDef {
                weapon_type: None,
                skill_types: vec!["Attack".to_string()],
                exclude_skill_types: vec!["Totem".to_string(), "SummonsTotem".to_string()],
                weapon_flags: vec!["Sword".to_string(), "Weapon1H".to_string()],
                exclude_used_by_mirage: true,
                select: Some("best_dps".to_string()),
            },
            count_stat: Some("SaviourMirageWarriorMaxCount".to_string()),
            less_damage_stat: Some("SaviourMirageWarriorLessDamage".to_string()),
            less_attack_speed_stat: None,
            cast_chance_stat: None,
            uses_stored_uses: true,
            calc_main_skill_offence: false,
            handler_id: Some("mirage:saviour_dual_wield_halving".to_string()),
            vendor_ref: "Modules/CalcMirages.lua:120-179".to_string(),
        },
        // Tawhoa's Chosen（:180-298）：授予效果名匹配；源 = Slam/Melee 攻击技能
        // DPS 最高者；触发冷却模型（icdr / 服务器帧取整 / 触发速率覆盖攻速）
        // 是真逻辑（handler）；less damage = ChieftainMirageChieftainMoreDamage。
        MirageConfigDef {
            mirage_id: "tawhoas_chosen".to_string(),
            trigger: MirageTriggerDef {
                skill_data_flag: None,
                granted_effect_name: Some("Tawhoa's Chosen".to_string()),
            },
            source_skill_filter: MirageSourceFilterDef {
                weapon_type: None,
                skill_types: vec!["Attack".to_string()],
                exclude_skill_types: vec![
                    "Vaal".to_string(),
                    "Totem".to_string(),
                    "SummonsTotem".to_string(),
                ],
                weapon_flags: vec![],
                exclude_used_by_mirage: true,
                select: Some("best_dps".to_string()),
            },
            count_stat: None,
            less_damage_stat: Some("ChieftainMirageChieftainMoreDamage".to_string()),
            less_attack_speed_stat: None,
            cast_chance_stat: None,
            uses_stored_uses: true,
            calc_main_skill_offence: false,
            handler_id: Some("mirage:tawhoa_trigger_rate".to_string()),
            vendor_ref: "Modules/CalcMirages.lua:180-298".to_string(),
        },
        // Sacred Wisps（:299-364）：skillData.triggeredBySacredWisps；源 = 主技能
        // 本体（weaponData1.type == "Wand"）；count/cast chance 取自 "Summon
        // Sacred Wisps" 技能的 stat（跨技能取数 = handler）。
        MirageConfigDef {
            mirage_id: "sacred_wisps".to_string(),
            trigger: MirageTriggerDef {
                skill_data_flag: Some("triggeredBySacredWisps".to_string()),
                granted_effect_name: None,
            },
            source_skill_filter: MirageSourceFilterDef {
                weapon_type: Some("Wand".to_string()),
                exclude_used_by_mirage: true,
                select: Some("main_skill".to_string()),
                ..Default::default()
            },
            count_stat: Some("SacredWispsMaxCount".to_string()),
            less_damage_stat: Some("SacredWispsLessDamage".to_string()),
            less_attack_speed_stat: None,
            cast_chance_stat: Some("SacredWispsChance".to_string()),
            uses_stored_uses: true,
            calc_main_skill_offence: true,
            handler_id: Some("mirage:sacred_wisps_source_skill".to_string()),
            vendor_ref: "Modules/CalcMirages.lua:299-364".to_string(),
        },
        // General's Cry（:365-419）：skillData.triggeredByGeneralsCry；不走
        // calculateMirage 子环境——主技能就地改造（冷却缩放 dpsMultiplier、
        // exert 系 mod 转写 Damage、QuantityMultiplier 注入），全程 handler。
        MirageConfigDef {
            mirage_id: "generals_cry".to_string(),
            trigger: MirageTriggerDef {
                skill_data_flag: Some("triggeredByGeneralsCry".to_string()),
                granted_effect_name: None,
            },
            source_skill_filter: MirageSourceFilterDef {
                exclude_used_by_mirage: true,
                select: Some("main_skill".to_string()),
                ..Default::default()
            },
            count_stat: Some("GeneralsCryDoubleMaxCount".to_string()),
            less_damage_stat: None,
            less_attack_speed_stat: None,
            cast_chance_stat: None,
            uses_stored_uses: false,
            calc_main_skill_offence: true,
            handler_id: Some("mirage:generals_cry_exert".to_string()),
            vendor_ref: "Modules/CalcMirages.lua:365-419".to_string(),
        },
    ];
    configs.sort_by(|a, b| a.mirage_id.cmp(&b.mirage_id));
    configs
}

/// 生成 `overlay/mirage_configs.json` 文本（byte-stable）。
pub fn run_gen_mirage_configs(args: &ExtractLuaArgs) -> io::Result<String> {
    let (commit, subject) = read_vendor_version(&resolve_version_file(args))?;
    let fingerprint = vendor_fingerprint(&args.vendor_root)?;
    let mut regen = "cargo run -p sync-pob-catalog -- gen-mirage-configs --vendor-root vendor/PathOfBuilding-PoE2/src".to_string();
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }
    let meta = MirageMeta {
        base: OverlayMeta {
            schema: "mirage_configs/v1".to_string(),
            generator: "sync-pob-catalog gen-mirage-configs（数据内嵌于工具源码）".to_string(),
            vendor: "PathOfBuilding-PoE2".to_string(),
            vendor_commit: commit,
            vendor_commit_subject: subject,
            extracted_files: vec!["Modules/CalcMirages.lua".to_string()],
            regen_command: regen,
        },
        vendor_fingerprint: fingerprint,
    };
    Ok(to_pretty_json(&MirageConfigsDoc {
        meta,
        configs: builtin_configs(),
    }))
}

/// CalcMirages.lua 粗粒度指纹：行数 + 字节数。
fn vendor_fingerprint(vendor_root: &Path) -> io::Result<String> {
    let path = vendor_root.join("Modules/CalcMirages.lua");
    let text = fs::read_to_string(&path).map_err(|error| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("无法读取 {}：{error}", path.display()),
        )
    })?;
    Ok(format!("{}L:{}B", text.lines().count(), text.len()))
}

#[cfg(test)]
mod tests {
    use super::builtin_configs;

    /// 5 条配置、id 唯一且升序；handler 条目带 `mirage:` 前缀
    /// （20-doc §5 handler 全局台账按前缀分域计数的约定）。
    #[test]
    fn builtin_configs_shape() {
        let configs = builtin_configs();
        assert_eq!(configs.len(), 5);
        let ids: Vec<&str> = configs.iter().map(|c| c.mirage_id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(ids, sorted, "mirage_id 必须唯一且升序");
        for config in &configs {
            if let Some(handler) = &config.handler_id {
                assert!(
                    handler.starts_with("mirage:"),
                    "handler id {handler} 缺 mirage: 前缀"
                );
            }
            assert!(
                config.trigger.skill_data_flag.is_some()
                    || config.trigger.granted_effect_name.is_some(),
                "{} 缺触发判定",
                config.mirage_id
            );
        }
    }
}
