use pobr_data::prelude::*;

use crate::CalcConfig;

#[derive(Debug, Clone, PartialEq)]
pub enum ModValue {
    Number(f64),
    Bool(bool),
    Text(String),
}

impl ModValue {
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            Self::Text(_) => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            Self::Number(value) => Some(*value != 0.0),
            Self::Text(_) => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            Self::Number(_) | Self::Bool(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModTag {
    Condition {
        var: String,
        negated: bool,
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

        if !self.keyword_flags.is_empty() && !self.keyword_flags.intersects(cfg.keyword_flags) {
            return false;
        }

        self.tags.iter().all(|tag| match tag {
            ModTag::Condition { var, negated } => {
                let enabled = cfg.condition(var);
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
            if let ModTag::Multiplier { var, div, limit } = tag {
                // PoB2 ModStore.lua EvalMod（Multiplier L365 / PerStat L460）：
                // `mult = m_floor(base / (tag.div or 1) + 0.0001)` —— 资源数除以 div 后向下取整
                // （+epsilon 抵消浮点误差）再作乘数，floor 先于 min(limit)。整倍场景（div=1、整数
                // 资源）floor 无影响；仅修正 `per 10 Strength` 在 95 力量等非整倍情形（旧值 9.5→9）。
                let count = (cfg.multiplier(var) / div.max(f64::EPSILON) + 0.0001).floor();
                value *= limit.map_or(count, |max| count.min(max));
            }
        }

        Some(value)
    }
}
