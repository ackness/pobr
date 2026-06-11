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

use std::collections::HashMap;

use pobr_data::catalog::jewel_radii::JewelRadiiDef;
use pobr_data::catalog::local_mods::LocalModsDef;
use pobr_data::catalog::{
    ArmourBaseStats, BaseItemDef, CostTypeDef, GemEffectDef, GrantedEffectDef, PassiveNodeDef,
    QualityStat, RuntimeConstants, SkillDamageStat, SkillGemDef, SkillLevelDef, SkillStatSetDef,
    WeaponBaseStats,
};
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

    /// 解析某主动技能在某等级上的参数：cast/attack 时间（秒）、各资源消耗、冷却（秒）。
    ///
    /// `skill_id` 为 `GrantedEffects.Id`（PoB `<Gem skillId>`）。返回 `None` 表示该
    /// 技能不在数据表中或为辅助效果（辅助效果不作为主动技能注入计算）。
    /// 等级越界时取最接近的已有等级行（数组按等级升序）。
    pub fn resolve_skill_level(
        &self,
        skill_id: &str,
        gem_level: u32,
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

        // 技能 stat（基础伤害值 + damage% 缩放）：分等级行 + 等级无关常量，供映射注入。
        // 品质段不在此处（主技能品质由 orchestrator 经 effect_stats 的 quality 段
        // 单独取数注入，保留 SourceKind::GemQuality 归因粒度），故 quality 传 0。
        let base_damage = self.effect_stats(skill_id, gem_level, 0).base;

        // 技能伤害倍率（PoB baseMultiplier）：优先**主 statSet** 行（T5.2 多 set 下
        // 缺省主 set，与单 set 时代一致）；stat-set 缺失（如 Flicker 等 stat-set 为空
        // 的技能）时回退到 GrantedEffectsPerLevel 的 base_multiplier
        // （二者同义，PoB 在两表均存；grenade 的 stat-set 7.57 与 per-level 一致，不受影响）。
        let damage_multiplier = self
            .skill_stat_sets
            .get(skill_id)
            .and_then(|def| def.sets.first())
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
        // overlay merge 写入主 set。
        let skill_attack_speed_more = self
            .skill_stat_sets
            .get(skill_id)
            .and_then(|def| def.sets.first())
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

    /// 取某授予效果在某 (宝石等级, 品质) 上的全部可映射 stat（契约 C1，T1 演进）：
    /// `base` 段 = stat-set 分等级行 + 等级无关常量；`quality` 段 = 品质表斜率 ×
    /// 品质的**截断取整**叠加。
    ///
    /// 品质语义对齐 PoB2 `CalcTools.lua:140-145`（`buildSkillInstanceStats`）：
    /// `stats[stat] += math.modf(rate × quality)`——`math.modf` 取整数部分即
    /// **trunc（toward zero）**，非 floor（负斜率时二者不同），Rust 侧用
    /// [`f64::trunc`] 严格对齐。品质为 0 / 无品质表条目时 `quality` 段为空。
    ///
    /// 对 active 与 **support** 效果同样适用（无 `is_support` 守卫）——support 宝石的
    /// 倍率 / 附加伤害 stat 经此取出，再由 [`crate::skill_stat_map`] 映射注入被支援技能
    /// （support 的品质表条目不存在——PoB2 导出即跳过，quality 段天然为空）。
    /// 等级越界取最接近的 ≤ 行；无 stat-set 数据时 base 段为空。
    pub fn effect_stats(&self, skill_id: &str, gem_level: u32, quality: u32) -> EffectStats {
        // T5.2 多 set 模型下缺省取**主 set**（sets[0]），与单 set 时代一致；
        // 形态选择（`<Gem statSetIndex>`）由 T5.4/T5.5 接入。
        let base = self
            .skill_stat_sets
            .get(skill_id)
            .and_then(|def| def.sets.first())
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
