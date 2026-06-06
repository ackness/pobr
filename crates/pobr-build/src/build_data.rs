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

use pobr_data::catalog::{
    ArmourBaseStats, BaseItemDef, CostTypeDef, GrantedEffectDef, PassiveNodeDef, SkillDamageStat,
    SkillGemDef, SkillLevelDef, SkillStatSetDef, WeaponBaseStats,
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
    /// 消耗资源类型表（按 `CostTypes` 索引升序；为空表示旧数据包无此域）。
    pub cost_types: Vec<CostTypeDef>,
    /// 物品基底表，以英文 canonical 名称为键（供装备 `Item.base` 名称 → 武器/护甲基底数值）。
    pub base_items: HashMap<String, BaseItemDef>,
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
            .map(|set| (set.id.clone(), set))
            .collect();

        let cost_types = data.cost_types()?;

        let base_items = data
            .base_items()?
            .into_iter()
            .map(|b| (b.name.clone(), b))
            .collect();

        Ok(Self {
            passive_nodes,
            skill_gems,
            class_attributes,
            granted_effects,
            granted_effect_levels,
            skill_stat_sets,
            cost_types,
            base_items,
        })
    }

    /// 构造一个空的 [`BuildData`]（无任何域数据）。用于测试或纯文本路径回退。
    pub fn empty() -> Self {
        Self {
            passive_nodes: HashMap::new(),
            skill_gems: HashMap::new(),
            class_attributes: HashMap::new(),
            granted_effects: HashMap::new(),
            granted_effect_levels: HashMap::new(),
            skill_stat_sets: HashMap::new(),
            cost_types: Vec::new(),
            base_items: HashMap::new(),
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
        let base_damage = self.effect_stats(skill_id, gem_level);

        Some(ResolvedSkillLevel {
            use_time_s,
            cooldown_s,
            mana_cost,
            base_damage,
            costs,
        })
    }

    /// 取某授予效果在某宝石等级上的全部可映射 stat（stat-set 的分等级行 + 等级无关常量）。
    ///
    /// 对 active 与 **support** 效果同样适用（无 `is_support` 守卫）——support 宝石的
    /// 倍率 / 附加伤害 stat 经此取出，再由 [`crate::skill_stat_map`] 映射注入被支援技能。
    /// 等级越界取最接近的 ≤ 行；无 stat-set 数据返回空。
    pub fn effect_stats(&self, skill_id: &str, gem_level: u32) -> Vec<SkillDamageStat> {
        let Some(set) = self.skill_stat_sets.get(skill_id) else {
            return Vec::new();
        };
        let mut stats = set
            .levels
            .iter()
            .rfind(|l| l.gem_level <= gem_level)
            .or(set.levels.first())
            .map(|level| level.stats.clone())
            .unwrap_or_default();
        stats.extend(set.constant_stats.iter().cloned());
        stats
    }

    /// 查询某职业的基础属性（按英文 canonical 名）；未知职业返回 `None`。
    pub fn class_attributes(&self, class_name: &str) -> Option<ClassBaseAttributes> {
        self.class_attributes.get(class_name).copied()
    }

    /// 判断某宝石 id 是否辅助宝石；未知宝石返回 `None`（调用方按需回退）。
    pub fn is_support_gem(&self, gem_id: &str) -> Option<bool> {
        self.skill_gems.get(gem_id).map(|gem| gem.is_support)
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
