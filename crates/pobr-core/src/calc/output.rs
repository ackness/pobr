use super::{DamageComponent, MinimalOutput, SkillUseTime};

#[derive(Debug, Clone, PartialEq)]
pub struct OutputTable {
    pub life: f64,
    pub mana: f64,
    pub armour: f64,
    pub evasion: f64,
    pub energy_shield: f64,
    pub chance_to_be_hit: f64,
    pub fire_resistance: f64,
    pub cold_resistance: f64,
    pub lightning_resistance: f64,
    pub max_fire_resistance: f64,
    pub max_cold_resistance: f64,
    pub max_lightning_resistance: f64,
    pub fire_resistance_over_cap: f64,
    pub cold_resistance_over_cap: f64,
    pub lightning_resistance_over_cap: f64,
    pub crit_chance: f64,
    /// 命中降级 / 幸运 / 分岔 / 必然之前、cap 之后的暴击几率（fraction）。供 breakdown 显示溢出。
    pub pre_effective_crit_chance: f64,
    pub crit_multiplier: f64,
    /// 按伤害类型拆分的非暴击击中分量；求和即非暴击总击中伤害。
    pub damage_components: Vec<DamageComponent>,
    pub total_hit_avg: f64,
    pub hit_chance: f64,
    pub action_rate: f64,
    pub dps: f64,

    // --- 追加机制字段（perform 的 fill 阶段写入；Default 0/None） ---
    /// 技能使用时间 / 行动速率解析结果。
    pub skill_use_time: Option<SkillUseTime>,
    /// 应用服务器帧上限后的有效行动速率（actions/s）。
    pub effective_action_rate: f64,
    /// 异常状态 DPS。
    pub bleed_dps: f64,
    pub ignite_dps: f64,
    pub poison_dps: f64,
    /// 感电增伤幅度（fraction，如 0.20）。
    pub shock_effect: f64,
    /// 各伤害类型最大可承受单次命中。
    pub physical_max_hit: f64,
    pub fire_max_hit: f64,
    pub cold_max_hit: f64,
    pub lightning_max_hit: f64,
    pub chaos_max_hit: f64,
    /// 综合 EHP（取各类型 max hit 最低）。
    pub total_ehp: f64,
    /// 生命 / 法力预留与剩余。
    pub life_reserved: f64,
    pub life_unreserved: f64,
    pub mana_reserved: f64,
    pub mana_unreserved: f64,
    /// 每秒恢复。
    pub life_regen: f64,
    pub mana_regen: f64,
    pub energy_shield_regen: f64,
    /// 防御几率类。
    pub block_chance: f64,
    pub spell_block_chance: f64,
    pub spell_suppression_chance: f64,

    // --- 防御扩展（Lane2：ES 充能 / 规避 / 承受乘数 / 暴击减免；perform fill 写入） ---
    /// ES 充能速率（每秒恢复比例，fraction；ZealotsOath 或 es=0 时为 0）。
    pub es_recharge_rate: f64,
    /// ES 充能开始延迟（秒；默认 4.0）。
    pub es_recharge_delay: f64,
    /// ES 充能每秒绝对恢复量（rate_fraction × energy_shield）。
    pub es_recharge_per_second: f64,
    /// 规避几率类（avoidance）：击中 / 投射物 / 各异常（百分比）。
    pub avoid_all_damage_from_hits: f64,
    pub avoid_projectile_damage: f64,
    pub avoid_stun: f64,
    pub avoid_ignite: f64,
    pub avoid_shock: f64,
    pub avoid_chill: f64,
    pub avoid_freeze: f64,
    pub avoid_poison: f64,
    pub avoid_bleeding: f64,
    /// 承受伤害乘数（受击口径，fraction；1.0 = 无减伤/增伤）。
    pub taken_multi_physical: f64,
    pub taken_multi_fire: f64,
    pub taken_multi_cold: f64,
    pub taken_multi_lightning: f64,
    pub taken_multi_chaos: f64,
    /// 减少承受的暴击额外伤害（百分比，0–100）。
    pub crit_extra_damage_reduction: f64,
    /// 敌人暴击效果乘数（加权平均伤害倍率，≥ 1.0）。
    pub enemy_crit_effect: f64,

    // --- 召唤物快照（Lane4：每个召唤物各自 offence/defence 输出；perform 多 Actor 写入） ---
    /// 各召唤物的关键输出快照（无召唤物时为空）。
    pub minions: Vec<MinionOutput>,

    // --- 触发（Lane4：触发速率上限/实际触发速率；perform 写入，未涉及时为 0） ---
    /// 触发速率上限（次/秒）。
    pub trigger_rate_cap: f64,
    /// 实际触发速率（次/秒）= min(上限, 有效源速率)。
    pub skill_trigger_rate: f64,
}

/// 单个召唤物的输出快照（结构同玩家 offence/defence 关键输出的子集）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MinionOutput {
    /// 召唤物等级（怪物等级）。
    pub level: u32,
    /// 召唤物 DPS（走玩家同款 offence 管线）。
    pub dps: f64,
    /// 生命池。
    pub life: f64,
    /// 护甲。
    pub armour: f64,
    /// 闪避。
    pub evasion: f64,
    /// 能量护盾。
    pub energy_shield: f64,
}

impl Default for OutputTable {
    fn default() -> Self {
        Self {
            life: 0.0,
            mana: 0.0,
            armour: 0.0,
            evasion: 0.0,
            energy_shield: 0.0,
            chance_to_be_hit: 0.0,
            fire_resistance: 0.0,
            cold_resistance: 0.0,
            lightning_resistance: 0.0,
            max_fire_resistance: 0.0,
            max_cold_resistance: 0.0,
            max_lightning_resistance: 0.0,
            fire_resistance_over_cap: 0.0,
            cold_resistance_over_cap: 0.0,
            lightning_resistance_over_cap: 0.0,
            crit_chance: 0.0,
            pre_effective_crit_chance: 0.0,
            crit_multiplier: 0.0,
            damage_components: Vec::new(),
            total_hit_avg: 0.0,
            hit_chance: 0.0,
            action_rate: 0.0,
            dps: 0.0,
            skill_use_time: None,
            effective_action_rate: 0.0,
            bleed_dps: 0.0,
            ignite_dps: 0.0,
            poison_dps: 0.0,
            shock_effect: 0.0,
            physical_max_hit: 0.0,
            fire_max_hit: 0.0,
            cold_max_hit: 0.0,
            lightning_max_hit: 0.0,
            chaos_max_hit: 0.0,
            total_ehp: 0.0,
            life_reserved: 0.0,
            life_unreserved: 0.0,
            mana_reserved: 0.0,
            mana_unreserved: 0.0,
            life_regen: 0.0,
            mana_regen: 0.0,
            energy_shield_regen: 0.0,
            block_chance: 0.0,
            spell_block_chance: 0.0,
            spell_suppression_chance: 0.0,
            es_recharge_rate: 0.0,
            es_recharge_delay: 0.0,
            es_recharge_per_second: 0.0,
            avoid_all_damage_from_hits: 0.0,
            avoid_projectile_damage: 0.0,
            avoid_stun: 0.0,
            avoid_ignite: 0.0,
            avoid_shock: 0.0,
            avoid_chill: 0.0,
            avoid_freeze: 0.0,
            avoid_poison: 0.0,
            avoid_bleeding: 0.0,
            // 承受乘数 / 敌人暴击效果默认中性（1.0 = 无减伤/增伤）。
            taken_multi_physical: 1.0,
            taken_multi_fire: 1.0,
            taken_multi_cold: 1.0,
            taken_multi_lightning: 1.0,
            taken_multi_chaos: 1.0,
            crit_extra_damage_reduction: 0.0,
            enemy_crit_effect: 1.0,
            minions: Vec::new(),
            trigger_rate_cap: 0.0,
            skill_trigger_rate: 0.0,
        }
    }
}

impl From<&MinimalOutput> for OutputTable {
    fn from(value: &MinimalOutput) -> Self {
        Self {
            life: value.life,
            mana: value.mana,
            fire_resistance: value.fire_resistance,
            cold_resistance: value.cold_resistance,
            lightning_resistance: value.lightning_resistance,
            max_fire_resistance: value.max_fire_resistance,
            max_cold_resistance: value.max_cold_resistance,
            max_lightning_resistance: value.max_lightning_resistance,
            fire_resistance_over_cap: value.fire_resistance_over_cap,
            cold_resistance_over_cap: value.cold_resistance_over_cap,
            lightning_resistance_over_cap: value.lightning_resistance_over_cap,
            crit_chance: value.crit_chance,
            pre_effective_crit_chance: value.pre_effective_crit_chance,
            crit_multiplier: value.crit_multiplier,
            damage_components: value.damage_components.clone(),
            total_hit_avg: value.total_hit_avg,
            hit_chance: value.hit_chance,
            action_rate: value.action_rate,
            dps: value.dps,
            ..Self::default()
        }
    }
}
