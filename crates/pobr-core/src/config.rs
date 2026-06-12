use std::collections::HashMap;

use pobr_data::prelude::*;

/// PerStat 的 actor output 读数函数（`stat 名 → output 值`；缺键 `None`）。
pub type StatLookup<'a> = &'a dyn Fn(&str) -> Option<f64>;

/// EvalMod 求值上下文（M4-T1 W-A3，蓝图 m4-offence-deep.md §3.3 契约 5）。
///
/// [`crate::Modifier::effective_number`] 的入参从 `&CalcConfig` 升级为本类型；
/// `From<&CalcConfig>` + `impl Into` 签名使全部既有调用点（传 `&cfg`）**零改动**
/// 编译（契约 5 预告的机械迁移面收敛为 0）。`matches` 仍取 `&CalcConfig`
/// （PerStat/GlobalLimit 不参与匹配过滤）。
///
/// `stat_lookup` = PerStat tag 的 actor **output** 读数通道（vendor
/// `ModStore.lua:280-325 GetStat`：`self.actor.output[stat] or cfg.skillStats
/// or 0`）——由消费方（T2/T4 的 pass 编排）在只读快照阶段提供；`None` =
/// 无快照，PerStat 取 0（保守等价 vendor output 缺位）。
#[derive(Clone, Copy)]
pub struct EvalContext<'a> {
    /// 匹配/条件/乘数上下文（既有通道）。
    pub cfg: &'a CalcConfig,
    /// `stat 名 → actor output 值`。`None` = 整体无快照。
    pub stat_lookup: Option<StatLookup<'a>>,
}

impl<'a> EvalContext<'a> {
    /// 仅 cfg、无 output 快照（等价 `From<&CalcConfig>`）。
    pub fn new(cfg: &'a CalcConfig) -> Self {
        Self {
            cfg,
            stat_lookup: None,
        }
    }

    /// 带 actor output 读数通道（PerStat 消费方用）。
    pub fn with_stat_lookup(cfg: &'a CalcConfig, lookup: StatLookup<'a>) -> Self {
        Self {
            cfg,
            stat_lookup: Some(lookup),
        }
    }

    /// vendor `GetStat` 默认路径：output 快照值，缺位 → 0（ModStore.lua:323
    /// `(self.actor.output and self.actor.output[stat]) or ... or 0`）。
    pub fn stat(&self, name: &str) -> f64 {
        self.stat_lookup
            .and_then(|lookup| lookup(name))
            .unwrap_or(0.0)
    }
}

impl<'a> From<&'a CalcConfig> for EvalContext<'a> {
    fn from(cfg: &'a CalcConfig) -> Self {
        Self::new(cfg)
    }
}

impl std::fmt::Debug for EvalContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvalContext")
            .field("cfg", &self.cfg)
            .field("stat_lookup", &self.stat_lookup.map(|_| "<fn>"))
            .finish()
    }
}

#[derive(Debug, Clone, Default)]
pub struct CalcConfig {
    pub flags: ModFlags,
    pub keyword_flags: KeywordFlags,
    pub skill_types: SkillTypes,
    pub damage_type: Option<DamageType>,
    pub conditions: HashMap<String, bool>,
    pub multipliers: HashMap<String, f64>,
    /// 额外的伤害缩放 ModName（按主技能关键词 / 武器类别派生，如 `GrenadeDamage`、
    /// `CrossbowDamage`）。`damage::aggregate_inc_more` 把它们纳入通用增伤桶，使
    /// `increased Grenade Damage` / `Damage with Crossbows` 对该技能生效。
    pub damage_keywords: Vec<String>,
    /// 有效 DPS 口径开关（PoB2 `env.mode_effective`）。
    ///
    /// - `false`（默认，面板/裸 DPS 口径）：进攻计算**不**引入敌人 modDB 的减伤
    ///   （抗性/护甲/`DamageTaken`/格挡）。命中率沿用现有标量 evasion 口径，保证与
    ///   历史输出一致（向后兼容）。
    /// - `true`（有效 DPS）：伤害末端乘 `enemy.mod_db` 的 `DamageTaken` 链、扣减敌人
    ///   抗性/护甲、扣敌人格挡，并启用敌人 `CannotEvade` 短路。
    ///
    /// 出处：agent-docs/accuracy-and-enemy.md §七（buffMode → mode_effective 口径表）、
    /// devs/docs/architecture/12-combat-mechanics-architecture.md §5。
    pub mode_effective: bool,
    /// buffMode 三态之 buffs 维度（PoB2 CalcSetup.lua:582-605：BUFFED/COMBAT/EFFECTIVE
    /// 均含 buffs）。门控 `env_finalize` 的 buff_pass 整段（aura/curse/debuff 分发）。
    ///
    /// 默认 **false**（与 `mode_effective` 默认一致）——未显式置位的既有调用方逐值不变；
    /// pobr-build 编排入口对 MAIN 口径显式置 true（PoB2 非 CALCS 模式恒 EFFECTIVE）。
    /// 蓝图 m3-orchestration.md §1 D5。
    pub mode_buffs: bool,
    /// buffMode 三态之 combat 维度（PoB2 CalcSetup.lua:582-605：COMBAT/EFFECTIVE 含
    /// combat）。门控 doActorMisc 等价段（expand_misc_buffs）、战斗条件自动置位
    /// （CalcPerform.lua:242-260）、flask/charm 合并。
    ///
    /// 默认 **false**，语义与 [`CalcConfig::mode_buffs`] 同步引入（M3 T0-2，蓝图 D5）。
    pub mode_combat: bool,
    /// 跨 actor multiplier 快照（M3 T0 为 S2-D 预留，本阶段零消费）。
    ///
    /// 对应 PoB2 ModStore EvalMod 的 `actor`/`limitActor` tag：`Multiplier`/`PerStat`
    /// 读取上下文切到 `env.player`/`env.minion`/parent 时，从此表按
    /// `"<actor>.<var>"`（如 `"player.PowerCharges"`）取对方 actor 的值。
    /// 由编排层在只读快照阶段回填；空表 = 行为与引入前逐值一致。
    pub actor_multipliers: HashMap<String, f64>,
    /// 注入的运行时常量包（M0-W3，架构文档 20 §1 P8/P9）。
    ///
    /// calc 公式中的全部游戏常量魔数（抗性边界 / 服务器帧 / 异常基线 / 各类 cap…）
    /// 改读此包；`Default` = fallback（与 `base/game_constants.json` 逐值相等，
    /// 无 GameData 时行为不变）。挂在 `CalcConfig` 上是因为 cfg 已线程化到全部
    /// calc 函数——这是把常量送达每个使用点的最小侵入通道。
    ///
    /// 注入入口：`CalculationSession::set_constants`（pobr-build
    /// `calculate_with_data` 在 `with_config` 之后调用；注意 `with_config`
    /// 会整体覆盖 cfg，故注入必须在其后）。
    pub constants: RuntimeConstants,
}

impl CalcConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn attack() -> Self {
        Self::new()
            .with_flags(ModFlags::ATTACK)
            .with_skill_types(SkillTypes::ATTACK)
    }

    pub fn spell() -> Self {
        Self::new()
            .with_flags(ModFlags::SPELL)
            .with_skill_types(SkillTypes::SPELL)
    }

    /// 是否为法术（PoE2 法术必中，不做精准/闪避检定）。
    /// 出处：agent-docs/accuracy-and-enemy.md §三：`if not isAttack then output.AccuracyHitChance = 100`。
    pub fn is_spell(&self) -> bool {
        self.skill_types.intersects(SkillTypes::SPELL)
    }

    /// 是否为攻击（需要精准/闪避命中检定）。
    pub fn is_attack(&self) -> bool {
        self.skill_types.intersects(SkillTypes::ATTACK)
    }

    pub fn with_flags(mut self, flags: ModFlags) -> Self {
        self.flags = flags;
        self
    }

    pub fn with_keyword_flags(mut self, keyword_flags: KeywordFlags) -> Self {
        self.keyword_flags = keyword_flags;
        self
    }

    pub fn with_skill_types(mut self, skill_types: SkillTypes) -> Self {
        self.skill_types = skill_types;
        self
    }

    pub fn with_damage_type(mut self, damage_type: DamageType) -> Self {
        self.damage_type = Some(damage_type);
        self
    }

    /// 设定额外伤害缩放 ModName（技能关键词 / 武器类别派生）。
    pub fn with_damage_keywords(mut self, names: Vec<String>) -> Self {
        self.damage_keywords = names;
        self
    }

    pub fn with_condition(mut self, name: impl Into<String>, enabled: bool) -> Self {
        self.conditions.insert(name.into(), enabled);
        self
    }

    pub fn with_multiplier(mut self, name: impl Into<String>, value: f64) -> Self {
        self.multipliers.insert(name.into(), value);
        self
    }

    /// 设置有效 DPS 口径开关（见 [`CalcConfig::mode_effective`]）。
    pub fn with_mode_effective(mut self, mode_effective: bool) -> Self {
        self.mode_effective = mode_effective;
        self
    }

    /// 设置 buffMode 之 buffs 维度（见 [`CalcConfig::mode_buffs`]）。
    pub fn with_mode_buffs(mut self, mode_buffs: bool) -> Self {
        self.mode_buffs = mode_buffs;
        self
    }

    /// 设置 buffMode 之 combat 维度（见 [`CalcConfig::mode_combat`]）。
    pub fn with_mode_combat(mut self, mode_combat: bool) -> Self {
        self.mode_combat = mode_combat;
        self
    }

    /// 注入运行时常量包（见 [`CalcConfig::constants`]）。未调用时为 `Default`
    /// （fallback，与入库 JSON 逐值相等）。
    pub fn with_constants(mut self, constants: RuntimeConstants) -> Self {
        self.constants = constants;
        self
    }

    pub fn condition(&self, name: &str) -> bool {
        // PoB2 `mode_effective` 派生条件：`Condition:Effective` 用于门控只在有效 DPS 口径
        // 才生效的敌侧 debuff（curse/exposure/slow effect-on-self）。显式置入的 `Effective`
        // 条件优先（便于测试覆盖），未显式置入时回退为 `mode_effective`。
        if name == "Effective"
            && let Some(explicit) = self.conditions.get(name)
        {
            return *explicit;
        }
        if name == "Effective" {
            return self.mode_effective;
        }
        self.conditions.get(name).copied().unwrap_or(false)
    }

    pub fn multiplier(&self, name: &str) -> f64 {
        self.multipliers.get(name).copied().unwrap_or(0.0)
    }

    /// 写入跨 actor multiplier 快照（见 [`CalcConfig::actor_multipliers`]；键形如
    /// `"player.PowerCharge"`）。供编排层在只读快照阶段回填 / 测试构造。
    pub fn with_actor_multiplier(
        mut self,
        actor: crate::ActorRef,
        var: impl AsRef<str>,
        value: f64,
    ) -> Self {
        self.actor_multipliers
            .insert(format!("{}.{}", actor.key(), var.as_ref()), value);
        self
    }

    /// 按 actor 维度读跨 actor multiplier 快照（[`ModTag`](crate::ModTag) 的
    /// `actor`/`limit_actor` 求值通道）。缺键＝0.0——保守等价 PoB2 ModStore.lua
    /// getActor 缺位时 mod 不生效的口径。
    pub fn actor_multiplier(&self, actor: crate::ActorRef, var: &str) -> f64 {
        self.actor_multipliers
            .get(&format!("{}.{}", actor.key(), var))
            .copied()
            .unwrap_or(0.0)
    }
}
