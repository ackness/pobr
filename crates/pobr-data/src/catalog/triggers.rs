//! mirage（幻影）配置域 schema（`overlay/mirage_configs.json`）。
//!
//! 数据来源（M5a 蓝图 Track D，缺口 14-#7）：vendor PoB2
//! `Modules/CalcMirages.lua` 的五个分支（Mirage Archer / Saviour Mirage
//! Warriors / Tawhoa's Chosen / Sacred Wisps / General's Cry）——分支体是
//! 过程闭包无法 luajit 序列化，5 条配置由 `sync-pob-catalog gen-mirage-configs`
//! **内嵌于工具源码**后落盘（满足「overlay 禁手改、只许工具再生」；vendor drift
//! 由 `_meta` 内记录的 CalcMirages.lua 粗粒度指纹提醒，M5a 蓝图 §6 开放问题 2）。
//!
//! 真特殊分支逻辑（Tawhoa 的触发冷却模型 / General's Cry 的 exert 转写等）走
//! `handler_id`（注册进 `pobr-core::rules::registry`，遵守 20-doc §5 handler
//! 总数 <100 监控）；本模块只定义 serde 形状，零逻辑。
//!
//! 本文件同时是 M4-T5 `trigger_configs.json` 的 schema 落点（届时扩展同文件，
//! M5a 蓝图 §2 D2：schema 统一归 catalog 守门）。

use serde::{Deserialize, Serialize};

/// mirage 触发判定（vendor `calcs.mirages` 的 if-elseif 链中该分支的命中条件，
/// 两字段二选一）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirageTriggerDef {
    /// 主技能 `skillData` 上的触发旗标（如 `triggeredByMirageArcher`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_data_flag: Option<String>,
    /// 主技能授予效果名精确匹配（如 Saviour 的 `Reflection`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub granted_effect_name: Option<String>,
}

/// mirage 源技能筛选（vendor `config.compareFunc` 的可数据化部分）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirageSourceFilterDef {
    /// 要求主手武器类型（如 `Bow` / `Wand`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapon_type: Option<String>,
    /// 要求技能类型（全部命中，如 `["Attack"]`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_types: Vec<String>,
    /// 任一命中即排除的技能类型（如 `["Totem", "SummonsTotem"]`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_skill_types: Vec<String>,
    /// 要求技能 cfg flags 含全部位名（如 Saviour 的 `["Sword", "Weapon1H"]`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub weapon_flags: Vec<String>,
    /// 排除已被 mirage 使用的技能（递归防护，vendor `usedByMirage` 条件）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub exclude_used_by_mirage: bool,
    /// 源技能选择策略：`main_skill`（mirage 复制主技能本身）或
    /// `best_dps`（遍历技能列表取 DPS 最高者，vendor GlobalCache 路径）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select: Option<String>,
}

/// 一类 mirage 的配置（对应 `Modules/CalcMirages.lua` 一个分支）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirageConfigDef {
    /// 稳定 id（snake_case，如 `mirage_archer`）。
    pub mirage_id: String,
    /// 触发判定。
    pub trigger: MirageTriggerDef,
    /// 源技能筛选。
    #[serde(default)]
    pub source_skill_filter: MirageSourceFilterDef,
    /// mirage 数量 stat 名（`Sum("BASE", …)` 聚合，如 `MirageArcherMaxCount`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count_stat: Option<String>,
    /// less damage stat 名（注入 `Damage MORE`，如 `MirageArcherLessDamage`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub less_damage_stat: Option<String>,
    /// less attack speed stat 名（注入 `Speed MORE`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub less_attack_speed_stat: Option<String>,
    /// 施放几率 stat 名（Sacred Wisps：`Speed MORE (chance-100)`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cast_chance_stat: Option<String>,
    /// 子环境继承主技能 `storedUses`（vendor `mirageUses = storedUses`）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub uses_stored_uses: bool,
    /// 主技能进攻面板继续按本体计算（vendor `calcMainSkillOffence`；false =
    /// mirage 输出整体替换主技能输出，如 Saviour / Tawhoa）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub calc_main_skill_offence: bool,
    /// 无法数据化的真特殊分支逻辑的 handler 稳定 id（如 Tawhoa 的触发冷却
    /// 模型）；`None` = 纯配置可驱动。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_id: Option<String>,
    /// vendor 出处（`Modules/CalcMirages.lua` 行段，人工核对锚点）。
    pub vendor_ref: String,
}

/// `overlay/mirage_configs.json` 顶层（消费侧忽略 `_meta`）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MirageConfigsDef {
    /// 配置列表，按 `mirage_id` 升序。
    pub configs: Vec<MirageConfigDef>,
}

// ===========================================================================
// M4-T5 W-E1：trigger_configs（vendor CalcTriggers.lua configTable 61 项）
// ===========================================================================

/// 触发配置的匹配键（vendor `CalcTriggers.lua:1452-1455` 四级查找：
/// 技能名 → triggeredBy 名 → awakened 归一名 → unique 物品名，全部 lowercase）。
///
/// `kind` 标注该 key 在 vendor 中的来源类别（用于消费侧选择 join 路径）；
/// awakened 归一（`gsub("^awakened ", "")`）后与 `triggered_by` 同名，不单列。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerKeyDef {
    /// key 类别：`skill`（主技能名命中）/ `triggered_by`（触发 support/meta 宝石名
    /// 命中）/ `unique_item`（unique 物品触发名命中）。
    pub kind: String,
    /// vendor configTable 的字面 key（lowercase）。
    pub name: String,
}

/// 受限技能谓词（R1 纪律：**三字段封顶**，字段引用 + any/all/not，禁自由表达式；
/// 扩能力需 ≥20 条目受益——20 号 §5 闸门，否则落 `handler_id`）。
///
/// 语义对照 vendor 闭包：
/// - `any_skill_types`：`skill.skillTypes[A] or skill.skillTypes[B]`（任一命中）；
/// - `all_mod_flags`：`band(skill.skillCfg.flags, X) > 0` 的**意图**转写——vendor
///   多 flag 写法 `band(flags, bor(Mace, Weapon1H)) > 0` 字面是 any-of，但 1H 武器
///   攻击的 skillCfg.flags 同时携带「武器类 + 持握」两位，all-of 更贴合条目意图
///   （如 Mjolner 要求**单手**锤）；单 flag 条目两种语义等价；
/// - `not_skill_types`：`not skill.skillTypes[X]`（任一命中即排除）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerSkillCondDef {
    /// 任一技能类型命中即通过（如 `["Melee", "Attack"]`）。空 = 不约束。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub any_skill_types: Vec<String>,
    /// 技能 cfg flags 须含全部位名（如 `["Claw"]` / `["Bow"]`）。空 = 不约束。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub all_mod_flags: Vec<String>,
    /// 任一技能类型命中即排除（如 `["SummonsTotem"]`）。空 = 不排除。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_skill_types: Vec<String>,
}

impl TriggerSkillCondDef {
    /// 谓词是否为空（不约束任何字段）。
    pub fn is_empty(&self) -> bool {
        self.any_skill_types.is_empty()
            && self.all_mod_flags.is_empty()
            && self.not_skill_types.is_empty()
    }
}

/// 一条触发配置（对应 vendor `Modules/CalcTriggers.lua:881-1417` configTable
/// 的一个条目；61 项全量转写，drift 由工具的 key 扫描对账防御）。
///
/// ~90% 条目是声明性事实（触发名 / 谓词 / 冷却覆盖 / 速率上限覆盖 / global 标记
/// / disable 条件），少数带真逻辑（Mjolner 双源、Snipe 蓄力等）→ `handler_id`
/// （计数监控 <100，20 号 §5）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerConfigDef {
    /// 匹配键（四级查找的字面 key + 类别）。
    pub key: TriggerKeyDef,
    /// 触发显示名覆盖（vendor `triggerName`，如 Limbsplit 的 "Gore Shockwave"）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger_name: Option<String>,
    /// 源速率取 use/cast 触发口径（vendor `triggerOnUse`：源每次**使用**触发，
    /// 不折命中/暴击）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub trigger_on_use: bool,
    /// 源速率用施法速率（vendor `useCastRate`）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub use_cast_rate: bool,
    /// 触发**源技能**谓词（vendor `triggerSkillCond` 的受限转写）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_skill_cond: Option<TriggerSkillCondDef>,
    /// **被触发技能**谓词（vendor `triggeredSkillCond` 的受限转写；vendor 固有的
    /// `triggeredBy*` skillData 旗标 + 同槽位要求是该谓词的默认语义，不重复编码）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triggered_skill_cond: Option<TriggerSkillCondDef>,
    /// 源技能按**授予效果名精确匹配**（vendor 闭包内
    /// `skill.activeEffect.grantedEffect.name == X`，如 Automation/Spellslinger）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_skill_name: Option<String>,
    /// 仅当主技能名等于此值时本条目生效（vendor 条目内
    /// `env.player.mainSkill...name == X` 门控，如 The Rippling Thoughts）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_main_skill_name: Option<String>,
    /// 触发几率 stat 名（vendor `triggerChance = modDB:Sum("BASE", nil, X)` /
    /// skillData 字段名，如 Kitava 的 `KitavaTriggerChance`、Cast when Stunned 的
    /// `chanceToTriggerOnStun`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger_chance_stat: Option<String>,
    /// 触发源速率 stat 名（vendor `trigRate = modDB:Sum("BASE", nil, X)`，
    /// 如 Intuitive Link 的 `IntuitiveLinkSourceRate`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_rate_stat: Option<String>,
    /// 冷却覆盖（秒；vendor `skillData.cooldown = N` 就地覆盖，如 Lioneye's Paws）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_override_s: Option<f64>,
    /// 触发速率上限覆盖（次/秒；vendor `skillData.triggerRateCapOverride`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger_rate_cap_override: Option<f64>,
    /// 全局触发（vendor `skillFlags.globalTrigger`：不依赖源技能速率，
    /// EffectiveSourceRate 取 TriggerRateCap）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub global_trigger: bool,
    /// 源 = 被触发技能自身（vendor `return {source = env.player.mainSkill}`）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub source_is_self: bool,
    /// 源速率不再叠加任何折算（vendor `skillData.sourceRateIsFinal`）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub source_rate_is_final: bool,
    /// 触发不受服务器帧取整（vendor `skillData.ignoresTickRate`）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ignores_tick_rate: bool,
    /// 按「每次命中都击杀」假设（vendor `assumingEveryHitKills`，on-kill 触发）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub assuming_every_hit_kills: bool,
    /// 忽略源速率门控（vendor `ignoreSourceRate`，Avenging Flame）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ignore_source_rate: bool,
    /// 触发几率折入**源暴击率**（vendor `skillData.triggerOnCrit`，CoC 链路；
    /// 来源 = SkillStatMap 对 triggeredByCoc 技能的固定旗标，转写为本条目事实）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub trigger_on_crit: bool,
    /// 生效前置 condition（vendor `modDB:Flag(nil, "Condition:X")` 门控，
    /// 如 The Hidden Blade 的 `Phasing`、Cast on Melee Kill 的 `KilledRecently`；
    /// 不满足时 vendor 置 disable / 退化自施法）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_condition: Option<String>,
    /// PoBR 侧 join 键：与该条目对应的 PoE2 授予效果 id（`GrantedEffects.Id`，
    /// 如 `MetaCastOnCritPlayer`）。vendor key 是 PoB 显示名（PoBR 数据模型无显示名），
    /// 识别接线按本字段匹配 socket group 内宝石 / 主技能；空 = 尚未映射（多为
    /// PoE1 unique，PoE2 无对应物）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub match_effect_ids: Vec<String>,
    /// 真逻辑条目的 handler 稳定 id（`trigger:` 前缀；20 号 §5 计数监控 <100）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_id: Option<String>,
    /// 转写备注（受限谓词无法表达 / 有意省略的 vendor 细节，人工核对锚点）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// vendor 出处（`Modules/CalcTriggers.lua` 行段）。
    pub vendor_ref: String,
    /// 是否已对照 vendor 行为人工核验（W-E1 落库默认 false，核验后翻 true）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub verified: bool,
}

/// `overlay/trigger_configs.json` 顶层（消费侧忽略 `_meta`）。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TriggerConfigsDef {
    /// 配置列表，按 `key.name` 升序；条目数与 vendor configTable 对账（=61）。
    pub configs: Vec<TriggerConfigDef>,
}
