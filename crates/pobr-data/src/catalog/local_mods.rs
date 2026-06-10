//! 局部词条白名单域 schema（`overlay/local_mods.json`）。
//!
//! pobr 准源（搬迁不变式：JSON 值与此处硬编码逐值相等）：
//! `crates/pobr-build/src/calc_orchestrator.rs::is_weapon_local_mod` 的
//! 文本枚举——武器槽上命中即从全局 modifier 池剔除（已计入武器 source
//! 独立乘区，留在全局会重复且错误地并入加法桶）：
//! 1. clean 文本以 `% increased physical damage` 结尾；
//! 2. clean 文本以 `% increased attack speed` 结尾；
//! 3. `adds N to M physical damage` 形式（`parse_adds_physical`）。
//!
//! vendor 对照（差异仅记录、不改值——见 audits/rearchitecture-2026-06-10/16-items.md
//! 「pobr 当前混合点的自查」）：PoB2 的局部性判定是 `src/Classes/Item.lua:1655-1682`
//! `calcLocal` 的**结构化规则**（mod name + flag 精确匹配、keywordFlags == 0、
//! 无 tag 或仅 InSlot tag），并非文本枚举；架构文档 20 §2.4 的终局方向是
//! 「local_mods.json 白名单 + 结构化局部结算」。M0 阶段先把现有文本枚举
//! 原样数据化（值不变 = parity 不变），结构化迁移留后续波次。
//!
//! 注意 clean 文本 = `clean_item_text` 产物（剥 `{...}` 标记 + trim + **小写**），
//! 故白名单条目一律小写英文。

use serde::{Deserialize, Serialize};

/// `overlay/local_mods.json` 顶层（单对象域）。
///
/// 目前仅武器段；护甲件局部防御（`parse_local_defence_inc/flat`）是结构化
/// form 解析而非白名单枚举，不入本表（后续随结构化局部结算一并裁决）。
///
/// `Default` 即内建 fallback（递归取 [`WeaponLocalModsDef::default`]）：与
/// `data/<ver>/overlay/local_mods.json` 逐值一致的镜像，旧数据包无本 overlay
/// 文件时的降级路径；由 pobr-gamedata 加载测试锁定一致性。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalModsDef {
    /// 武器局部词条判定规则。
    pub weapon: WeaponLocalModsDef,
}

/// 武器局部词条白名单（`is_weapon_local_mod` 的数据形态）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeaponLocalModsDef {
    /// `ends_with` 命中即局部的 clean 文本后缀（小写）。
    pub increased_suffixes: Vec<String>,
    /// `adds N to M <suffix>` 形式中视为局部的伤害后缀（小写，不含首空格）。
    pub adds_damage_suffixes: Vec<String>,
}

/// 内建 fallback 值 = `is_weapon_local_mod` 原硬编码枚举（搬迁不变式准源）。
impl Default for WeaponLocalModsDef {
    fn default() -> Self {
        Self {
            increased_suffixes: vec![
                "% increased physical damage".to_string(),
                "% increased attack speed".to_string(),
            ],
            adds_damage_suffixes: vec!["physical damage".to_string()],
        }
    }
}
