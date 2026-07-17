//! `Build`：PoB Build 的内存状态。
//!
//! 聚合一个 build 的全部可计算输入：角色基础（等级 / 职业 / 升华）、天赋树
//! ([`PassiveTreeSpec`])、装备（按 [`EquipmentSlot`] 的 [`Item`]）、技能宝石组
//! （[`SocketGroup`]）、Build 级配置（[`BuildConfig`]）以及当前视图（[`ViewMode`]）。
//!
//! 类型一律采用 REAL 权威定义（`pobr_data` 的 `PassiveTreeSpec` / `Item` /
//! `ViewMode`）。`SocketGroup` 是本 crate 的简化等价物（不依赖
//! SANDBOX 专有类型），承载「主动技能 + 辅助宝石」的稳定 id 与启用状态。

use std::collections::HashMap;

use pobr_data::build_config::ViewMode;
use pobr_data::item::{EquipmentSlot, Item};
use pobr_data::passive_tree::{NodeId, PassiveTreeSpec};

use crate::build_config::BuildConfig;

/// 一个宝石的授予效果引用（`<Gem skillId>` + `<Gem level>` + `<Gem quality>` +
/// `<Gem statSetIndex>`），active/support 皆可。由计算侧按数据表（`is_support`）分类。
///
/// 契约 C4（M1 蓝图 §3.3）收口：`quality` 为 T1 份额（default 0 = 无品质），
/// `stat_set_index` 为 T5 份额（形态选择）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GemSkillRef {
    /// 授予效果 id（PoB `<Gem skillId>`，如 `SupportAddedLightningDamagePlayer`）。
    pub skill_id: String,
    /// 宝石等级（PoB `<Gem level>`）。
    pub gem_level: u32,
    /// 宝石品质（PoB `<Gem quality>`，0–23；0 = 无品质）。消费侧按
    /// `trunc(per_quality_rate × quality)` 叠加品质 stat（见
    /// `BuildData::effect_stats`，对齐 PoB2 CalcTools.lua `buildSkillInstanceStats`）。
    pub quality: u32,
    /// statSet 形态选择（PoB `<Gem statSetIndex>`，**1-based**，索引 PoB2 导出的
    /// statSets 列表 = `StatSetDef::vendor_set_index` 语义；vendor SkillsTab.lua:354
    /// 读 / :489 写）。`None` = 未指定（缺省主 set；PoB2 序列化缺省态为字面量
    /// `"nil"`，解析归一化为 `None`）。`statSetIndexCalcs`（calcs 页独立选择）
    /// M1 不做，解析忽略。
    pub stat_set_index: Option<u32>,
    /// PoB `<Gem nameSpec>` 显示名——仅当 XML 缺 `skillId`/`gemId`（lineage
    /// support 如 Atziri's Communion 的序列化形态）时携带，`skill_id` 此时为空串。
    /// 编排层 `stage_build_view` 按显示名归一匹配 granted_effects 回填
    /// `skill_id`（PoB2 SkillsTab 按 nameSpec 反查 gem 的等价物）；未解析成功
    /// 的引用在全部消费点因 `granted_effects.get("")` 落空而惰性跳过。
    pub name_spec: Option<String>,
}

/// 一组同插槽的技能宝石（主动技能 + 其辅助）。简化等价物：用稳定 gem id 表示。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SocketGroup {
    /// PoB `<Skill source>`（装备授予技能为 `Item:<id>:<name>`）。`None` 表示
    /// 用户手动创建的普通技能组；编排层据此区分同槽同技能的手动组与授予组。
    pub source: Option<String>,
    /// 装在哪个槽位（如 `"weapon1"`）。`None` 表示未指定 / 灵魂珠等无槽来源。
    pub slot: Option<String>,
    /// 该组是否参与计算（PoB 里每组可单独启停）。
    pub enabled: bool,
    /// 该组的宝石稳定 id（主动技能在前，辅助宝石在后；与 PoB Gem 列表顺序一致）。
    pub gem_ids: Vec<String>,
    /// 主动技能的授予效果 id（PoB `<Gem skillId>`，如 `ExplosiveGrenadePlayer`），
    /// 用于解析该技能的分等级参数（cast/attack time、cost、cooldown）。`None`=未知。
    pub active_skill_id: Option<String>,
    /// 主动技能宝石的等级（PoB `<Gem level>`），用于在分等级数组中定位行。
    pub active_gem_level: Option<u32>,
    /// 主动技能宝石的品质（PoB `<Gem quality>`），与 [`GemSkillRef::quality`] 同步
    /// （T1.4：取数侧实际经 `gem_skills` 查品质，本字段供 builder/快照口径对齐）。
    pub active_gem_quality: Option<u32>,
    /// 该组**每个启用宝石**的授予效果引用（按 PoB Gem 列表顺序，含 active 与 support）；
    /// 供解析 support 宝石的分等级 stat（倍率/附加伤害）注入被支援技能。
    pub gem_skills: Vec<GemSkillRef>,
    /// PoB `<Skill mainActiveSkill="N">`（**1-based**）：指向该组**非辅助技能列表**中的第 N 个
    /// （meta/触发壳算入，support 不计入）。用于多主动技能组（如 Cast on Crit + Comet）里
    /// 选中正确的主技能；`None`=未指定（退化为该组首个伤害技能）。
    pub main_active_skill: Option<usize>,
}

impl SocketGroup {
    pub fn new() -> Self {
        Self {
            source: None,
            slot: None,
            enabled: true,
            gem_ids: Vec::new(),
            active_skill_id: None,
            active_gem_level: None,
            active_gem_quality: None,
            gem_skills: Vec::new(),
            main_active_skill: None,
        }
    }

    pub fn with_slot(mut self, slot: impl Into<String>) -> Self {
        self.slot = Some(slot.into());
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_gem(mut self, gem_id: impl Into<String>) -> Self {
        self.gem_ids.push(gem_id.into());
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// 设定主动技能授予效果 id + 宝石等级（分等级参数解析的键）。
    pub fn with_active_skill(mut self, skill_id: impl Into<String>, gem_level: u32) -> Self {
        self.active_skill_id = Some(skill_id.into());
        self.active_gem_level = Some(gem_level);
        self
    }

    /// 追加一个宝石的授予效果引用（active 或 support；按 PoB Gem 列表顺序）。
    /// 品质缺省 0（无品质）；带品质用 [`Self::with_gem_skill_quality`]。
    pub fn with_gem_skill(self, skill_id: impl Into<String>, gem_level: u32) -> Self {
        self.with_gem_skill_quality(skill_id, gem_level, 0)
    }

    /// 追加一个带品质的宝石授予效果引用（T1.4，契约 C4）。
    pub fn with_gem_skill_quality(
        mut self,
        skill_id: impl Into<String>,
        gem_level: u32,
        quality: u32,
    ) -> Self {
        self.gem_skills.push(GemSkillRef {
            skill_id: skill_id.into(),
            gem_level,
            quality,
            stat_set_index: None,
            name_spec: None,
        });
        self
    }

    /// 追加一个带 statSet 形态选择的宝石授予效果引用（T5.4，契约 C4）。
    pub fn with_gem_skill_stat_set(
        mut self,
        skill_id: impl Into<String>,
        gem_level: u32,
        stat_set_index: u32,
    ) -> Self {
        self.gem_skills.push(GemSkillRef {
            skill_id: skill_id.into(),
            gem_level,
            quality: 0,
            stat_set_index: Some(stat_set_index),
            name_spec: None,
        });
        self
    }

    /// 设定 PoB `mainActiveSkill`（1-based，索引该组非辅助技能列表）。
    pub fn with_main_active_skill(mut self, n: usize) -> Self {
        self.main_active_skill = Some(n);
        self
    }
}

/// 角色身份（PoB Build XML 的 `<Build>` 头部字段子集）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterIdentity {
    pub level: u32,
    pub class_name: String,
    pub ascendancy_name: String,
}

/// 范围珠宝（radius jewel）的几何展开输入。
///
/// PoB2 `... Passive Skills in Radius also grant <mod>` 词条需要按珠宝插槽**半径内已分配**
/// 的对应种类节点数 × 授予值注入全局 mod。本结构携带几何计算所需的最小信息：插槽所在
/// 树节点 id、半径档位文本（`Radius:` 行）、以及 `also grant` 行（原文，后续逐行展开）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RadiusJewel {
    /// 珠宝所在的树插槽节点 `skill` id（`<Socket nodeId>`）。
    pub socket_node: u32,
    /// 半径档位文本（`Radius:` 行原文，如 `Small`/`Medium`/`Large`/`Very Large`）。
    /// 缺失时为 `None`（PoB 默认珠宝半径，按 Large 近似）。
    pub radius_label: Option<String>,
    /// `... Passive Skills in Radius also grant <mod>` 行（原文，含种类前缀）。
    pub grant_lines: Vec<String>,
    /// `N% increased Effect of Notable Passive Skills in Radius`（Time-Lost 珠宝；
    /// vendor ModParser.lua:6847 → `JewelNotablePassiveSkillEffect` INC，消费点
    /// CalcSetup.lua:246-275 对半径内 Notable 节点 modList 整体 ScaleAddList）。
    /// 同珠宝多行时取末行（vendor `localNotableIncEffect = mod.value` 后写覆盖）。
    pub notable_effect_inc: u32,
}

/// PoB Build 的内存状态。
///
/// 不可变更新：所有 `with_*` 方法返回新副本；`set_item` / `add_socket_group` 等
/// 也保持「构造新值」语义（沿用 builder 风格，避免就地共享可变状态）。
#[derive(Debug, Clone, Default)]
pub struct Build {
    /// 角色身份。
    pub character: CharacterIdentity,
    /// 当前视图（PoB UI 状态，兼容 Build XML `viewMode`）。
    pub view_mode: ViewMode,
    /// 已分配天赋树。
    pub tree: PassiveTreeSpec,
    /// 该 build 记录的 PoB 天赋树版本（`<Spec treeVersion>`，如 `"0_5"`）。`None`=旧存档
    /// 未标注。**纯对账元信息**：calc 仍按已加载数据的树解释已分配节点（对齐 PoB2「用当前
    /// 池重算」）；树版本失配的实际症状=已分配节点不在已加载树中，用
    /// [`crate::diagnose_tree_version`] 显性化（calc 现状静默跳过，见 pobr-tree node.rs）。
    pub tree_version: Option<String>,
    /// 装备：按 [`EquipmentSlot`] 索引。遍历时按 `slot.id()` 字典序排序以保证确定性
    /// （`EquipmentSlot` 仅实现 `Hash` 不实现 `Ord`，REAL 类型不可改）。
    pub items: HashMap<EquipmentSlot, Item>,
    /// 珠宝（天赋树/深渊槽，无固定 [`EquipmentSlot`]）。其词条按全局作用注入
    /// （多数珠宝为全局；radius 珠宝当前按全局近似）。
    pub jewels: Vec<Item>,
    /// 范围珠宝（`... in Radius also grant <mod>`）的几何展开输入。与 `jewels` 并存：
    /// `jewels` 注入珠宝**自身**的全局词条，本列表额外按半径几何把 `also grant` 展开为
    /// 「半径内已分配对应种类节点数 × 授予」的全局 mod（见 `calc_orchestrator`）。
    pub radius_jewels: Vec<RadiusJewel>,
    /// 非激活武器组专属点（`<WeaponSet1/2>` 中属于未激活组的已分配节点）。
    ///
    /// 这些节点的**自身** mod 已在解析层 masking（不进 [`Self::tree`] 的 `allocated_nodes`，
    /// 等价 PoB2 `Condition: WeaponSet<N>` 门控）；但 PoB2 仍把它们留在 `allocNodes`，
    /// 因此**范围珠宝授予**（`... in Radius also grant`）按 PoB2 语义仍会落到这些节点上
    /// （CalcSetup.lua:209-228，授予源=jewel、按 jewel allocMode 门控）。此列表供
    /// `radius_jewel_expansions` 在 radius 几何里并回完整已分配集，复刻该行为。
    /// 无武器组切换的 build 此列表为空，对计算零影响。
    pub inactive_weapon_set_nodes: Vec<NodeId>,
    /// **激活态**药剂/护符的「槽名 + 物品」（M3-T4，蓝图 §7.2-2）：
    /// `("Flask 1"|"Charm 1..3", Item)`，XML 文档序，仅 `active="true"` 的槽进入
    /// （vendor CalcSetup.lua:1014-1028 `slot.active` 门控）。槽名供 flask/charm
    /// 结构化通道做 `SourceId(Flask, "flask.<slot>")` 归因与 flask/charm 分类，
    /// 经 `ingest_flask_charm` → env_finalize 阶段 3 `merge_flasks_charms` 进计算。
    pub utility_slots: Vec<(String, Item)>,
    /// 技能宝石组。
    pub socket_groups: Vec<SocketGroup>,
    /// 主技能组索引（PoB `<Build mainSocketGroup>`，**1-based**，指向 `socket_groups`）。
    /// `None`=未指定（退化为启发式选首个伤害技能）。
    pub main_socket_group: Option<usize>,
    /// Build 级配置。
    pub config: BuildConfig,
}

impl Build {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_character(mut self, character: CharacterIdentity) -> Self {
        self.character = character;
        self
    }

    pub fn with_view_mode(mut self, view_mode: ViewMode) -> Self {
        self.view_mode = view_mode;
        self
    }

    /// 设定主技能组索引（PoB `mainSocketGroup`，1-based）。
    pub fn with_main_socket_group(mut self, group: usize) -> Self {
        self.main_socket_group = Some(group);
        self
    }

    pub fn with_tree(mut self, tree: PassiveTreeSpec) -> Self {
        self.tree = tree;
        self
    }

    /// 设定该 build 的 PoB 天赋树版本标注（`<Spec treeVersion>`），返回新副本。
    pub fn with_tree_version(mut self, version: Option<String>) -> Self {
        self.tree_version = version;
        self
    }

    pub fn with_config(mut self, config: BuildConfig) -> Self {
        self.config = config;
        self
    }

    /// 设置某槽位装备，返回新副本。
    pub fn set_item(mut self, slot: EquipmentSlot, item: Item) -> Self {
        self.items.insert(slot, item);
        self
    }

    /// 设定珠宝列表，返回新副本。
    pub fn with_jewels(mut self, jewels: Vec<Item>) -> Self {
        self.jewels = jewels;
        self
    }

    /// 设定范围珠宝（radius jewel）几何展开列表，返回新副本。
    pub fn with_radius_jewels(mut self, radius_jewels: Vec<RadiusJewel>) -> Self {
        self.radius_jewels = radius_jewels;
        self
    }

    /// 设定非激活武器组专属点列表（见 [`Self::inactive_weapon_set_nodes`]），返回新副本。
    pub fn with_inactive_weapon_set_nodes(mut self, nodes: Vec<NodeId>) -> Self {
        self.inactive_weapon_set_nodes = nodes;
        self
    }

    /// 设定激活态药剂/护符的槽名保留列表（见 [`Self::utility_slots`]），返回新副本。
    pub fn with_utility_slots(mut self, slots: Vec<(String, Item)>) -> Self {
        self.utility_slots = slots;
        self
    }

    /// 追加一个技能宝石组，返回新副本。
    pub fn add_socket_group(mut self, group: SocketGroup) -> Self {
        self.socket_groups.push(group);
        self
    }

    /// 按 `slot.id()` 字典序的确定性顺序遍历已装备物品（槽位 + 物品）。
    pub fn equipped_items(&self) -> Vec<(EquipmentSlot, &Item)> {
        let mut entries: Vec<(EquipmentSlot, &Item)> = self
            .items
            .iter()
            .map(|(slot, item)| (*slot, item))
            .collect();
        entries.sort_by_key(|(slot, _)| slot.id());
        entries
    }

    /// 已启用的技能宝石组。
    pub fn enabled_socket_groups(&self) -> impl Iterator<Item = &SocketGroup> {
        self.socket_groups.iter().filter(|g| g.enabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pobr_data::item::{ItemBaseId, ItemRarity, RolledDefence};

    fn sample_item() -> Item {
        Item {
            base: ItemBaseId::from("Iron Ring"),
            rarity: ItemRarity::Rare,
            quality: 0,
            implicit_texts: vec![],
            modifier_texts: vec!["+10 to maximum Life".into()],
            enchant_texts: vec![],
            rolled_defence: RolledDefence::default(),
            parsed_stats: vec![],
        }
    }

    #[test]
    fn build_accumulates_items_deterministically() {
        let build = Build::new()
            .set_item(EquipmentSlot::Ring1, sample_item())
            .set_item(EquipmentSlot::Amulet, sample_item());
        let slots: Vec<_> = build.equipped_items().into_iter().map(|(s, _)| s).collect();
        assert_eq!(slots.len(), 2);
        assert!(slots.contains(&EquipmentSlot::Ring1));
        assert!(slots.contains(&EquipmentSlot::Amulet));
    }

    #[test]
    fn enabled_socket_groups_filters_disabled() {
        let build = Build::new()
            .add_socket_group(SocketGroup::new().with_gem("a").with_enabled(true))
            .add_socket_group(SocketGroup::new().with_gem("b").with_enabled(false));
        let enabled: Vec<_> = build.enabled_socket_groups().collect();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].gem_ids, vec!["a".to_string()]);
    }
}
