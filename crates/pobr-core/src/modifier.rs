use pobr_data::prelude::*;

use crate::CalcConfig;

#[derive(Debug, Clone, PartialEq)]
pub enum ModValue {
    Number(f64),
    Bool(bool),
    Text(String),
    /// 嵌套 modifier 载荷（PoB2 LIST mod 的 table 值形态）。
    ///
    /// 典型用途：`EnemyModifier` 词条——外层 mod 落在 player db 上，内层 mods 由
    /// 编排层（env_finalize 的 `forward_enemy_modifiers`）经 [`crate::ModDb::list_nested`]
    /// 透传转发到目标 db。数值/布尔/文本通道对该变体一律返回 `None`（不参与
    /// sum/more/flag/override 聚合）。
    NestedMods(Vec<Modifier>),
}

impl ModValue {
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            Self::Text(_) | Self::NestedMods(_) => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            Self::Number(value) => Some(*value != 0.0),
            Self::Text(_) | Self::NestedMods(_) => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            Self::Number(_) | Self::Bool(_) | Self::NestedMods(_) => None,
        }
    }

    /// 嵌套 modifier 载荷（仅 [`Self::NestedMods`] 返回 `Some`）。
    pub fn as_nested_mods(&self) -> Option<&[Modifier]> {
        match self {
            Self::NestedMods(mods) => Some(mods),
            Self::Number(_) | Self::Bool(_) | Self::Text(_) => None,
        }
    }
}

/// 跨 actor 取数引用（PoB2 ModStore.lua `getActor`：tag 上的 `actor`/`limitActor`
/// 字段把 `Multiplier`/`Condition` 的读取上下文从「当前 actor」切到对方 actor）。
///
/// 求值通道是 [`CalcConfig::actor_multipliers`](crate::CalcConfig) 只读快照（键形如
/// `"player.PowerCharge"`，由编排层在只读快照阶段回填，沿用 SummonedMinion 注入
/// 玩家 multiplier 的先例一般化）。`key()` 给出快照键前缀。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorRef {
    /// 顶层玩家（PoB2 `tag.actor == "player"`：minion 词条引用玩家侧数值）。
    Player,
    /// 直接父 actor（PoB2 `tag.actor == "parent"`：如 Agony Crawler 引用玩家 virulence）。
    Parent,
    /// 召唤物（PoB2 `tag.actor == "minion"`：玩家词条引用召唤物侧数值）。
    Minion,
}

impl ActorRef {
    /// [`CalcConfig::actor_multipliers`](crate::CalcConfig) 快照键前缀。
    pub fn key(self) -> &'static str {
        match self {
            Self::Player => "player",
            Self::Parent => "parent",
            Self::Minion => "minion",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModTag {
    Condition {
        var: String,
        negated: bool,
        /// 跨 actor 条件（PoB2 `ActorCondition` tag 的 actor 维度）。`None`（缺省）＝
        /// 读当前 `cfg.condition(var)`，行为与引入前逐字一致；`Some(actor)` ＝ 查
        /// `cfg.actor_multipliers["<actor>.<var>"]` 真值（≠0 为真；快照缺键＝假）。
        actor: Option<ActorRef>,
    },
    /// 按某资源/属性数量线性缩放（PoB2 `Multiplier` / `PerStat` tag）。
    ///
    /// 有效值 = `cfg.multiplier(var) / div`（再受 `limit` 上限约束）。
    /// - 充能数类（`per power charge`）：`div = 1`，`var = PowerCharge` 等。
    /// - 资源/属性类（`per 1 Spirit`、`per 10 Intelligence`、`per 5 player levels`）：
    ///   `div = N`，`var` 为资源名（`Spirit`/`Strength`/`Dexterity`/`Intelligence`/
    ///   `Level`/`Armour`/`Evasion`/`EnergyShield`/`Mana`/`Life` 等）。
    Multiplier {
        var: String,
        /// 每多少单位资源缩放一次（PoB2 `div`）。`per power charge` 等无除数时为 `1.0`。
        div: f64,
        limit: Option<f64>,
        /// 跨 actor 取数（PoB2 ModStore.lua:347-353 `tag.actor`）。`None`（缺省）＝读
        /// 当前 `cfg.multiplier(var)`，行为与引入前逐字一致；`Some(actor)` ＝ 查
        /// `cfg.actor_multipliers["<actor>.<var>"]`（快照缺键＝0）。
        actor: Option<ActorRef>,
        /// 动态上限变量（PoB2 ModStore.lua:369 `tag.limitVar`：`limit = tag.limit or
        /// GetMultiplier(limitTarget, tag.limitVar)`——静态 `limit` 优先）。
        limit_var: Option<String>,
        /// 动态上限的取数 actor（PoB2 ModStore.lua:338-345 `tag.limitActor`，如 Agony
        /// Crawler 以玩家 virulence 为上限）。`None` ＝ 当前 `cfg.multiplier(limit_var)`。
        limit_actor: Option<ActorRef>,
    },
    DamageType(DamageType),
    SkillTypes(SkillTypes),
    /// 槽位限定（PoB2 `calcLib.mod({slotName=slot})`）：该 modifier 仅作用于匹配槽位的
    /// per-slot 防御聚合（如 `80% increased Armour from Equipped Body Armour`）。
    ///
    /// **不参与 [`Modifier::matches`] 的普通过滤**（对 `sum`/`more` 等全局查询透明）——
    /// 由 [`crate::ModDb`] 的 per-slot 查询路径（`sum_for_slot`/`more_for_slot`）显式按槽位读取，
    /// 避免影响进攻 / 其他全局查询语义。槽名为稳定槽位 ID（见 `EquipmentSlot::id`）。
    SlotName(String),
}

impl ModTag {
    /// 同 actor 布尔条件（`actor: None`，行为与字段引入前逐字一致）。
    /// 跨 actor 条件请用结构体字面量显式给 `actor`。
    pub fn condition(var: impl Into<String>, negated: bool) -> Self {
        Self::Condition {
            var: var.into(),
            negated,
            actor: None,
        }
    }

    /// 同 actor 数量缩放（`actor`/`limit_var`/`limit_actor` 均 `None`，行为与字段
    /// 引入前逐字一致）。跨 actor / 动态上限请用结构体字面量显式给字段。
    pub fn multiplier(var: impl Into<String>, div: f64, limit: Option<f64>) -> Self {
        Self::Multiplier {
            var: var.into(),
            div,
            limit,
            actor: None,
            limit_var: None,
            limit_actor: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Modifier {
    pub name: ModName,
    pub mod_type: ModType,
    pub value: ModValue,
    pub source: Option<String>,
    pub origin: Option<ModifierSource>,
    pub flags: ModFlags,
    pub keyword_flags: KeywordFlags,
    pub tags: Vec<ModTag>,
}

impl Modifier {
    pub fn number(name: impl Into<ModName>, mod_type: ModType, value: f64) -> Self {
        Self::new(name, mod_type, ModValue::Number(value))
    }

    pub fn flag(name: impl Into<ModName>) -> Self {
        Self::new(name, ModType::Flag, ModValue::Bool(true))
    }

    pub fn text(name: impl Into<ModName>, mod_type: ModType, value: impl Into<String>) -> Self {
        Self::new(name, mod_type, ModValue::Text(value.into()))
    }

    pub fn new(name: impl Into<ModName>, mod_type: ModType, value: ModValue) -> Self {
        Self {
            name: name.into(),
            mod_type,
            value,
            source: None,
            origin: None,
            flags: ModFlags::NONE,
            keyword_flags: KeywordFlags::NONE,
            tags: Vec::new(),
        }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_origin(mut self, mut origin: ModifierSource) -> Self {
        if origin.stat_id.is_none() {
            origin.stat_id = Some(self.name.clone());
        }
        if origin.mod_type.is_none() {
            origin.mod_type = Some(self.mod_type);
        }
        self.origin = Some(origin);
        self
    }

    pub fn with_flags(mut self, flags: ModFlags) -> Self {
        self.flags = flags;
        self
    }

    pub fn with_keyword_flags(mut self, keyword_flags: KeywordFlags) -> Self {
        self.keyword_flags = keyword_flags;
        self
    }

    pub fn with_tag(mut self, tag: ModTag) -> Self {
        self.tags.push(tag);
        self
    }

    pub fn matches(&self, cfg: &CalcConfig) -> bool {
        // PoB2 ModList.lua：`band(cfg.flags, mod.flags) == mod.flags` —— mod.flags 必须是
        // cfg.flags 的子集（mod 上每个 flag 都被 cfg 满足才生效），而非任一重叠（intersects）。
        // 空 flag（NONE）是任意集合子集 → 恒匹配，涵盖原 is_empty 短路。
        if !self.flags.is_subset_of(cfg.flags) {
            return false;
        }

        // PoB2 Global.lua `MatchKeywordFlags`：mod 去掉 MatchAll 后为空 → 恒匹配；带 MatchAll →
        // cfg 须含 mod 全部 keyword（ALL）；否则任一重叠即可（ANY）。当前全 NONE 下退化为恒真。
        if !self.keyword_flags.matches_context(cfg.keyword_flags) {
            return false;
        }

        self.tags.iter().all(|tag| match tag {
            ModTag::Condition {
                var,
                negated,
                actor,
            } => {
                // 跨 actor 条件（PoB2 ActorCondition）：查 actor_multipliers 快照真值
                // （≠0 为真，缺键＝假——等价 PoB2 getActor 失败时 mod 不生效的保守口径）。
                let enabled = match actor {
                    None => cfg.condition(var),
                    Some(actor) => cfg.actor_multiplier(*actor, var) != 0.0,
                };
                if *negated { !enabled } else { enabled }
            }
            ModTag::Multiplier { .. } => true,
            ModTag::DamageType(damage_type) => cfg.damage_type == Some(*damage_type),
            ModTag::SkillTypes(skill_types) => {
                skill_types.is_empty() || skill_types.intersects(cfg.skill_types)
            }
            // 槽位限定对普通过滤透明（由 ModDb 的 per-slot 查询路径显式处理）。
            ModTag::SlotName(_) => true,
        })
    }

    /// 该 modifier 的槽位限定（若有 [`ModTag::SlotName`]）。供 per-slot 防御聚合按槽过滤。
    pub fn slot_name(&self) -> Option<&str> {
        self.tags.iter().find_map(|tag| match tag {
            ModTag::SlotName(slot) => Some(slot.as_str()),
            _ => None,
        })
    }

    pub fn effective_number(&self, cfg: &CalcConfig) -> Option<f64> {
        let mut value = self.value.as_number()?;

        for tag in &self.tags {
            if let ModTag::Multiplier {
                var,
                div,
                limit,
                actor,
                limit_var,
                limit_actor,
            } = tag
            {
                // 取数源按 actor 维度切换（PoB2 ModStore.lua:347-353 `tag.actor` →
                // getActor(self, ...).modDB）：None＝当前 cfg.multiplier；Some＝
                // actor_multipliers 快照（缺键＝0，保守等价 PoB2 actor 缺位不生效）。
                let base = match actor {
                    None => cfg.multiplier(var),
                    Some(actor) => cfg.actor_multiplier(*actor, var),
                };
                // PoB2 ModStore.lua EvalMod（Multiplier L365 / PerStat L460）：
                // `mult = m_floor(base / (tag.div or 1) + 0.0001)` —— 资源数除以 div 后向下取整
                // （+epsilon 抵消浮点误差）再作乘数，floor 先于 min(limit)。整倍场景（div=1、整数
                // 资源）floor 无影响；仅修正 `per 10 Strength` 在 95 力量等非整倍情形（旧值 9.5→9）。
                let count = (base / div.max(f64::EPSILON) + 0.0001).floor();
                // 上限解析（PoB2 ModStore.lua:369 `local limit = tag.limit or
                // GetMultiplier(limitTarget, tag.limitVar, cfg)`——静态 limit 优先，
                // 动态 limit_var 按 limit_actor 维度取数）。
                let effective_limit = limit.or_else(|| {
                    limit_var.as_ref().map(|lv| match limit_actor {
                        None => cfg.multiplier(lv),
                        Some(actor) => cfg.actor_multiplier(*actor, lv),
                    })
                });
                value *= effective_limit.map_or(count, |max| count.min(max));
            }
        }

        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 锚点：`actor: None` 的 Multiplier/Condition 行为与字段引入前逐字一致
    /// （E1 搬迁不变式，golden diff=0 的单元级对应）。
    #[test]
    fn none_actor_keeps_legacy_behavior() {
        let cfg = CalcConfig::new()
            .with_multiplier("PowerCharge", 3.0)
            .with_condition("FullLife", true);

        let mult = Modifier::number("Damage", ModType::Inc, 10.0).with_tag(ModTag::multiplier(
            "PowerCharge",
            1.0,
            None,
        ));
        assert_eq!(mult.effective_number(&cfg), Some(30.0));

        let cond = Modifier::number("Damage", ModType::Inc, 10.0)
            .with_tag(ModTag::condition("FullLife", false));
        assert!(cond.matches(&cfg));
    }

    /// `actor: Some(_)` 的 Multiplier 改查 `cfg.actor_multipliers["<actor>.<var>"]`；
    /// 快照缺键＝0（保守等价 PoB2 getActor 缺位）。
    #[test]
    fn actor_multiplier_reads_actor_snapshot() {
        let cfg = CalcConfig::new()
            .with_multiplier("Virulence", 5.0) // 本 actor 值，不应被读到。
            .with_actor_multiplier(ActorRef::Parent, "Virulence", 12.0);

        let modifier = Modifier::number("Damage", ModType::Inc, 2.0).with_tag(ModTag::Multiplier {
            var: "Virulence".into(),
            div: 1.0,
            limit: None,
            actor: Some(ActorRef::Parent),
            limit_var: None,
            limit_actor: None,
        });
        assert_eq!(modifier.effective_number(&cfg), Some(24.0));

        // 快照缺键 → 乘数 0。
        let missing = Modifier::number("Damage", ModType::Inc, 2.0).with_tag(ModTag::Multiplier {
            var: "Virulence".into(),
            div: 1.0,
            limit: None,
            actor: Some(ActorRef::Minion),
            limit_var: None,
            limit_actor: None,
        });
        assert_eq!(missing.effective_number(&cfg), Some(0.0));
    }

    /// 动态上限：静态 `limit` 优先于 `limit_var`（PoB2 `tag.limit or GetMultiplier(...)`）；
    /// `limit_var` 按 `limit_actor` 维度取数。
    #[test]
    fn limit_var_resolves_dynamic_limit() {
        let cfg = CalcConfig::new()
            .with_multiplier("PowerCharge", 9.0)
            .with_multiplier("MaxCharges", 4.0)
            .with_actor_multiplier(ActorRef::Player, "MaxCharges", 6.0);

        // limit_var 走本 actor multipliers。
        let local = Modifier::number("Damage", ModType::Inc, 1.0).with_tag(ModTag::Multiplier {
            var: "PowerCharge".into(),
            div: 1.0,
            limit: None,
            actor: None,
            limit_var: Some("MaxCharges".into()),
            limit_actor: None,
        });
        assert_eq!(local.effective_number(&cfg), Some(4.0));

        // limit_actor 切到对方 actor 快照。
        let cross = Modifier::number("Damage", ModType::Inc, 1.0).with_tag(ModTag::Multiplier {
            var: "PowerCharge".into(),
            div: 1.0,
            limit: None,
            actor: None,
            limit_var: Some("MaxCharges".into()),
            limit_actor: Some(ActorRef::Player),
        });
        assert_eq!(cross.effective_number(&cfg), Some(6.0));

        // 静态 limit 优先于 limit_var。
        let static_wins =
            Modifier::number("Damage", ModType::Inc, 1.0).with_tag(ModTag::Multiplier {
                var: "PowerCharge".into(),
                div: 1.0,
                limit: Some(2.0),
                actor: None,
                limit_var: Some("MaxCharges".into()),
                limit_actor: Some(ActorRef::Player),
            });
        assert_eq!(static_wins.effective_number(&cfg), Some(2.0));
    }

    /// `actor: Some(_)` 的 Condition 改查 actor 快照真值（≠0 为真，缺键＝假）。
    #[test]
    fn actor_condition_reads_actor_snapshot() {
        let cfg = CalcConfig::new().with_actor_multiplier(ActorRef::Player, "Blind", 1.0);

        let hit = Modifier::number("Damage", ModType::Inc, 10.0).with_tag(ModTag::Condition {
            var: "Blind".into(),
            negated: false,
            actor: Some(ActorRef::Player),
        });
        assert!(hit.matches(&cfg));

        let missing = Modifier::number("Damage", ModType::Inc, 10.0).with_tag(ModTag::Condition {
            var: "Maimed".into(),
            negated: false,
            actor: Some(ActorRef::Player),
        });
        assert!(!missing.matches(&cfg));

        // negated 语义在 actor 维度同样适用。
        let negated = Modifier::number("Damage", ModType::Inc, 10.0).with_tag(ModTag::Condition {
            var: "Maimed".into(),
            negated: true,
            actor: Some(ActorRef::Player),
        });
        assert!(negated.matches(&cfg));
    }
}
