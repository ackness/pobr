//! M0 按域 loader：W2 九张常量表（`base/`）+ W4d 两张小查表（`overlay/`），
//! 与 `pobr_data::catalog` 各域 schema 对应。
//!
//! 每个子模块实现 `GameData` 上的对应加载方法（base 域走 `base/` 优先、
//! 版本根回退定位；overlay 域恒走 `overlay/` 定位）。

pub mod base_player_mods;
pub mod character_constants;
pub mod enemy_presets;
pub mod game_constants;
pub mod jewel_radii;
pub mod monster_scaling;
pub mod non_damaging_ailments;
pub mod unarmed_data;
pub mod weapon_types;

// ---- M0-W4d 小查表（overlay 层）----
pub mod high_precision_mods;
pub mod local_mods;

// ---- M0-W4a per-skill 覆盖值（overlay 层，loader + 专用 merge）----
pub mod skill_overrides;

// ---- M1-T1 宝石品质 stat 斜率（overlay 层，纯查表）----
pub mod gem_quality_stats;

// ---- M1-T2 SkillStatMap 映射表（overlay 层，消费侧 = stat_map_engine）----
pub mod skill_stat_map;

// ---- M1-T5.1 宝石→授予效果连边（overlay 层，merge 进 skill_gems + meta 展开索引）----
pub mod gem_effects;

// ---- M1-T5.2 statSet label / vendor 导出序号边车（overlay 层，merge 进 stat sets）----
pub mod stat_set_labels;

// ---- M2-D1 基底物品覆盖值（overlay 层，loader + 专用 merge：block/spirit）----
pub mod base_item_overrides;

// ---- M3 前置：config 选项目录 + 内建 buff 定义（overlay 层，纯查表）----
pub mod buff_definitions;
pub mod config_options;

// ---- M3 S1-C：curse 优先级数据表（overlay 层，纯查表；消费侧 = M3-T3 buff_pass）----
pub mod curse_priority;

// ---- pre-M5 数据前置（overlay 层，纯 loader 零接线；一表一文件）----
pub mod catalysts; // M5c：催化剂品质标签匹配表
pub mod granted_effect_minions; // M5a：宝石→召唤物外键边车
pub mod minions; // M5a：召唤物条目
pub mod mirage_configs; // M5a-D2：mirage 配置
pub mod mod_scalability; // M5c：{range:x} 可缩放性表
pub mod runes; // M5c：符文/魂核词条表
pub mod special_mods; // M5b：special 词条模板
pub mod spectres; // M5a：魂灵条目
pub mod uniques; // M5c：传奇 raw+索引双层

// ---- M6 前置：ModParser 解析规则六表（overlay 层，消费侧 = M6-B scan 引擎）----
pub mod parser_rules;

// ---- M4-T5 W-E1：触发配置 61 项（overlay 层，消费侧 = orchestrator 触发段）----
pub mod trigger_configs;
