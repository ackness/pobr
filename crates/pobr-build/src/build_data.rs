//! [`BuildData`]：把 [`GameData`] 的按域查询结果收敛为 orchestrator 需要的内存索引。
//!
//! [`crate::calc_orchestrator::calculate_with_data`] 需要把 Build 里存的稳定 id
//! （天赋节点 `skill` id、宝石 id、职业名）解析为可计算的 modifier 来源。这些解析
//! 依赖游戏数据，而 I/O 收口在 [`pobr_gamedata::GameData`]。本模块在 **调用方已加载**
//! [`GameData`] 后，把所需域一次性投影为内存索引（节点表 / 宝石表 / 职业基础属性），
//! 供 orchestrator 零额外 I/O 地查询。
//!
//! 设计约束：本 crate 不持有文件 I/O，[`GameData`] 由调用方构造并传入；本模块只读
//! 其按域 loader，落地为确定性内存结构。

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use pobr_core::HighPrecisionRules;
use pobr_core::rules::stat_map_engine::StatMapCatalog;
use pobr_data::catalog::buffs::BuffDef;
use pobr_data::catalog::curse_priority::CursePriorityDef;
use pobr_data::catalog::jewel_radii::JewelRadiiDef;
use pobr_data::catalog::local_mods::LocalModsDef;
use pobr_data::catalog::{
    ArmourBaseStats, BaseItemDef, CostTypeDef, GemEffectDef, GrantedEffectDef, PassiveNodeDef,
    QualityStat, RuntimeConstants, SkillDamageStat, SkillGemDef, SkillLevelDef, SkillStatSetDef,
    StatSetDef, TriggerConfigDef, WeaponBaseStats,
};
use pobr_gamedata::ruleset::ConfigCatalog;
use pobr_gamedata::{GameData, LoadError};

/// 职业基础属性（PoE2 起始 str/dex/int），用于 [`pobr_core::CharacterBase`] 派生。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassBaseAttributes {
    pub strength: i32,
    pub dexterity: i32,
    pub intelligence: i32,
}

/// 某主动技能在某等级上解析出的计算相关参数（时间单位均为秒）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResolvedSkillLevel {
    /// 使用时间（秒）：攻击型取攻击时间，否则取施放时间。`None`=由武器/默认决定。
    pub use_time_s: Option<f64>,
    /// 冷却时间（秒）。`None`=无冷却。
    pub cooldown_s: Option<f64>,
    /// 法力消耗（资源 = `Mana`）。`None`=无法力消耗（可能为 Life/ES 等其他资源，见 `costs`）。
    pub mana_cost: Option<f64>,
    /// 该等级上已解析的技能**基础伤害 stat**（如 `spell_minimum_base_fire_damage` → 值）。
    /// 由计算侧映射为 `<Type>DamageMin/Max` BASE 词条注入。空=无 stat-set 伤害数据。
    pub base_damage: Vec<SkillDamageStat>,
    /// 全部资源消耗（按 `CostTypes` 解析的资源名 + 已除 divisor 的量）。
    /// 含 Mana/Life/ES/Rage/Ward 等及 per-second 持续消耗。空=无 CostTypes 数据或无消耗。
    pub costs: Vec<ResolvedCost>,
    /// 技能伤害倍率（PoB `baseMultiplier`；攻击技能武器+附加伤害的放大倍率）。`1.0`=无。
    pub damage_multiplier: f64,
    /// 攻击速度乘数（PoB `attackSpeedMultiplier`，百分点，可负）。作用于武器攻击速率
    /// `AttackRate × (1 + v/100)`（如 Flicker -50）。`None`=无（武器速率不变）。
    pub attack_speed_multiplier: Option<f64>,
    /// 技能基础暴击率（PoB `critChance`，百分点；如 Comet 13.0=13%）。法系技能的固有暴击源；
    /// 攻击技能若 `None` 由武器基底暴击决定。`None`=数据缺失（旧数据包或该技能无 critChance 行）。
    pub crit_chance: Option<f64>,
    /// statSet `baseMods` 固有**攻击速度 MORE**（PoB2 `mod("Speed","MORE",N,ModFlag.Attack)`，百分点；
    /// 如 Flicker Strike=285）。作为 `AttackSpeed` MORE 注入速度乘区（仅攻击技能消费）。`None`=无。
    pub skill_attack_speed_more: Option<f64>,
}

/// 某授予效果在某 (宝石等级, 品质) 上的可映射 stat——按来源分两段（契约 C1）：
/// `base` = stat-set 分等级行 + 等级无关常量；`quality` = 品质叠加段
/// （`trunc(per_quality_rate × quality)`，对齐 PoB2 CalcTools.lua:140-145
/// `buildSkillInstanceStats` 的品质前置叠加）。
///
/// 分段保留归因粒度（PoBR 增量资产，20-target §1.1）：quality 段经
/// `mapped_stat_modifiers` 注入时用 `SourceKind::GemQuality`（id 前缀
/// `gem.<效果 id>.q<Q>`）。不区分归因的消费点用 [`Self::all`] 合并遍历
/// （等价于 PoB2 把品质先加进同一 stats 表的数值语义；同 stat 的 BASE/INC
/// 在 mod_db 内加法合并，与先合后映一致）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffectStats {
    /// stat-set 分等级行 + 等级无关常量（既有 base 段）。
    pub base: Vec<SkillDamageStat>,
    /// 品质叠加段（quality = 0 或无品质表条目时为空）。
    pub quality: Vec<SkillDamageStat>,
}

impl EffectStats {
    /// base + quality 顺序串接的合并视图（不区分归因的取数点遍历用）。
    pub fn all(&self) -> impl Iterator<Item = &SkillDamageStat> {
        self.base.iter().chain(self.quality.iter())
    }
}

/// 某授予效果一个**未选 statSet** 的 stat 快照（W-J global-only merge 取数源，
/// 见 [`BuildData::unselected_set_stats`]）。
#[derive(Debug, Clone, PartialEq)]
pub struct UnselectedSetStats {
    /// statmap per-set 覆盖定位键 = vendor 1-based 导出序号的十进制字符串
    /// （[`pobr_data::catalog::stat_map::SkillStatMapDef::per_stat_set`] 键约定，
    /// 供 `stat_map_engine::map_stat_global_only` 的 `set_key` 直传）。
    pub set_key: String,
    /// statSet 稳定 id（归因 label / 调试用，如 `FlameWallProjectileBuffPlayer`）。
    pub set_id: String,
    /// 该 set 在 (gem_level, quality) 上的 stat 表——同 stat 已加法合并、按
    /// stat 字典序（vendor `buildSkillInstanceStats` 表语义，CalcTools.lua:138-200）。
    pub stats: Vec<SkillDamageStat>,
}

/// 一项已解析的技能资源消耗。
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedCost {
    /// 资源 id（`Mana` / `Life` / `ES` / `Rage` / `Ward` / `ManaPercent` / `ManaPerMinute` …）。
    pub resource: String,
    /// 消耗量（已除 `CostTypes.Divisor`：per-minute 资源 ÷60 得每秒量）。
    pub amount: f64,
    /// 是否为按时间持续消耗（per-second）。
    pub per_second: bool,
}

/// 从 [`GameData`] 投影出的、orchestrator 计算所需的内存索引。
///
/// 各域懒解析的产物在此**预先解析为内存结构**（调用方一次加载、多次复用）：
/// - `passive_nodes`：`skill id -> 节点定义`（供 [`pobr_tree::collect_allocated_mods`]）；
/// - `skill_gems`：`gem id -> 宝石定义`（供 active/support 分类）；
/// - `class_attributes`：`职业名 -> 基础属性`（供 CharacterBase 派生）。
#[derive(Debug, Clone)]
pub struct BuildData {
    /// 被动节点表，以 `skill` 数值 id 为键。
    pub passive_nodes: HashMap<u32, PassiveNodeDef>,
    /// 技能宝石表，以稳定 gem id 为键（如 `Metadata/Items/Gem/...`）。
    pub skill_gems: HashMap<String, SkillGemDef>,
    /// 职业基础属性表，以英文 canonical 职业名为键（如 `Ranger`）。
    pub class_attributes: HashMap<String, ClassBaseAttributes>,
    /// 授予效果表，以 `GrantedEffects.Id` 为键（如 `ExplosiveGrenadePlayer`）；
    /// 即 PoB `<Gem skillId>` 指向的目标，供主动技能 cast/cost 解析。
    pub granted_effects: HashMap<String, GrantedEffectDef>,
    /// 授予效果分等级参数表，以 `GrantedEffects.Id` 为键（升序等级数组）。
    pub granted_effect_levels: HashMap<String, Vec<SkillLevelDef>>,
    /// 授予效果分等级**伤害 stat 集**，以 `GrantedEffects.Id` 为键（每级已解析伤害 stat）。
    pub skill_stat_sets: HashMap<String, SkillStatSetDef>,
    /// 宝石品质 stat 斜率（`overlay/gem_quality_stats.json`），以 `GrantedEffects.Id`
    /// 为键。旧数据包无此 overlay 域时为空表（品质不产生 stat，向后兼容）。
    pub gem_quality_stats: HashMap<String, Vec<QualityStat>>,
    /// 宝石→授予效果连边（`overlay/gem_effects.json`，M1-T5.1），以**主效果 id**
    /// （`granted_effect_id`）为键——meta/复合宝石展开（T5.6）按 socket group 里
    /// 宝石的 `skill_id`（= 主效果 id）正向查附加效果。旧数据包无此 overlay 域时
    /// 为空表（无展开，向后兼容）。
    pub gem_effects: HashMap<String, GemEffectDef>,
    /// 消耗资源类型表（按 `CostTypes` 索引升序；为空表示旧数据包无此域）。
    pub cost_types: Vec<CostTypeDef>,
    /// 物品基底表，以英文 canonical 名称为键（供装备 `Item.base` 名称 → 武器/护甲基底数值）。
    pub base_items: HashMap<String, BaseItemDef>,
    /// 注入 calc 的运行时常量包（M0-W3 注入管道）：由 `GameData::load_ruleset()`
    /// 已数据化的域合并而成；未数据化/缺文件的域回退 `Default`（与 JSON 逐值相等）。
    /// `calculate_with_data` 经 `CalculationSession::set_constants` 注入 pobr-core。
    pub constants: RuntimeConstants,
    /// 范围珠宝环形档表（`base/jewel_radii.json`）：距离乘数 + 档位 label→inner/outer。
    /// 消费侧在本 crate 的树几何（`radius_jewel_grant_texts` → pobr-tree
    /// `compute_radius_jewel_effect_with_radii`），不经 `RuntimeConstants` 进 pobr-core。
    /// 数据缺失回退 `Default`（与 JSON 逐值相等）。
    pub jewel_radii: JewelRadiiDef,
    /// 局部词条白名单（`overlay/local_mods.json`，M0-W4d 数据化）。
    /// 数据包缺该 overlay 文件时为内建 fallback [`LocalModsDef::default`]
    /// （与 JSON 逐值一致的镜像，行为不变）。
    pub local_mods: LocalModsDef,
    /// statmap 数据目录（`overlay/skill_stat_map.json` → [`StatMapCatalog`]，
    /// M1-T2.4 切换后默认 [`crate::StatMapMode::Data`] 通道的数据源）。
    /// `calculate_with_data` 在编排选项未显式注入 catalog 时回退此处；
    /// 缺 overlay 文件（旧数据包）= `None`（数据通道全 miss，不注入任何映射）。
    pub stat_map_catalog: Option<Arc<StatMapCatalog>>,
    /// 内建 buff 定义表（`overlay/buff_definitions.json`，M3-T2 B3）。
    /// `calculate_with_data` 经 `CalculationSession::set_buff_definitions` 注入，
    /// env_finalize 阶段 6（doActorMisc 等价）消费——整段 `cfg.mode_combat`
    /// 门控（默认 false），注入本身零行为变化。缺 overlay 文件（旧数据包）
    /// = 空表（无内建 buff 展开，向后兼容）。
    pub buff_definitions: Vec<BuffDef>,
    /// curse 优先级数据表（`overlay/curse_priority.json`，M3-T3 C3）。
    /// `calculate_with_data` 经 `CalculationSession::set_curse_priority` 注入，
    /// env_finalize 阶段 4（buff_pass）的 curse priority/limit 消费——整段
    /// `cfg.mode_buffs` 门控（默认 false），注入本身零行为变化。缺 overlay
    /// 文件（旧数据包）= `None`（消费侧权重全 0 回退，R7 缺表容忍）。
    pub curse_priority: Option<CursePriorityDef>,
    /// config 选项目录（`overlay/config_options.json`，M3-T1 A5 主路径切换）。
    /// `calculate_with_data` 经 `crate::config_resolve::resolve_config` 消费——
    /// `Some` 走 `config_interpreter::interpret` 主路径；`None`（旧数据包 /
    /// [`BuildData::empty`]）回退旧 parse_config 产出（R7 缺表容忍）。
    pub config_catalog: Option<Arc<ConfigCatalog>>,
    /// 触发配置识别索引（M4-T5 W-E1）：`match_effect_ids` 的授予效果 id →
    /// `overlay/trigger_configs.json` 条目（vendor CalcTriggers.lua configTable
    /// 转写）。orchestrator 触发段按 socket group 内宝石 / 主技能 id 查此表识别
    /// gem-link/triggeredBy 关系。缺 overlay 文件（旧数据包）= 空表（识别面为空，
    /// 行为不变）；未映射 PoE2 id 的条目（多为 PoE1 unique）不入索引。
    pub trigger_configs: HashMap<String, TriggerConfigDef>,
    /// 取整精度规则（`overlay/high_precision_mods.json` → `RuleSet` →
    /// pobr-core [`HighPrecisionRules`]，M4-I 去重接线）。`calculate_with_data`
    /// 经 `CalculationSession::set_high_precision_rules` 注入，buff_pass /
    /// merge_flasks_charms 的 ScaleAddMod 缩放消费（T1 写原语同一份规则）。
    /// 缺 overlay 文件（旧数据包）= [`HighPrecisionRules::default`]
    /// （无例外表 fallback）。
    pub high_precision: HighPrecisionRules,
}

impl BuildData {
    /// 从一个已构造的 [`GameData`] 加载并投影 orchestrator 所需的全部域。
    ///
    /// 这是唯一会触发 [`GameData`] I/O 的入口；失败时返回 [`LoadError`]（缺文件 /
    /// 解析错误）。调用方应缓存返回值，避免对同一版本目录重复加载。
    pub fn load(data: &GameData) -> Result<Self, LoadError> {
        let passive_nodes = data
            .passive_nodes()?
            .into_iter()
            .map(|node| (node.skill, node))
            .collect();

        let skill_gems = data
            .skill_gems()?
            .into_iter()
            .map(|gem| (gem.id.clone(), gem))
            .collect();

        let class_attributes = data
            .passive_tree_meta()?
            .classes
            .into_iter()
            .map(|class| {
                (
                    class.name,
                    ClassBaseAttributes {
                        strength: class.base_str,
                        dexterity: class.base_dex,
                        intelligence: class.base_int,
                    },
                )
            })
            .collect();

        let granted_effects = data
            .granted_effects()?
            .into_iter()
            .map(|effect| (effect.id.clone(), effect))
            .collect();

        let granted_effect_levels = data.granted_effect_levels()?.into_iter().collect();

        let skill_stat_sets = data
            .skill_stat_sets()?
            .into_iter()
            .map(|set| (set.effect_id.clone(), set))
            .collect();

        // 品质 stat 斜率（overlay 域）：缺文件 = 空表（品质不产生 stat，向后兼容）。
        let gem_quality_stats = data
            .gem_quality_stats()?
            .map(|def| {
                def.effects
                    .into_iter()
                    .map(|e| (e.effect_id, e.stats))
                    .collect()
            })
            .unwrap_or_default();

        // 宝石→效果连边（overlay 域，T5.1）：按主效果 id 建索引（meta 展开正向查询键）。
        // 缺文件 = 空表（无展开，向后兼容）。
        let gem_effects = data
            .gem_effects()?
            .map(|def| {
                def.gems
                    .into_iter()
                    .map(|g| (g.granted_effect_id.clone(), g))
                    .collect()
            })
            .unwrap_or_default();

        let cost_types = data.cost_types()?;

        let base_items = data
            .base_items()?
            .into_iter()
            .map(|b| (b.name.clone(), b))
            .collect();

        // M0-W3：RuleSet 已数据化的域合并进常量包；None 域保持 Default fallback
        // （与 JSON 逐值相等，注入/回退两条路径输出一致）。
        let ruleset = data.load_ruleset()?;
        let mut constants = RuntimeConstants::default();
        if let Some(game_constants) = ruleset.game_constants {
            constants.game_constants = game_constants;
        }
        if let Some(character_constants) = ruleset.character_constants {
            constants.character_constants = character_constants;
        }
        if let Some(monster_scaling) = ruleset.monster_scaling {
            constants.monster_scaling = monster_scaling;
        }
        if let Some(enemy_presets) = ruleset.enemy_presets {
            constants.enemy_presets = enemy_presets;
        }
        if let Some(unarmed_data) = ruleset.unarmed_data {
            constants.unarmed_data = unarmed_data;
        }
        if let Some(weapon_types) = ruleset.weapon_types {
            constants.weapon_types = weapon_types;
        }
        // 范围珠宝档位表：数据化域 Some 则覆盖、None 回退 Default（与 JSON 逐值相等）。
        let jewel_radii = ruleset.jewel_radii.unwrap_or_default();
        // 取整精度例外表（M4-I 去重接线）：缺 overlay 文件 = Default（无例外表）。
        let high_precision = ruleset
            .high_precision_mods
            .map(HighPrecisionRules::from_def)
            .unwrap_or_default();

        // 局部词条白名单：缺 overlay 文件（旧数据包）时降级回内建 fallback
        // （与 JSON 逐值一致），其余加载/解析错误照常上抛，不静默。
        let local_mods = match data.local_mods() {
            Ok(def) => def,
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                LocalModsDef::default()
            }
            Err(e) => return Err(e),
        };

        // statmap 数据目录（M1-T2.4 切换：默认 Data 通道的数据源）。缺 overlay
        // 文件（旧数据包）= `None`（数据通道全 miss），其余加载/解析错误照常上抛。
        let stat_map_catalog = data
            .skill_stat_map()?
            .map(|def| Arc::new(StatMapCatalog::new(def)));

        // 内建 buff 定义（overlay 域，M3-T2）：缺文件 = 空表（无内建 buff 展开，
        // 向后兼容），其余加载/解析错误照常上抛。
        let buff_definitions = data
            .buff_definitions()?
            .map(|def| def.buffs)
            .unwrap_or_default();

        // curse 优先级表（overlay 域，M3-T3）：缺文件 = None（消费侧权重全 0 回退），
        // 其余加载/解析错误照常上抛。
        let curse_priority = data.curse_priority()?;

        // config 选项目录（overlay 域，M3-T1 A5）：缺文件 = `None`（消费方回退
        // 旧 parse_config 产出，R7 容忍），其余加载/解析错误照常上抛。
        let config_catalog = ruleset.config_catalog.map(Arc::new);

        // 触发配置识别索引（W-E1）：按 match_effect_ids 展开为 effect id → 条目。
        // 缺 overlay 文件（旧数据包）= 空表（识别面为空，行为不变）。
        let trigger_configs = data
            .trigger_configs()?
            .map(|def| {
                def.configs
                    .into_iter()
                    .flat_map(|config| {
                        config
                            .match_effect_ids
                            .clone()
                            .into_iter()
                            .map(move |id| (id, config.clone()))
                            .collect::<Vec<_>>()
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self {
            passive_nodes,
            skill_gems,
            class_attributes,
            granted_effects,
            granted_effect_levels,
            skill_stat_sets,
            gem_quality_stats,
            gem_effects,
            cost_types,
            base_items,
            constants,
            jewel_radii,
            local_mods,
            stat_map_catalog,
            buff_definitions,
            curse_priority,
            config_catalog,
            trigger_configs,
            high_precision,
        })
    }

    /// 构造一个空的 [`BuildData`]（无任何域数据；局部词条白名单取内建
    /// fallback——它是判定规则而非内容数据，空表会让武器局部剔除失效）。
    /// 用于测试或纯文本路径回退。
    pub fn empty() -> Self {
        Self {
            passive_nodes: HashMap::new(),
            skill_gems: HashMap::new(),
            class_attributes: HashMap::new(),
            granted_effects: HashMap::new(),
            granted_effect_levels: HashMap::new(),
            skill_stat_sets: HashMap::new(),
            gem_quality_stats: HashMap::new(),
            gem_effects: HashMap::new(),
            cost_types: Vec::new(),
            base_items: HashMap::new(),
            constants: RuntimeConstants::default(),
            jewel_radii: JewelRadiiDef::default(),
            local_mods: LocalModsDef::default(),
            stat_map_catalog: None,
            buff_definitions: Vec::new(),
            curse_priority: None,
            config_catalog: None,
            trigger_configs: HashMap::new(),
            high_precision: HighPrecisionRules::default(),
        }
    }

    /// 按基底名称查武器基底数值（`Item.base` → `WeaponBaseStats`）；非武器/未知返回 `None`。
    pub fn weapon_base(&self, base_name: &str) -> Option<&WeaponBaseStats> {
        self.base_items
            .get(base_name)
            .and_then(|b| b.weapon.as_ref())
    }

    /// 按基底名称查护甲基底数值（`Item.base` → `ArmourBaseStats`）；非护甲/未知返回 `None`。
    pub fn armour_base(&self, base_name: &str) -> Option<&ArmourBaseStats> {
        self.base_items
            .get(base_name)
            .and_then(|b| b.armour.as_ref())
    }

    /// 按 statSet 形态选择（`<Gem statSetIndex>`，1-based **vendor 导出序号**）取
    /// 选中 set；`None` / 序号未命中（vendor 未导出该序号、旧数据包无序号边车）
    /// 回退**主 set**——宁可缺省不可错选（蓝图 T5.5 保守口径；未选 set 的
    /// global-only merge 留 W-J）。
    fn select_stat_set(&self, skill_id: &str, set_index: Option<u32>) -> Option<&StatSetDef> {
        let def = self.skill_stat_sets.get(skill_id)?;
        match set_index {
            Some(n) => def
                .sets
                .iter()
                .find(|s| s.vendor_set_index == Some(n))
                .or_else(|| def.sets.first()),
            None => def.sets.first(),
        }
    }

    /// 选中 statSet 的 statmap **per-set 覆盖定位键**（vendor 1-based 导出序号的
    /// 十进制字符串，W-J 接线）：选中规则与 [`Self::select_stat_set`] 一致
    /// （`set_index` 命中 vendor 序号否则回退主 set）。无 stat-set 数据 / 选中 set
    /// 无 vendor 序号（未导出）→ `None`（调用方落引擎默认 set `"1"`，与 PoB2 缺省
    /// `statSetIndex=1` 等价，vendor `SkillsTab.lua:354`）。
    pub fn selected_set_key(&self, skill_id: &str, set_index: Option<u32>) -> Option<String> {
        self.select_stat_set(skill_id, set_index)
            .and_then(|s| s.vendor_set_index)
            .map(|i| i.to_string())
    }

    /// 选中 statSet 的 dotIs* 旗标（M4-T4 W-D1；statSet `baseMods` 直挂布尔，
    /// catalog [`pobr_data::catalog::DotFlags`]，经 skill_overrides overlay merge）。
    /// 选中规则与 [`Self::select_stat_set`] 一致；无数据时返回默认全 false
    /// （保守剥全部 dotCfg 位）。
    pub fn selected_set_dot_flags(
        &self,
        skill_id: &str,
        set_index: Option<u32>,
    ) -> pobr_data::catalog::DotFlags {
        self.select_stat_set(skill_id, set_index)
            .map(|s| s.dot_flags)
            .unwrap_or_default()
    }

    /// 解析某主动技能在某等级上的参数：cast/attack 时间（秒）、各资源消耗、冷却（秒）。
    /// statSet 形态取缺省主 set；带形态选择用 [`Self::resolve_skill_level_with_set`]。
    ///
    /// `skill_id` 为 `GrantedEffects.Id`（PoB `<Gem skillId>`）。返回 `None` 表示该
    /// 技能不在数据表中或为辅助效果（辅助效果不作为主动技能注入计算）。
    /// 等级越界时取最接近的已有等级行（数组按等级升序）。
    pub fn resolve_skill_level(
        &self,
        skill_id: &str,
        gem_level: u32,
    ) -> Option<ResolvedSkillLevel> {
        self.resolve_skill_level_with_set(skill_id, gem_level, None)
    }

    /// [`Self::resolve_skill_level`] 的 statSet 形态选择版（T5.5）：`set_index` =
    /// PoB `<Gem statSetIndex>`（1-based vendor 导出序号，`None`/未命中回退主 set）。
    /// 选中 set 决定技能 stat（`base_damage`）与伤害倍率（`damage_multiplier`）。
    pub fn resolve_skill_level_with_set(
        &self,
        skill_id: &str,
        gem_level: u32,
        set_index: Option<u32>,
    ) -> Option<ResolvedSkillLevel> {
        let effect = self.granted_effects.get(skill_id)?;
        if effect.is_support {
            return None;
        }
        let rows = self.granted_effect_levels.get(skill_id)?;
        if rows.is_empty() {
            return None;
        }
        // 取等级 ≤ gem_level 的最高行；都比 gem_level 高则取首行。
        let row = rows
            .iter()
            .rfind(|r| r.level <= gem_level)
            .unwrap_or(&rows[0]);

        // 使用时间：优先该等级的攻击时间，回退授予效果的施放时间（毫秒→秒）。
        let use_time_ms = row.attack_time_ms.or(effect.cast_time);
        let use_time_s = use_time_ms
            .filter(|&t| t > 0)
            .map(|t| f64::from(t) / 1000.0);
        let cooldown_s = row
            .cooldown_ms
            .filter(|&c| c > 0)
            .map(|c| f64::from(c) / 1000.0);

        // 消耗：按 effect.cost_types（资源类型索引）与 row.cost_amounts 位置配对，经
        // CostTypes 表解析为资源名 + 除 divisor（per-minute 资源 ÷60 得每秒量）。
        // 无 CostTypes 数据时回退「索引 0 = 法力」启发式（向后兼容）。
        let mut costs = Vec::new();
        for (i, &type_idx) in effect.cost_types.iter().enumerate() {
            let Some(&raw_amount) = row.cost_amounts.get(i) else {
                continue;
            };
            if raw_amount == 0 {
                continue;
            }
            match self.cost_types.get(type_idx as usize) {
                Some(def) if !def.id.is_empty() => costs.push(ResolvedCost {
                    resource: def.id.clone(),
                    amount: f64::from(raw_amount) / f64::from(def.divisor.max(1)),
                    per_second: def.per_minute,
                }),
                _ if type_idx == 0 => costs.push(ResolvedCost {
                    resource: "Mana".into(),
                    amount: f64::from(raw_amount),
                    per_second: false,
                }),
                _ => {}
            }
        }
        // 法力消耗（瞬时 `Mana` 资源）供 fill_skill_mechanics 的 SkillManaCostBase 读取。
        let mana_cost = costs
            .iter()
            .find(|c| c.resource == "Mana" && !c.per_second)
            .map(|c| c.amount);

        // 技能 stat（基础伤害值 + damage% 缩放）：选中 set 的分等级行 + 等级无关常量，
        // 供映射注入。品质段不在此处（主技能品质由 orchestrator 经 effect_stats 的
        // quality 段单独取数注入，保留 SourceKind::GemQuality 归因粒度），故 quality 传 0。
        let base_damage = self.effect_stats(skill_id, gem_level, 0, set_index).base;

        // 技能伤害倍率（PoB baseMultiplier）：优先**选中 statSet**（缺省主 set）行；
        // stat-set 缺失（如 Flicker 等 stat-set 为空的技能）时回退到
        // GrantedEffectsPerLevel 的 base_multiplier
        // （二者同义，PoB 在两表均存；grenade 的 stat-set 7.57 与 per-level 一致，不受影响）。
        let damage_multiplier = self
            .select_stat_set(skill_id, set_index)
            .and_then(|set| {
                set.levels
                    .iter()
                    .rfind(|l| l.gem_level <= gem_level)
                    .or(set.levels.first())
            })
            .map(|l| l.damage_multiplier)
            .or(row.base_multiplier)
            .unwrap_or(1.0);

        // statSet baseMods 固有攻击速度 MORE（PoB2 自带常量，如 Flicker 285）。等级无关；
        // overlay merge 写入主 set，选中附加 set 时无该值（None）。
        let skill_attack_speed_more = self
            .select_stat_set(skill_id, set_index)
            .and_then(|set| set.skill_attack_speed_more);

        Some(ResolvedSkillLevel {
            use_time_s,
            cooldown_s,
            mana_cost,
            base_damage,
            costs,
            damage_multiplier,
            attack_speed_multiplier: row.attack_speed_multiplier,
            crit_chance: row.crit_chance,
            skill_attack_speed_more,
        })
    }

    /// 取某授予效果在某 (宝石等级, 品质, statSet 形态) 上的全部可映射 stat
    /// （契约 C1 终版签名，T1→T5 演进收口）：`base` 段 = **选中 set** 的分等级行 +
    /// 等级无关常量；`quality` 段 = 品质表斜率 × 品质的**截断取整**叠加。
    ///
    /// `set_index` = PoB `<Gem statSetIndex>`（1-based vendor 导出序号，见
    /// [`pobr_data::catalog::StatSetDef::vendor_set_index`]）；`None` / 未命中
    /// 回退主 set。**未选 set 的 global-only merge 本签名不做**（PoB2
    /// `CalcActiveSkill.lua:124-140` 依赖 statmap mod 的 GlobalEffect tag）——
    /// 未选 set 的取数走 [`Self::unselected_set_stats`]（W-J），由调用方经
    /// `stat_map_engine::map_stat_global_only` 过滤注入。
    ///
    /// 品质语义对齐 PoB2 `CalcTools.lua:140-145`（`buildSkillInstanceStats`）：
    /// `stats[stat] += math.modf(rate × quality)`——`math.modf` 取整数部分即
    /// **trunc（toward zero）**，非 floor（负斜率时二者不同），Rust 侧用
    /// [`f64::trunc`] 严格对齐。品质为 0 / 无品质表条目时 `quality` 段为空。
    ///
    /// 对 active 与 **support** 效果同样适用（无 `is_support` 守卫）——support 宝石的
    /// 倍率 / 附加伤害 stat 经此取出，再由 statmap 数据引擎（`pobr-core::rules::stat_map_engine`）映射注入被支援技能
    /// （support 的品质表条目不存在——PoB2 导出即跳过，quality 段天然为空）。
    /// 等级越界取最接近的 ≤ 行；无 stat-set 数据时 base 段为空。
    pub fn effect_stats(
        &self,
        skill_id: &str,
        gem_level: u32,
        quality: u32,
        set_index: Option<u32>,
    ) -> EffectStats {
        let base = self
            .select_stat_set(skill_id, set_index)
            .map(|set| {
                let mut stats = set
                    .levels
                    .iter()
                    .rfind(|l| l.gem_level <= gem_level)
                    .or(set.levels.first())
                    .map(|level| level.stats.clone())
                    .unwrap_or_default();
                stats.extend(set.constant_stats.iter().cloned());
                stats
            })
            .unwrap_or_default();

        let quality_stats = if quality > 0 {
            self.gem_quality_stats
                .get(skill_id)
                .map(|rows| {
                    rows.iter()
                        .map(|q| SkillDamageStat {
                            stat: q.stat.clone(),
                            // trunc（toward zero），对齐 math.modf 整数部分。
                            value: (q.per_quality_rate * f64::from(quality)).trunc(),
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        EffectStats {
            base,
            quality: quality_stats,
        }
    }

    /// 取某授予效果在某 (宝石等级, 品质, statSet 形态) 下**未选 set** 的 stat 快照
    /// （W-J global-only merge 的取数源，PoB2 `CalcActiveSkill.lua:124-140`：
    /// 选中 set 之外的每个 statSet 以 `onlyGlobals=true` 参与 merge）。
    ///
    /// - 选中 set 判定与 [`Self::effect_stats`] 同规则（[`Self::select_stat_set`]：
    ///   `set_index` 命中 vendor 序号否则回退主 set），返回其余 set；
    /// - vendor 未导出的 set（`vendor_set_index = None`，模板策展跳过，如
    ///   IceNovaPlayerOnFrostbolt）不在 PoB2 `grantedEffect.statSets` 列表，
    ///   **不参与** global merge，此处同样剔除；
    /// - 每个 set 的 stats 按 vendor `buildSkillInstanceStats`（CalcTools.lua:138-200）
    ///   的**表语义**构造：品质段（trunc，品质表按 effect 共享、逐 set 都先叠加）+
    ///   分等级行（≤ gem_level 最高行，越界取首行）+ 等级无关常量，**同 stat 加法
    ///   合并**、键按字典序（确定性）。与 [`Self::effect_stats`] 的分段串接不同——
    ///   statmap 条目可能带 `value` 覆盖（非线性），全局-only 查表必须以合并后的
    ///   单值为输入才与 vendor 逐 stat 单次 merge 一致。
    ///
    /// 无多 set / 效果未知时返回空。注：未选 set 的 `baseMods`（PoBR 蒸馏字段
    /// `skill_attack_speed_more`，无 GlobalEffect tag）按 vendor `:131-135`
    /// 只收 global baseMod 的语义**恒不注入**，故本快照不含该字段。
    pub fn unselected_set_stats(
        &self,
        skill_id: &str,
        gem_level: u32,
        quality: u32,
        set_index: Option<u32>,
    ) -> Vec<UnselectedSetStats> {
        let Some(def) = self.skill_stat_sets.get(skill_id) else {
            return Vec::new();
        };
        let selected_id = self
            .select_stat_set(skill_id, set_index)
            .map(|s| s.set_id.as_str());
        let mut out = Vec::new();
        for set in &def.sets {
            // vendor 未导出（无序号）不参与；选中 set 本身跳过。
            let Some(idx) = set.vendor_set_index else {
                continue;
            };
            if Some(set.set_id.as_str()) == selected_id {
                continue;
            }
            // buildSkillInstanceStats 表语义：同 stat 加法合并（BTreeMap 字典序确定性）。
            let mut acc: BTreeMap<String, f64> = BTreeMap::new();
            if quality > 0
                && let Some(rows) = self.gem_quality_stats.get(skill_id)
            {
                for q in rows {
                    // trunc（toward zero），与 effect_stats 的品质段同语义。
                    *acc.entry(q.stat.clone()).or_default() +=
                        (q.per_quality_rate * f64::from(quality)).trunc();
                }
            }
            if let Some(level) = set
                .levels
                .iter()
                .rfind(|l| l.gem_level <= gem_level)
                .or(set.levels.first())
            {
                for s in &level.stats {
                    *acc.entry(s.stat.clone()).or_default() += s.value;
                }
            }
            for s in &set.constant_stats {
                *acc.entry(s.stat.clone()).or_default() += s.value;
            }
            out.push(UnselectedSetStats {
                set_key: idx.to_string(),
                set_id: set.set_id.clone(),
                stats: acc
                    .into_iter()
                    .map(|(stat, value)| SkillDamageStat { stat, value })
                    .collect(),
            });
        }
        out
    }

    /// 查询某职业的基础属性（按英文 canonical 名）；未知职业返回 `None`。
    pub fn class_attributes(&self, class_name: &str) -> Option<ClassBaseAttributes> {
        self.class_attributes.get(class_name).copied()
    }

    /// 判断某宝石 id 是否辅助宝石；未知宝石返回 `None`（调用方按需回退）。
    pub fn is_support_gem(&self, gem_id: &str) -> Option<bool> {
        self.skill_gems.get(gem_id).map(|gem| gem.is_support)
    }

    /// 判断某授予效果是否为**光环**（`skill_types` 含 `Aura`）。光环对自身（及在场盟友）
    /// 施加持续 buff——其分等级 stat 走 [`Self::effect_stats`] 取值后由防御侧注入。
    /// 未知效果返回 `false`（保守，不臆造光环语义）。诅咒（作用于敌人）`skill_types` 不含
    /// `Aura`，故不会被误判为自身 buff。
    pub fn is_aura(&self, skill_id: &str) -> bool {
        self.granted_effects
            .get(skill_id)
            .map(|e| e.skill_types.iter().any(|t| t == "Aura"))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pobr_gamedata::repo_data_root;

    /// 仓库内置数据目录下的版本目录（与 orchestrator 测试共用）。
    pub(crate) fn repo_version_dir() -> std::path::PathBuf {
        repo_data_root().join("4.5.0.3.4")
    }

    #[test]
    fn loads_repo_data_domains() {
        let data = GameData::new(repo_version_dir());
        let bd = BuildData::load(&data).expect("load repo data");
        assert!(!bd.passive_nodes.is_empty(), "passive nodes loaded");
        assert!(!bd.skill_gems.is_empty(), "skill gems loaded");
        assert!(!bd.class_attributes.is_empty(), "class attrs loaded");
    }

    #[test]
    fn resolves_known_class_attributes() {
        let data = GameData::new(repo_version_dir());
        let bd = BuildData::load(&data).expect("load");
        let ranger = bd.class_attributes("Ranger").expect("Ranger present");
        // Ranger 起始：7 str / 15 dex / 7 int（passive_tree_meta）。
        assert_eq!(ranger.dexterity, 15);
        assert!(bd.class_attributes("NoSuchClass").is_none());
    }

    /// T5.5（契约 C1 终版）：statSet 形态选择——`set_index` 按 vendor 导出序号
    /// 选中 set；None / 未命中回退主 set（保守口径）。
    #[test]
    fn effect_stats_selects_stat_set_by_vendor_index() {
        use pobr_data::catalog::{SkillDamageStat, SkillStatSetLevel, StatSetDef};
        fn set(set_id: &str, vendor_idx: Option<u32>, stat: &str, value: f64) -> StatSetDef {
            StatSetDef {
                set_id: set_id.into(),
                label: None,
                vendor_set_index: vendor_idx,
                base_effectiveness: 0.0,
                constant_stats: Vec::new(),
                skill_attack_speed_more: None,
                dot_flags: Default::default(),
                levels: vec![SkillStatSetLevel {
                    gem_level: 1,
                    damage_multiplier: 1.0,
                    stats: vec![SkillDamageStat {
                        stat: stat.into(),
                        value,
                    }],
                }],
            }
        }
        let mut skill_stat_sets = HashMap::new();
        skill_stat_sets.insert(
            "Synth".to_string(),
            pobr_data::catalog::SkillStatSetDef {
                effect_id: "Synth".into(),
                sets: vec![
                    set("SynthMain", Some(1), "spell_minimum_base_fire_damage", 10.0),
                    set("SynthAlt", Some(2), "spell_minimum_base_cold_damage", 99.0),
                ],
            },
        );
        let bd = BuildData {
            skill_stat_sets,
            ..BuildData::empty()
        };
        // 缺省（None）→ 主 set。
        let main = bd.effect_stats("Synth", 1, 0, None);
        assert_eq!(main.base[0].stat, "spell_minimum_base_fire_damage");
        // statSetIndex=2 → vendor 序号 2 的附加 set（XML 解析 → 消费 round-trip）。
        let alt = bd.effect_stats("Synth", 1, 0, Some(2));
        assert_eq!(alt.base[0].stat, "spell_minimum_base_cold_damage");
        assert_eq!(alt.base[0].value, 99.0);
        // 未命中序号（vendor 未导出 / 越界）→ 回退主 set，不错选。
        let miss = bd.effect_stats("Synth", 1, 0, Some(7));
        assert_eq!(miss.base[0].stat, "spell_minimum_base_fire_damage");
        // resolve 链同口径：Synth 不在 granted_effects 表（resolve 返回 None），
        // select 路径已由 effect_stats 覆盖；此处仅锚定签名可用。
        assert!(
            bd.resolve_skill_level_with_set("Synth", 1, Some(2))
                .is_none()
        );
    }

    /// W-J：未选 set 快照——选中判定与 effect_stats 同规则；vendor 未导出 set 剔除；
    /// 品质段逐 set 叠加（trunc）；同 stat 加法合并、字典序确定性。
    #[test]
    fn unselected_set_stats_snapshot_semantics() {
        use pobr_data::catalog::{QualityStat, SkillDamageStat, SkillStatSetLevel, StatSetDef};
        fn set(
            set_id: &str,
            vendor_idx: Option<u32>,
            level_stats: Vec<(&str, f64)>,
            constant_stats: Vec<(&str, f64)>,
        ) -> StatSetDef {
            let ds = |v: Vec<(&str, f64)>| {
                v.into_iter()
                    .map(|(stat, value)| SkillDamageStat {
                        stat: stat.into(),
                        value,
                    })
                    .collect::<Vec<_>>()
            };
            StatSetDef {
                set_id: set_id.into(),
                label: None,
                vendor_set_index: vendor_idx,
                base_effectiveness: 0.0,
                constant_stats: ds(constant_stats),
                skill_attack_speed_more: None,
                dot_flags: Default::default(),
                levels: vec![SkillStatSetLevel {
                    gem_level: 1,
                    damage_multiplier: 1.0,
                    stats: ds(level_stats),
                }],
            }
        }
        let mut bd = BuildData::empty();
        bd.skill_stat_sets.insert(
            "Synth".to_string(),
            pobr_data::catalog::SkillStatSetDef {
                effect_id: "Synth".into(),
                sets: vec![
                    set("SynthMain", Some(1), vec![("alpha", 10.0)], vec![]),
                    // 分等级行与常量同 stat（beta）→ 加法合并为 5+2=7。
                    set(
                        "SynthAlt",
                        Some(2),
                        vec![("beta", 5.0)],
                        vec![("beta", 2.0), ("gamma", 1.0)],
                    ),
                    // vendor 未导出（模板策展跳过）→ 不参与 global merge。
                    set("SynthHidden", None, vec![("hidden", 99.0)], vec![]),
                ],
            },
        );
        // 品质表按 effect 共享：未选 set 同样先叠加（trunc(0.55×19)=10）。
        bd.gem_quality_stats.insert(
            "Synth".into(),
            vec![QualityStat {
                stat: "beta".into(),
                per_quality_rate: 0.55,
            }],
        );

        // 缺省（None）→ 选中主 set，未选 = 仅 SynthAlt（Hidden 剔除）。
        let unsel = bd.unselected_set_stats("Synth", 1, 19, None);
        assert_eq!(unsel.len(), 1);
        assert_eq!(unsel[0].set_key, "2");
        assert_eq!(unsel[0].set_id, "SynthAlt");
        // beta = 品质 10 + 分等级 5 + 常量 2 = 17；gamma = 1；字典序。
        assert_eq!(
            unsel[0]
                .stats
                .iter()
                .map(|s| (s.stat.as_str(), s.value))
                .collect::<Vec<_>>(),
            vec![("beta", 17.0), ("gamma", 1.0)]
        );

        // 显式选中 set 2 → 未选 = 主 set（品质同样叠加进其快照）。
        let unsel = bd.unselected_set_stats("Synth", 1, 0, Some(2));
        assert_eq!(unsel.len(), 1);
        assert_eq!(unsel[0].set_key, "1");
        assert_eq!(unsel[0].set_id, "SynthMain");
        assert_eq!(
            unsel[0]
                .stats
                .iter()
                .map(|s| (s.stat.as_str(), s.value))
                .collect::<Vec<_>>(),
            vec![("alpha", 10.0)]
        );

        // 序号未命中（越界）→ 选中回退主 set，未选与缺省一致；未知效果 → 空。
        assert_eq!(
            bd.unselected_set_stats("Synth", 1, 19, Some(7)),
            bd.unselected_set_stats("Synth", 1, 19, None)
        );
        assert!(bd.unselected_set_stats("NoSuch", 1, 20, None).is_empty());
    }

    /// M1-T5.1：宝石→效果连边经 overlay/gem_effects.json merge 进 SkillGemDef，
    /// 并按主效果 id 建 meta 展开索引（gem_effects）。
    #[test]
    fn gem_effect_links_loaded_from_overlay() {
        let data = GameData::new(repo_version_dir());
        let bd = BuildData::load(&data).expect("load");
        let ice = bd
            .skill_gems
            .get("Metadata/Items/Gems/SkillGemIceNova")
            .expect("IceNova gem present");
        assert_eq!(ice.granted_effect_id.as_deref(), Some("IceNovaPlayer"));
        // meta 展开索引按主效果 id 可查（GemSkillRef.skill_id = 主效果 id）。
        assert!(bd.gem_effects.contains_key("IceNovaPlayer"));
        // 附加授予效果外键（18-G5）：Blasphemy 宝石主效果 BlasphemyPlayer 附带
        // SupportBlasphemyPlayer（vendor Gems.lua additionalGrantedEffectId1）。
        let blasphemy = bd.gem_effects.get("BlasphemyPlayer").expect("Blasphemy");
        assert_eq!(
            blasphemy.additional_granted_effect_ids,
            ["SupportBlasphemyPlayer"]
        );
    }

    #[test]
    fn classifies_support_gem() {
        let data = GameData::new(repo_version_dir());
        let bd = BuildData::load(&data).expect("load");
        // 任取一颗已知辅助宝石与一颗主动宝石做分类断言。
        let any_support = bd.skill_gems.values().find(|g| g.is_support);
        let any_active = bd.skill_gems.values().find(|g| !g.is_support);
        if let Some(g) = any_support {
            assert_eq!(bd.is_support_gem(&g.id), Some(true));
        }
        if let Some(g) = any_active {
            assert_eq!(bd.is_support_gem(&g.id), Some(false));
        }
        assert_eq!(bd.is_support_gem("Metadata/Items/Gem/DoesNotExist"), None);
    }
}
