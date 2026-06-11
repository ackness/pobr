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

    // --- 防御恢复扩展（Lane A：充能状态 / 偷取 / Recoup；perform fill 写入，无来源时中性 0） ---
    /// 充能当前/最大层数（Power / Frenzy / Endurance）。
    pub charge_power_current: u32,
    pub charge_power_maximum: u32,
    pub charge_frenzy_current: u32,
    pub charge_frenzy_maximum: u32,
    pub charge_endurance_current: u32,
    pub charge_endurance_maximum: u32,
    /// 偷取面板速率（每秒；0 = 无偷取）。
    pub life_leech_rate: f64,
    pub mana_leech_rate: f64,
    pub es_leech_rate: f64,
    /// Recoup 返还速率（每秒；依赖 damage_taken 估算基准）。
    pub life_recoup_rate: f64,
    pub es_recoup_rate: f64,

    // --- 异常扩展（Lane B：冰缓 / 冰冻·电击姿态积累 / 流血·中毒叠层；perform fill_ailments 写入） ---
    /// 冰缓行动速度降低（%，如 30.0 = 30%；0 = 不施加）。
    pub chill_effect: f64,
    /// 冰冻姿态积累（% per hit；0 = 不积累）。
    pub freeze_buildup_pct: f64,
    /// 电击姿态积累（% per hit；0 = 不积累）。
    pub electrocute_buildup_pct: f64,
    /// 流血/中毒/点燃多层 DPS 与活跃层数（0 = 无叠层配置；默认 max_stacks=1，stacked==单层）。
    pub bleed_stacked_dps: f64,
    pub bleed_active_stacks: f64,
    pub poison_stacked_dps: f64,
    pub poison_active_stacks: f64,
    pub ignite_stacked_dps: f64,
    pub ignite_active_stacks: f64,

    // --- 技能功能（Lane C：AoE / 投射物 / 冷却 / 消耗；perform fill 写入，无 base 时 0） ---
    /// AoE 最终半径（内部坐标单位）与面积乘数。
    pub aoe_radius: f64,
    pub aoe_area_mod: f64,
    /// 投射物数量（无投射物词条时为 0）。
    pub projectile_count: f64,
    /// 冷却（秒；0 = 无冷却）与可储存使用次数。
    pub cooldown: f64,
    pub cooldown_stored_uses: u32,
    /// 资源消耗（final_cost；无 base 配置时为 0）。
    pub mana_cost: f64,
    pub life_cost: f64,
    pub spirit_reserved: f64,

    // --- M2 防御扩展（W0.2 契约字段：默认 0 中性；A–F track 分批接线写入。
    //     golden 参照 = examples/demo-bd-test/builds/*/meta.json::player_stats 同名键） ---
    /// Spirit 池本值（base × inc × more + Override，PoB2 doActorLifeManaSpirit 同构）。
    pub spirit: f64,
    /// Spirit 未预留余量（= spirit − spirit_reserved；vendor CalcDefence.lua:337
    /// 无下限，超订为负——golden `SpiritUnreserved` 存在 −130 等负值，
    /// W0.2 原注释「下限 0」与 vendor/golden 不符，D-3 接线时修正）。
    pub spirit_unreserved: f64,
    /// 格挡几率上限（%；PoB2 `BlockChanceMax`，CalcDefence.lua:961-966）。
    pub block_chance_max: f64,
    /// 法术格挡几率上限（%；PoB2 `SpellBlockChanceMax`）。
    pub spell_block_chance_max: f64,
    /// 有效格挡几率（%；lucky/unlucky 幂后，PoB2 `EffectiveBlockChance`，
    /// CalcDefence.lua:1030-1058）。
    pub effective_block_chance: f64,
    /// 有效法术格挡几率（%；PoB2 `EffectiveSpellBlockChance`）。
    pub effective_spell_block_chance: f64,
    /// 格挡承伤比例（%；被格挡命中仍承受的伤害份额，PoB2 `BlockEffect`，
    /// ModParser.lua:2479；0 = 完全格挡）。
    pub block_effect: f64,
    /// 偏斜等级（PoB2 `DeflectionRating` = BASE + Evasion/Armour GainAsDeflection，
    /// CalcDefence.lua:1487-1490）。
    pub deflection_rating: f64,
    /// 偏斜几率（%；PoB2 `DeflectChance` = deflectChance(rating, enemyAccuracy)，
    /// CalcDefence.lua:48-54、:1491）。
    pub deflect_chance: f64,
    /// 综合闪避几率（%；PoB2 `EvadeChance`，CalcDefence.lua:1396-1466）。
    pub evade_chance: f64,
    /// 近战闪避几率（%；PoB2 `MeleeEvadeChance`，四分型独立 inc 乘区）。
    pub melee_evade_chance: f64,
    /// 投射物闪避几率（%；PoB2 `ProjectileEvadeChance`）。
    pub projectile_evade_chance: f64,
    /// 法术闪避几率（%；PoB2 `SpellEvadeChance`）。
    pub spell_evade_chance: f64,
    /// 法术投射物闪避几率（%；PoB2 `SpellProjectileEvadeChance`）。
    pub spell_projectile_evade_chance: f64,
    /// 眩晕阈值（PoB2 `StunThreshold`，基 Life/ES/Mana 词条切换，
    /// CalcDefence.lua:2525-2643）。
    pub stun_threshold: f64,
    /// 受眩晕几率（%；PoB2 `SelfStunChance` = StunBaseMult × 有效伤/阈值）。
    pub self_stun_chance: f64,
    /// 受眩晕持续（秒；按 ServerTickRate 上取整）。
    pub stun_duration: f64,
    /// 结界池（PoB2 `Ward`，per-slot 聚合 + EnergyShieldToWard，
    /// CalcDefence.lua:1144-1273）。
    pub ward: f64,
    /// 可恢复生命池（PoB2 `LifeRecoverable`，EHP 循环的生命池口径）。
    pub life_recoverable: f64,
    /// ES 恢复上限池（PoB2 `EnergyShieldRecoveryCap`，EHP 循环的 ES 池口径）。
    pub energy_shield_recovery_cap: f64,
    /// 面板物理减伤（%；PoB2 `PhysicalDamageReduction`，参考击中下的护甲 DR）。
    pub physical_damage_reduction: f64,
    /// 致死所需命中数（PoB2 `NumberOfDamagingHits`，CalcDefence.lua:2979-3153）。
    pub number_of_damaging_hits: f64,
    /// 计入 not-hit/block/deflect 概率层后的致死命中数（PoB2
    /// `NumberOfMitigatedDamagingHits`，CalcDefence.lua:3246-3247）。
    pub number_of_mitigated_hits: f64,
    /// 旧口径综合 EHP（各类型 max hit 取 min；Track F 切换 `total_ehp` 语义为
    /// PoB2 口径（mitigatedHits × totalEnemyDamageIn，:3322）后，旧值保留于此作附加指标）。
    pub total_ehp_lowest_max_hit: f64,
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
            // Lane A：充能 / 偷取 / Recoup 默认中性 0（无来源词条时不影响面板）。
            charge_power_current: 0,
            charge_power_maximum: 0,
            charge_frenzy_current: 0,
            charge_frenzy_maximum: 0,
            charge_endurance_current: 0,
            charge_endurance_maximum: 0,
            life_leech_rate: 0.0,
            mana_leech_rate: 0.0,
            es_leech_rate: 0.0,
            life_recoup_rate: 0.0,
            es_recoup_rate: 0.0,
            // Lane B：异常扩展默认 0（无对应命中/词条时不施加）。
            chill_effect: 0.0,
            freeze_buildup_pct: 0.0,
            electrocute_buildup_pct: 0.0,
            bleed_stacked_dps: 0.0,
            bleed_active_stacks: 0.0,
            poison_stacked_dps: 0.0,
            poison_active_stacks: 0.0,
            ignite_stacked_dps: 0.0,
            ignite_active_stacks: 0.0,
            // Lane C：技能功能默认 0（无 base 配置时不写入）。
            aoe_radius: 0.0,
            aoe_area_mod: 0.0,
            projectile_count: 0.0,
            cooldown: 0.0,
            cooldown_stored_uses: 0,
            mana_cost: 0.0,
            life_cost: 0.0,
            spirit_reserved: 0.0,
            // M2 防御扩展（W0.2）：未接线前全部 0 中性（ParityStatus=Planned，
            // 不进 extract_display_values）。
            spirit: 0.0,
            spirit_unreserved: 0.0,
            block_chance_max: 0.0,
            spell_block_chance_max: 0.0,
            effective_block_chance: 0.0,
            effective_spell_block_chance: 0.0,
            block_effect: 0.0,
            deflection_rating: 0.0,
            deflect_chance: 0.0,
            evade_chance: 0.0,
            melee_evade_chance: 0.0,
            projectile_evade_chance: 0.0,
            spell_evade_chance: 0.0,
            spell_projectile_evade_chance: 0.0,
            stun_threshold: 0.0,
            self_stun_chance: 0.0,
            stun_duration: 0.0,
            ward: 0.0,
            life_recoverable: 0.0,
            energy_shield_recovery_cap: 0.0,
            physical_damage_reduction: 0.0,
            number_of_damaging_hits: 0.0,
            number_of_mitigated_hits: 0.0,
            total_ehp_lowest_max_hit: 0.0,
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

#[cfg(test)]
mod m2_default_neutral_tests {
    use super::OutputTable;

    /// M2-W0.2 中性不变式：新防御扩展字段在 `Default` 下全部为 0（未接线前不影响
    /// 任何既有输出/比较；A–F track 接线后由 perform fill 写入）。
    #[test]
    fn m2_defence_extension_fields_default_to_zero() {
        let out = OutputTable::default();
        for (name, v) in [
            ("spirit", out.spirit),
            ("spirit_unreserved", out.spirit_unreserved),
            ("block_chance_max", out.block_chance_max),
            ("spell_block_chance_max", out.spell_block_chance_max),
            ("effective_block_chance", out.effective_block_chance),
            (
                "effective_spell_block_chance",
                out.effective_spell_block_chance,
            ),
            ("block_effect", out.block_effect),
            ("deflection_rating", out.deflection_rating),
            ("deflect_chance", out.deflect_chance),
            ("evade_chance", out.evade_chance),
            ("melee_evade_chance", out.melee_evade_chance),
            ("projectile_evade_chance", out.projectile_evade_chance),
            ("spell_evade_chance", out.spell_evade_chance),
            (
                "spell_projectile_evade_chance",
                out.spell_projectile_evade_chance,
            ),
            ("stun_threshold", out.stun_threshold),
            ("self_stun_chance", out.self_stun_chance),
            ("stun_duration", out.stun_duration),
            ("ward", out.ward),
            ("life_recoverable", out.life_recoverable),
            ("energy_shield_recovery_cap", out.energy_shield_recovery_cap),
            ("physical_damage_reduction", out.physical_damage_reduction),
            ("number_of_damaging_hits", out.number_of_damaging_hits),
            ("number_of_mitigated_hits", out.number_of_mitigated_hits),
            ("total_ehp_lowest_max_hit", out.total_ehp_lowest_max_hit),
        ] {
            assert_eq!(v, 0.0, "{name} 默认应为 0（中性）");
        }
    }
}
