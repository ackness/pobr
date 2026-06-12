//! 适配后入库的游戏数据目录（catalog）schema。
//!
//! 这是 PoBR **自有的最小 JSON schema**，由 `pobr-data-adapter` 从 GGG `.dat`
//! 原始导出（pathofexile-dat 产物）解析外键、反范式化后生成，落在仓库
//! `data/<poe_version>/`（三层布局：`base/` 全自动再生、`overlay/` vendor 抽取、
//! `generated/` 确定性缓存，见架构文档 20 §1 P1）。运行时由 loader
//! （`pobr-gamedata`）以 serde 加载。
//!
//! 设计目标：与 GGG 原始列名 / PoB 生成 Lua 解耦；只保留计算/显示需要的字段；
//! 稳定字符串 ID；版本可钉、diff 友好（数组按 id 排序）。
//!
//! 模块按数据域内聚拆分；所有类型经此处 `pub use` 全量 re-export，
//! 外部一律以 `pobr_data::catalog::X` 路径引用（拆分不破坏既有路径）。

pub mod items;
pub mod manifest;
pub mod mods;
pub mod skills;
pub mod tree;

// ---- M0-W2 九表 schema 空壳（防并行冲突预创建，W2 填充类型）----
pub mod base_player_mods;
pub mod character_constants;
pub mod enemy_presets;
pub mod game_constants;
pub mod jewel_radii;
pub mod monster_scaling;
pub mod non_damaging_ailments;
pub mod unarmed_data;
pub mod weapon_types;

// ---- M0-W3 注入管道：calc 消费的运行时常量包 ----
pub mod runtime;

// ---- M0-W4d 小查表 overlay schema（取整精度例外表 + 局部词条白名单）----
pub mod high_precision_mods;
pub mod local_mods;

// ---- M0-W4a per-skill 覆盖值 overlay schema（vendor Lua 抽取）----
pub mod skill_overrides;

// ---- M1-T2 SkillStatMap overlay schema（vendor Lua 抽取，skill_stat_map/v1）----
pub mod stat_map;

// ---- M2-D1 基底物品覆盖值 overlay schema（vendor Data/Bases 抽取：block/spirit）----
pub mod base_item_overrides;

pub use base_item_overrides::{BaseItemOverrideEntry, BaseItemOverridesDef};

// ---- M3 前置：受限 DSL 单点 schema + config 选项目录 + 内建 buff 定义 ----
pub mod buffs;
pub mod config_def;
pub mod value_expr;

// ---- M3 S1-C：curse 优先级数据表 overlay schema（vendor 抽取，curse_priority/v1）----
pub mod curse_priority;

// ---- pre-M5 数据前置（M5a/M5b/M5c 蓝图的数据生产项，零消费接线）----
pub mod actors; // M5a：minions / spectres / granted_effect_minions（vendor 抽取）
pub mod item_overlay; // M5c：mod_scalability / catalysts / runes / uniques（vendor 抽取）
pub mod parser_rules; // M5b：special_mods 模板（人工策展）+ M6：ModParser 解析规则六表（mod_parser_rules/v1）
pub mod triggers; // M5a-D2：mirage_configs（工具内嵌生成）；M4-T5 trigger_configs 同文件扩展

pub use actors::{
    GrantedEffectMinionDef, GrantedEffectMinionsDef, LuaValueDef, MinionEntryDef, MinionModDef,
    MinionsDef,
};
pub use item_overlay::{
    CatalystDef, CatalystsDef, ModScalabilityDef, ModScalabilityEntryDef, ModScalabilitySlotDef,
    RuneDef, RuneSlotDef, RunesDef, UniqueDef, UniquesDef,
};
pub use items::{ArmourBaseStats, BaseItemDef, WeaponBaseStats};
pub use manifest::{CATALOG_SCHEMA_VERSION, DataManifest, DomainSections};
pub use mods::{ModDef, ModStat, StatDef};
pub use parser_rules::{
    FlagPhraseDef, FlagTypeDef, FlagTypeModDef, FormDef, MOD_PARSER_RULES_SCHEMA,
    ModParserRulesDoc, ModTemplateDef, NameMapDef, PhraseNamesDef, PhraseValueDef, PreFlagDef,
    RuleEffectsDef, SpecialModsDef, SpecialTemplateDef, TagPhraseDef, TagTemplate, TemplateNameDef,
    TemplateScalarDef, TemplateTagDef, TemplateValueDef, ValueExprDef, ValueOpDef,
};
pub use runtime::RuntimeConstants;
pub use skills::{
    CostTypeDef, DotFlags, GemEffectDef, GemEffectsDef, GemQualityStatDef, GemQualityStatsDef,
    GrantedEffectDef, QualityStat, SkillDamageStat, SkillGemDef, SkillLevelDef, SkillStatSetDef,
    SkillStatSetLevel, StatSetDef, StatSetLabelDef, StatSetLabelsDef,
};
pub use tree::{
    PassiveAscendancy, PassiveClass, PassiveNodeDef, PassiveNodeKind, PassiveNodeVariant,
    PassiveTreeMeta,
};
pub use triggers::{
    MirageConfigDef, MirageConfigsDef, MirageSourceFilterDef, MirageTriggerDef, TriggerConfigDef,
    TriggerConfigsDef, TriggerKeyDef, TriggerSkillCondDef,
};
pub use unarmed_data::{UnarmedDataTable, UnarmedWeaponDef};
pub use weapon_types::{WeaponTypeDef, WeaponTypeTable};
