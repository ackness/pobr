use pobr_data::prelude::*;

use crate::{CalcConfig, EvalContext};

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
        /// 倒数缩放（PoB2 ModStore.lua:378-380 `tag.invert`：limit 之后
        /// `mult = 1/mult`，mult 为 0 时保持 0——如 Elemental Conflux 三元素
        /// MORE 按 `Multiplier:ElementalConflux<El>Effect`（Average 档 = 3）
        /// 取 1/3 均摊）。
        invert: bool,
        /// 总量限幅（PoB2 ModStore.lua:370-371 + 402-404 `tag.limitTotal`）：为真时
        /// `limit`/`limit_var` **不**截断乘数 `mult`，而是在 `value × mult` 之后对
        /// **最终贡献**封顶（`value = min(value, limit)`）。如「每层中毒 +N% 伤害，
        /// 至多 +M%」（`Multiplier{var, limit=M, limitTotal}`）。缺省 `false` ＝ 旧的
        /// 计数封顶（`mult = min(mult, limit)`）。
        limit_total: bool,
    },
    /// 按 actor **已算出 stat（output 表）**线性缩放（M4-T1 W-A3；PoB2 `PerStat`
    /// tag，ModStore.lua:440-489）。与 [`ModTag::Multiplier`] 拆开：Multiplier 读
    /// 编排层预灌的 `cfg.multipliers`，PerStat 读 [`EvalContext::stat_lookup`]
    /// （actor output 快照；缺通道/缺键 → 0，保守等价 vendor GetStat 缺位）。
    ///
    /// 有效乘数 = `floor(stat / div + 0.0001)`，再受 `limit`（静态优先）或
    /// `limit_var`（`cfg.multiplier(limit_var)`，vendor :462 GetMultiplier(self)）
    /// 上限约束。vendor 的 `statList`/`divVar`/`limitTotal`/`base` 偏置形态
    /// 本批不做（无消费方，登记 10-G3 余量）。
    PerStat {
        /// output 表 stat 名（如 `Life`/`Mana`/`Armour`）。
        stat: String,
        /// 每多少单位缩放一次（vendor `tag.div or 1`）。
        div: f64,
        /// 静态上限（vendor `tag.limit`，优先于 `limit_var`）。
        limit: Option<f64>,
        /// 动态上限变量（vendor `tag.limitVar` → `GetMultiplier(self, ·)`）。
        limit_var: Option<String>,
        /// 跨 actor 读数（与 M3 落地的 Multiplier `actor` 形态统一：`Some` →
        /// 查 `cfg.actor_multipliers["<actor>.<stat>"]` 快照，缺键＝0）。
        actor: Option<ActorRef>,
    },
    /// 跨 mod 累计限幅（M4-T1 W-A3；PoB2 EvalMod 尾段 ModStore.lua:895-905
    /// `tag.globalLimit`/`tag.globalLimitKey`）：同 `key` 的 mod 生效值在**单次
    /// 聚合查询内**（vendor 每次 Sum/More/Tabulate 调用新建 `globalLimits` 表）
    /// 累计封顶——超限部分截断，余额记账。
    ///
    /// vendor 把这两个字段挂在任意 tag 上；pobr 形态化为独立 tag（语义不变，
    /// 由 [`crate::ModDb`] 聚合循环消费；对 [`Modifier::matches`] 透明）。
    /// W-C1（chance-to-deal-Double-Damage DOUBLED form）是首个消费方。
    GlobalLimit {
        /// 累计上限（vendor `tag.globalLimit`）。
        value: f64,
        /// 记账桶键（vendor `tag.globalLimitKey`，如 `"DoubleDamage"`）。
        key: String,
    },
    /// 按某 multiplier 是否越过阈值的二元 gate（PoB2 `MultiplierThreshold` tag，
    /// ModStore.lua:559-573）。典型来源「against enemies within/further than N metres」
    /// → `var = "enemyDistance"`，`threshold = N×10`（米→单位）。
    ///
    /// 生效判定（vendor `if (upper and stat>th) or (not upper and stat<th) then return`，
    /// 落在错误一侧时跳过该 mod）：读 `cfg.multiplier(var)` 为 `stat`——
    /// - `upper = true`（within，近）：`stat ≤ threshold` 时生效；
    /// - `upper = false`（further，远）：`stat ≥ threshold` 时生效。
    ///
    /// `enemyDistance` 由编排层从 `Multiplier:enemyDistance`（Condition:Effective，
    /// 默认 20）折入 cfg.multipliers（effective＝20、panel＝0）。异常叠层形态
    /// （`<X>Stacks`, threshold=1）仍由 parser 扁平化为 `Condition{Enemy<X>}`，不走本 tag。
    MultiplierThreshold {
        var: String,
        threshold: f64,
        /// `true` = within（`stat ≤ threshold` 生效）；`false` = further（`≥` 生效）。
        upper: bool,
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
    /// 按敌方距离线性插值缩放（PoB2 `DistanceRamp` tag，ModStore.lua:574-590）：
    /// Close Combat / Far Combat / Point Blank / Far Shot 等「近/远战伤害随距离变化」
    /// 的 MORE/INC 词条。生效值 = `base × interp(ramp, skillDist)`——`skillDist` 取
    /// [`CalcConfig::skill_distance`]（vendor `skillCfg.skillDist = env.mode_effective
    /// and configInput.enemyDistance`，**仅 effective 口径 + enemyDistance 的 `<Input>`
    /// 显式值**，不含 `<Placeholder>` 占位值）。
    ///
    /// `ramp` 为 `(距离, 倍率)` 升序点列：`skillDist ≤ 首点距离` → 取首点倍率；
    /// `≥ 末点距离` → 取末点倍率；区间内线性插值。**panel 口径 / enemyDistance 仅
    /// placeholder 未设 Input**（`skill_distance == None`）→ [`Modifier::effective_number`]
    /// 返回 `None`（整条 mod 跳过），镜像 vendor `if not cfg.skillDist then return end`。
    /// demo 套件 18 个 build 的 enemyDistance 全是 placeholder → 此 tag 全休眠，
    /// 与 golden 一致（PoB2 同样不应用 Close Combat 距离 MORE）。
    DistanceRamp {
        /// `(距离, 倍率)` 点列，按距离升序（如 Close Combat `[(10,1),(35,0)]`）。
        ramp: Vec<(f64, f64)>,
    },
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
            invert: false,
            limit_total: false,
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
            // 数值缩放 / 累计限幅 / 距离插值 tag 不参与匹配过滤（求值期消费）。
            ModTag::Multiplier { .. }
            | ModTag::PerStat { .. }
            | ModTag::GlobalLimit { .. }
            | ModTag::DistanceRamp { .. } => true,
            // 阈值 gate（vendor ModStore.lua:559-573）：stat 落在错误一侧 → 不生效。
            ModTag::MultiplierThreshold {
                var,
                threshold,
                upper,
            } => {
                let stat = cfg.multiplier(var);
                if *upper {
                    stat <= *threshold
                } else {
                    stat >= *threshold
                }
            }
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

    /// 生效数值（应用 Multiplier / PerStat 缩放 tag）。
    ///
    /// （M4-T1 W-A3，契约 5）入参升级为 [`EvalContext`]；`impl Into` + `From<&CalcConfig>`
    /// 使既有调用点（传 `&cfg`）零改动——仅 PerStat 消费方需显式构造带
    /// `stat_lookup` 的上下文。[`ModTag::GlobalLimit`] 不在此结算（跨 mod 记账，
    /// 归 [`crate::ModDb`] 聚合循环，vendor 同样在 EvalMod 尾段由聚合层传表）。
    #[inline]
    pub fn effective_number<'a>(&self, ctx: impl Into<EvalContext<'a>>) -> Option<f64> {
        self.effective_number_ref(&ctx.into())
    }

    /// [`effective_number`](Self::effective_number) 的引用入参形态——mod_db 聚合
    /// 热路径用（单指针传参，避免逐 mod 拷贝 [`EvalContext`]；bench 门禁敏感）。
    #[inline]
    pub(crate) fn effective_number_ref(&self, ctx: &EvalContext<'_>) -> Option<f64> {
        let cfg = ctx.cfg;
        let mut value = self.value.as_number()?;

        for tag in &self.tags {
            match tag {
                ModTag::Multiplier {
                    var,
                    div,
                    limit,
                    actor,
                    limit_var,
                    limit_actor,
                    invert,
                    limit_total,
                } => {
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
                    // limitTotal（vendor :370-371）：limit 不截断 mult，留待 value×mult
                    // 之后封顶最终贡献；否则计数封顶（:375 `mult = min(mult, limit)`）。
                    let mut count = if *limit_total {
                        count
                    } else {
                        effective_limit.map_or(count, |max| count.min(max))
                    };
                    // 倒数缩放（PoB2 ModStore.lua:378-380：limit 之后
                    // `if tag.invert and mult ~= 0 then mult = 1 / mult end`）。
                    if *invert && count != 0.0 {
                        count = 1.0 / count;
                    }
                    value *= count;
                    // 总量封顶（vendor :402-404 `value = m_min(value, limitTotal)`）——
                    // 作用于本 tag 乘算后的累计贡献。
                    if *limit_total && let Some(max) = effective_limit {
                        value = value.min(max);
                    }
                }
                ModTag::PerStat {
                    stat,
                    div,
                    limit,
                    limit_var,
                    actor,
                } => {
                    // （W-A3）读 actor output 快照（vendor ModStore.lua:440-455
                    // PerStat 分支 → GetStat）；跨 actor 维度与 Multiplier 统一走
                    // actor_multipliers 快照。
                    let base = match actor {
                        None => ctx.stat(stat),
                        Some(actor) => cfg.actor_multiplier(*actor, stat),
                    };
                    // vendor :460 `mult = m_floor(base / (tag.div or 1) + 0.0001)`。
                    let count = (base / div.max(f64::EPSILON) + 0.0001).floor();
                    // vendor :461-468：limit = tag.limit or GetMultiplier(self, limitVar)
                    // → mult = min(mult, limit)（limitTotal 形态本批不做，见 tag doc）。
                    let effective_limit =
                        limit.or_else(|| limit_var.as_ref().map(|lv| cfg.multiplier(lv)));
                    value *= effective_limit.map_or(count, |max| count.min(max));
                }
                ModTag::DistanceRamp { ramp } => {
                    // vendor ModStore.lua:574-590：`skillDist` 缺位 → 整条 mod 跳过
                    // （`return`）。`cfg.skill_distance` = vendor `skillCfg.skillDist`
                    // （`mode_effective and configInput.enemyDistance`，**仅 Input 值**，
                    // 不含 placeholder——见 [`CalcConfig::skill_distance`]）。`None` →
                    // 返回 None 让 ModDb 聚合跳过该 mod（区别于 Multiplier 的 0 倍率：
                    // 距离 0 仍会按 ramp 首点倍率生效，故必须 None 跳过而非按 0 插值）。
                    let dist = cfg.skill_distance?;
                    value *= ramp_factor(ramp, dist)?;
                }
                // MultiplierThreshold 是二元 gate（在 matches 里求值），不缩放数值。
                ModTag::Condition { .. }
                | ModTag::MultiplierThreshold { .. }
                | ModTag::GlobalLimit { .. }
                | ModTag::DamageType(_)
                | ModTag::SkillTypes(_)
                | ModTag::SlotName(_) => {}
            }
        }

        Some(value)
    }
}

/// 距离插值倍率（PoB2 ModStore.lua:578-589 `DistanceRamp` 分支逐句对译）。
///
/// `ramp` 为 `(距离, 倍率)` 升序点列；`dist`（= skillDist）夹在两点之间时线性插值，
/// 越过首/末点则取端点倍率（clamp）。空点列返回 `None`（防御，使整条 mod 跳过）。
fn ramp_factor(ramp: &[(f64, f64)], dist: f64) -> Option<f64> {
    let first = *ramp.first()?;
    let last = *ramp.last()?;
    if dist <= first.0 {
        return Some(first.1);
    }
    if dist >= last.0 {
        return Some(last.1);
    }
    // 找包夹 `dist` 的相邻点对线性插值（vendor :583-588 同序）。
    for pair in ramp.windows(2) {
        let (d0, m0) = pair[0];
        let (d1, m1) = pair[1];
        if dist <= d1 {
            return Some(m0 + (m1 - m0) * (dist - d0) / (d1 - d0));
        }
    }
    // 理论不可达（dist < last.0 必落入某区间）；保守取末点。
    Some(last.1)
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
            invert: false,
            limit_total: false,
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
            invert: false,
            limit_total: false,
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
            invert: false,
            limit_total: false,
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
            invert: false,
            limit_total: false,
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
                invert: false,
                limit_total: false,
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

    /// DistanceRamp（Close Combat `[(10,1),(35,0)]`）按 `skill_distance` 线性插值：
    /// 距离 20 → 倍率 0.6 → 30% MORE × 0.6 = 18%（vendor ModStore.lua:586 同算）。
    #[test]
    fn distance_ramp_interpolates_with_skill_distance() {
        let modifier =
            Modifier::number("Damage", ModType::More, 30.0).with_tag(ModTag::DistanceRamp {
                ramp: vec![(10.0, 1.0), (35.0, 0.0)],
            });
        // 距离 20：插值 1 + (0-1)*(20-10)/(35-10) = 0.6 → 30 × 0.6 = 18。
        let cfg = CalcConfig::new().with_skill_distance(Some(20.0));
        assert_eq!(modifier.effective_number(&cfg), Some(18.0));
    }

    /// DistanceRamp 端点 clamp：≤ 首点距离取首点倍率，≥ 末点距离取末点倍率。
    #[test]
    fn distance_ramp_clamps_at_endpoints() {
        let modifier =
            Modifier::number("Damage", ModType::More, 30.0).with_tag(ModTag::DistanceRamp {
                ramp: vec![(10.0, 1.0), (35.0, 0.0)],
            });
        // 距离 5 ≤ 10 → 倍率 1.0 → 30。
        let close = CalcConfig::new().with_skill_distance(Some(5.0));
        assert_eq!(modifier.effective_number(&close), Some(30.0));
        // 距离 50 ≥ 35 → 倍率 0.0 → 0。
        let far = CalcConfig::new().with_skill_distance(Some(50.0));
        assert_eq!(modifier.effective_number(&far), Some(0.0));
    }

    /// DistanceRamp 无 `skill_distance`（panel 口径 / enemyDistance 仅 placeholder 未设
    /// Input）→ 整条 mod 跳过（`effective_number` 返回 `None`，镜像 vendor
    /// `if not cfg.skillDist then return`）。demo 套件全 build 走此路径，匹配 golden。
    #[test]
    fn distance_ramp_skipped_without_skill_distance() {
        let modifier =
            Modifier::number("Damage", ModType::More, 30.0).with_tag(ModTag::DistanceRamp {
                ramp: vec![(10.0, 1.0), (35.0, 0.0)],
            });
        let cfg = CalcConfig::new();
        assert_eq!(modifier.effective_number(&cfg), None);
    }

    /// MultiplierThreshold（vendor ModStore.lua:559-573）：`within`（upper）在
    /// `stat ≤ threshold` 生效；`further`（!upper）在 `stat ≥ threshold` 生效。
    #[test]
    fn multiplier_threshold_within_and_further() {
        let within = Modifier::number("CriticalStrikeMultiplier", ModType::Inc, 40.0).with_tag(
            ModTag::MultiplierThreshold {
                var: "enemyDistance".into(),
                threshold: 20.0,
                upper: true,
            },
        );
        let further =
            Modifier::number("Damage", ModType::Inc, 10.0).with_tag(ModTag::MultiplierThreshold {
                var: "enemyDistance".into(),
                threshold: 30.0,
                upper: false,
            });

        // within 2m（≤20）：敌距 20 → 生效；敌距 25 → 不生效。
        assert!(within.matches(&CalcConfig::new().with_multiplier("enemyDistance", 20.0)));
        assert!(!within.matches(&CalcConfig::new().with_multiplier("enemyDistance", 25.0)));
        // further 3m（≥30）：敌距 30 → 生效；敌距 20 → 不生效。
        assert!(further.matches(&CalcConfig::new().with_multiplier("enemyDistance", 30.0)));
        assert!(!further.matches(&CalcConfig::new().with_multiplier("enemyDistance", 20.0)));
        // 缺 enemyDistance（默认 0）：within ≤ threshold 恒真；further ≥ threshold 恒假。
        assert!(within.matches(&CalcConfig::new()));
        assert!(!further.matches(&CalcConfig::new()));
    }
}
