use std::collections::HashMap;

use pobr_data::prelude::*;

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
}
