//! 角色基础常量域 schema（`base/character_constants.json`，等级/属性派生常量）。
//!
//! 数值常量自 `pobr-core::character` 迁出（**公式逻辑仍留 Rust**，本表只承载
//! 数值；架构文档 20 §3.1）。pobr 没有的数值才从 vendor PoB2 抽取，逐字段在
//! doc 注明 vendor 文件:行号（vendor commit `2df5a74`，见 `vendor/.pob2-version.txt`）。
//!
//! 来源对照：
//! - pobr 准源：`crates/pobr-core/src/character.rs`（10 个常量，f23e88f 已对齐 PoB2）；
//! - vendor：`src/Data/Misc.lua:140` `data.characterConstants` 表 +
//!   `src/Modules/CalcSetup.lua:615-622` 角色基础段 +
//!   `src/Modules/CalcPerform.lua:420-443` 属性派生段 +
//!   `src/Modules/Data.lua:174` `AccuracyPerDexBase`。
//!
//! 边界说明：抗性上限 / 充能上限 / 暴伤基础等虽然在 vendor 同属
//! `data.characterConstants`，但按架构文档 20 §3.1 归 `game_constants.json`
//! 的 character 段，本表不收，避免双表重复定义。
//!
//! TODO（仅记录，不在本任务处理）：架构文档 20 §3.2 把本表列在 `overlay/`，
//! 而预注册的 `manifest.json` 把 `character_constants` 注册在 `base` 段；
//! 本实现以 manifest 为准落 `base/`，归属争议留给后续裁决。

use serde::{Deserialize, Serialize};

/// 角色等级/属性派生常量（单对象域，整文件就是一个本结构的 JSON object）。
///
/// 消费方为 `pobr-core::character`（`CharacterBase` 派生公式）：
/// - 固有生命 = `base_life_constant + life_per_level*level + life_per_strength*Str`
/// - 固有魔力 = `base_mana_constant + mana_per_level*level + mana_per_intelligence*Int`
/// - 固有精准 = `base_accuracy_constant + accuracy_per_level*level + accuracy_per_dexterity*Dex`
/// - 固有闪避 = `base_evasion`
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CharacterConstantsDef {
    /// 固有生命常量项（pobr 准源 `BASE_LIFE_CONSTANT = 16`；vendor
    /// `CalcSetup.lua:615` Multiplier 语义 `base = 16`）。
    pub base_life_constant: f64,
    /// 每等级生命（pobr 准源 `LIFE_PER_LEVEL = 12`；vendor `Data/Misc.lua:152`
    /// `life_per_level`）。
    pub life_per_level: f64,
    /// 每 1 点力量生命（pobr 准源 `LIFE_PER_STRENGTH = 2`；vendor
    /// `CalcPerform.lua:429` 字面量 `output.Str * 2`）。
    pub life_per_strength: f64,
    /// 固有魔力常量项（pobr 准源 `BASE_MANA_CONSTANT = 30`；vendor
    /// `CalcSetup.lua:616` `base = 30`）。
    pub base_mana_constant: f64,
    /// 每等级魔力（pobr 准源 `MANA_PER_LEVEL = 4`；vendor `Data/Misc.lua:153`
    /// `mana_per_level`）。
    pub mana_per_level: f64,
    /// 每 1 点智力魔力（pobr 准源 `MANA_PER_INTELLIGENCE = 2`；vendor
    /// `CalcPerform.lua:440` 字面量 `output.Int * 2`）。
    pub mana_per_intelligence: f64,
    /// 固有精准常量项（pobr 准源 `BASE_ACCURACY_CONSTANT = -6`；vendor
    /// `CalcSetup.lua:622` `base = -data.characterConstants["accuracy_rating_per_level"]`）。
    pub base_accuracy_constant: f64,
    /// 每等级精准（pobr 准源 `ACCURACY_PER_LEVEL = 6`；vendor
    /// `Data/Misc.lua:154` `accuracy_rating_per_level`）。
    pub accuracy_per_level: f64,
    /// 每 1 点敏捷精准（pobr 准源 `ACCURACY_PER_DEXTERITY = 6`；vendor
    /// `Modules/Data.lua:174` `AccuracyPerDexBase = 6`）。
    pub accuracy_per_dexterity: f64,
    /// 固有基础闪避（pobr 准源 `BASE_EVASION = 7`；vendor `Data/Misc.lua:151`
    /// `base_evasion_rating`）。
    pub base_evasion: f64,
    /// 每等级力量（vendor-only：`Data/Misc.lua:157` `strength_per_level = 0`）。
    pub strength_per_level: f64,
    /// 每等级敏捷（vendor-only：`Data/Misc.lua:158` `dexterity_per_level = 0`）。
    pub dexterity_per_level: f64,
    /// 每等级智力（vendor-only：`Data/Misc.lua:159` `intelligence_per_level = 0`）。
    pub intelligence_per_level: f64,
}
