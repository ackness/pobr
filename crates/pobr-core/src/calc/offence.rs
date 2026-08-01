use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, TraceGraph, TraceNodeId, TraceOperation, TraceOutput, TracedValue};

use super::crit::{resolve_crit, resolve_crit_traced};
use super::crit_pass::run_crit_passes;
use super::damage::DamageComponent;
use super::scaled_damage::{dps_end_factors, scaled_damage_effect};
use super::{ActorBaseStats, BreakdownStep, BreakdownTable, OutputTable, hit_chance, round};

#[derive(Debug, Clone, Copy, Default)]
pub struct MinimalInput {
    pub base_life: f64,
    pub base_mana: f64,
    pub base_fire_resistance: f64,
    pub base_cold_resistance: f64,
    pub base_lightning_resistance: f64,
    pub base_accuracy: f64,
    pub enemy_evasion: f64,
    pub base_hit_min: f64,
    pub base_hit_max: f64,
    pub base_action_rate: f64,
}

impl From<ActorBaseStats> for MinimalInput {
    fn from(value: ActorBaseStats) -> Self {
        Self {
            base_life: value.life,
            base_mana: value.mana,
            base_fire_resistance: value.fire_resistance,
            base_cold_resistance: value.cold_resistance,
            base_lightning_resistance: value.lightning_resistance,
            base_accuracy: value.accuracy,
            enemy_evasion: 0.0,
            base_hit_min: value.hit_min,
            base_hit_max: value.hit_max,
            base_action_rate: value.action_rate,
        }
    }
}

impl From<MinimalInput> for ActorBaseStats {
    fn from(value: MinimalInput) -> Self {
        Self {
            life: value.base_life,
            mana: value.base_mana,
            armour: 0.0,
            evasion: 0.0,
            energy_shield: 0.0,
            accuracy: value.base_accuracy,
            fire_resistance: value.base_fire_resistance,
            cold_resistance: value.base_cold_resistance,
            lightning_resistance: value.base_lightning_resistance,
            hit_min: value.base_hit_min,
            hit_max: value.base_hit_max,
            action_rate: value.base_action_rate,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MinimalOutput {
    pub life: f64,
    pub mana: f64,
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
    // ===：Stored 族（vendor CalcOffence.lua:4047-4057，pre-resist、
    // 含 allMult；crit 腿额外 ×CritMultiplier）。ailment magnitude 的 vendor 口径
    // 输入；经 HandOutput 暴露 per-hand 值。===
    pub stored_crit_avg: Vec<(DamageType, f64)>,
    pub stored_hit_avg: Vec<(DamageType, f64)>,
    pub stored_combined_avg: Vec<(DamageType, f64)>,
    /// `Stored<Type>{Hit,Crit}{Min,Max}` 族（append，vendor `:4050-4056`）：
    /// damaging ailment 来源伤害的 min/max 输入面（RollAverage 内插在区间上进行）。
    pub stored_ranges: Vec<super::output::StoredDamageRange>,
    pub breakdown: Vec<BreakdownStep>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TracedMinimalOutput {
    pub output: MinimalOutput,
    pub trace: TraceGraph,
    pub outputs: Vec<TraceOutput>,
}

impl TracedMinimalOutput {
    pub fn node_for(&self, stat: DisplayStatId) -> Option<TraceNodeId> {
        self.outputs
            .iter()
            .find(|output| output.stat == stat)
            .map(|output| output.node_id)
    }
}

impl MinimalOutput {
    pub(crate) fn from_output_and_breakdown(
        output: &OutputTable,
        breakdown: &BreakdownTable,
    ) -> Self {
        Self {
            life: output.life,
            mana: output.mana,
            fire_resistance: output.fire_resistance,
            cold_resistance: output.cold_resistance,
            lightning_resistance: output.lightning_resistance,
            max_fire_resistance: output.max_fire_resistance,
            max_cold_resistance: output.max_cold_resistance,
            max_lightning_resistance: output.max_lightning_resistance,
            fire_resistance_over_cap: output.fire_resistance_over_cap,
            cold_resistance_over_cap: output.cold_resistance_over_cap,
            lightning_resistance_over_cap: output.lightning_resistance_over_cap,
            crit_chance: output.crit_chance,
            pre_effective_crit_chance: output.pre_effective_crit_chance,
            crit_multiplier: output.crit_multiplier,
            damage_components: output.damage_components.clone(),
            total_hit_avg: output.total_hit_avg,
            hit_chance: output.hit_chance,
            action_rate: output.action_rate,
            dps: output.dps,
            // Stored 族经 per-hand 子表回读（OutputTable 顶层不平铺该族；
            // 非攻击/无 hand pass 时为空——与 HandOutput 的 Option 语义一致）。
            stored_crit_avg: output
                .main_hand
                .as_ref()
                .map(|hand| hand.stored_crit_avg.clone())
                .unwrap_or_default(),
            stored_hit_avg: output
                .main_hand
                .as_ref()
                .map(|hand| hand.stored_hit_avg.clone())
                .unwrap_or_default(),
            stored_combined_avg: output
                .main_hand
                .as_ref()
                .map(|hand| hand.stored_combined_avg.clone())
                .unwrap_or_default(),
            stored_ranges: output
                .main_hand
                .as_ref()
                .map(|hand| hand.stored_ranges.clone())
                .unwrap_or_default(),
            breakdown: breakdown.steps().to_vec(),
        }
    }
}

/// 单条抗性的解析结果：capped final / 最大抗性 / over-cap。
pub(crate) struct ResistanceResolution {
    pub(crate) final_value: f64,
    max: f64,
    over_cap: f64,
}

/// 解析一条抗性（vendor CalcDefence.lua:888-930 全通道口径）：
/// - total = Override(`<X>Resistance`/`<X>Resist`)，缺位时
///   `(base + Σ BASE) × max((1 + ΣINC/100) × ΠMORE, 0)`（:891-899，
///   "fire resistance is N%" 走 override，"reduced fire resistance" 走 INC 乘区）
/// - max   = Override(`Maximum<X>Resistance`/`<X>ResistMax`)，缺位时
///   `min(75 + Σ BASE, 90)`（:875/:914——max 的 override **不过** hard_cap）
/// - final = max(min(total, max), −200)（负抗下界 `resist_floor`，
///   :890 `min = data.misc.ResistFloor` / :924 `final = m_max(m_min(total, max), min)`）
/// - over  = max(total - max, 0)
///
/// mod 名取双口径：PoBR parser 长名（`FireResistance`）+ vendor special 通道
/// 短名（`FireResist`），元素类型再并共享名 `ElementalResist`/`ElementalResistMax`
/// （:895 `isElemental[elem]`；override 与 vendor 一致只查单元素名）。enemy 侧
/// （`resolve_enemy_resistance`）早已同构双口径，此处对齐玩家侧。
pub(crate) fn resolve_resistance(
    db: &ModDb,
    cfg: &CalcConfig,
    base: f64,
    element: &str,
    is_elemental: bool,
) -> ResistanceResolution {
    let long = ModName::from(format!("{element}Resistance").as_str());
    let short = ModName::from(format!("{element}Resist").as_str());
    let max_long = ModName::from(format!("Maximum{element}Resistance").as_str());
    let max_short = ModName::from(format!("{element}ResistMax").as_str());

    let mut res_names = vec![long.clone(), short.clone()];
    let mut max_names = vec![max_long.clone(), max_short.clone()];
    if is_elemental {
        res_names.push(ModName::from("ElementalResist"));
        max_names.push(ModName::from("MaximumAllElementalResistances"));
        max_names.push(ModName::from("ElementalResistMax"));
    }

    let total = db
        .override_(cfg, long)
        .or_else(|| db.override_(cfg, short))
        .unwrap_or_else(|| {
            let summed = base + db.sum(ModType::Base, cfg, &res_names);
            let factor = ((1.0 + db.sum(ModType::Inc, cfg, &res_names) / 100.0)
                * db.more(cfg, &res_names))
            .max(0.0);
            summed * factor
        });
    let max = db
        .override_(cfg, max_long)
        .or_else(|| db.override_(cfg, max_short))
        .unwrap_or_else(|| {
            //  默认最大抗性 / 硬上限改读注入常量包（fallback == 旧 const，值不变）。
            (cfg.constants.character().base_maximum_all_resistances_pct
                + db.sum(ModType::Base, cfg, &max_names))
            .min(cfg.constants.game().resist_hard_cap)
        });
    ResistanceResolution {
        final_value: round(total.min(max).max(cfg.constants.game().resist_floor)),
        max: round(max),
        over_cap: round((total - max).max(0.0)),
    }
}

/// 旧三参入口：等价于对**空敌人 modDB** 计算（向后兼容，输出与历史一致）。
///
/// 敌人侧机制（受伤链/抗性护甲减伤/格挡/`CannotEvade`）需要敌人 modDB，
/// 由 [`calculate_minimal_vs_enemy`] 提供；`perform` 走后者。
pub fn calculate_minimal(db: &ModDb, cfg: &CalcConfig, input: &MinimalInput) -> MinimalOutput {
    calculate_minimal_vs_enemy(db, &ModDb::new(), cfg, input)
}

/// 出手速率解析（= vendor `globalOutput.Speed`）：速度族（按技能类型取 AttackSpeed 或
/// CastSpeed，SkillSpeed 始终）作为一个 inc/more 乘区；ActionSpeed 独立乘区单独相乘
/// （对齐 PoB CalcOffence：`finalRate = base × (1+Σinc/100) × Π(more) × ActionSpeedMod`）。
/// 攻击吃武器攻速 + AttackSpeed，法术吃技能施法速率 + CastSpeed——不混淆。
/// 速度 inc/more 缩放后，先按附加施放/攻击时间（TotalCastTime/TotalAttackTime）换算
/// 有效时间，再乘 ActionSpeed 独立乘区（PoB CalcOffence L2827 的加法分母 + 末端 action
/// speed），最后冷却限速 + 服务器帧 cap。
///
/// 独立成函供两处共用：`calculate_minimal_vs_enemy` 主链，以及 warcry uptime 预算
/// （`calc::warcry`——vendor 的 warcry 段读同一 `globalOutput.Speed`，
/// CalcOffence.lua:3235）。
pub(crate) fn resolve_action_rate(db: &ModDb, cfg: &CalcConfig, input: &MinimalInput) -> f64 {
    let speed_names = super::skill_use_time::speed_names_for(cfg);
    let action_speed_names = [ModName::from(super::skill_use_time::ACTION_SPEED)];
    let inc_speed = db.sum(ModType::Inc, cfg, &speed_names);
    let more_speed = db.more(cfg, &speed_names);
    let action_speed_mod = (1.0 + db.sum(ModType::Inc, cfg, &action_speed_names) / 100.0)
        * db.more(cfg, &action_speed_names);
    let scaled_rate = apply_total_time(
        db,
        cfg,
        input.base_action_rate * (1.0 + inc_speed / 100.0) * more_speed,
    );
    let uncapped_action_rate = scaled_rate * action_speed_mod;
    if dbg_env!("POBR_DBG_SPEED").is_some() {
        eprintln!(
            "[POBR_DBG_SPEED] base={} inc={} more={} action={} scaled={} names={:?}",
            input.base_action_rate,
            inc_speed,
            more_speed,
            action_speed_mod,
            scaled_rate,
            speed_names
        );
    }
    round(apply_server_tick_cap(
        db,
        cfg,
        apply_cooldown_cap(db, cfg, uncapped_action_rate),
    ))
}

/// 完整入口：玩家 modDB + 敌人 modDB。敌人侧减伤/受伤链/格挡仅在
/// `cfg.mode_effective == true` 时生效（面板口径不引入敌人交互，保证与历史输出一致）。
///
/// 出处：agent-docs/accuracy-and-enemy.md §二.2,§二.3,§六,§七；
///       devs/docs/architecture/12-combat-mechanics-architecture.md §4.2,§5；
///       PoB2 `CalcOffence.lua`（`enemyDB:Sum/More DamageTaken`、`enemyBlockChance`、`CannotEvade`）。
pub fn calculate_minimal_vs_enemy(
    db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    input: &MinimalInput,
) -> MinimalOutput {
    let life = scaled_pool(db, cfg, input.base_life, "MaximumLife");
    let mana = scaled_pool(db, cfg, input.base_mana, "MaximumMana");
    let fire = resolve_resistance(db, cfg, input.base_fire_resistance, "Fire", true);
    let cold = resolve_resistance(db, cfg, input.base_cold_resistance, "Cold", true);
    let lightning = resolve_resistance(db, cfg, input.base_lightning_resistance, "Lightning", true);
    let fire_resistance = fire.final_value;
    let cold_resistance = cold.final_value;
    let lightning_resistance = lightning.final_value;

    let action_rate = resolve_action_rate(db, cfg, input);
    let accuracy_names = [ModName::from("Accuracy")];
    let accuracy = scaled_numeric_stat(db, cfg, input.base_accuracy, &accuracy_names);
    // PoE2 命中率（agent-docs/accuracy-and-enemy.md §二,§三）：
    // - 非攻击必中（对齐 vendor CalcOffence.lua:2611-2612 `if not isAttack
    //   then output.AccuracyHitChance = 100`）：法术/DoT/召唤等一切非攻击不做精准检定。
    //   旧口径 `is_spell()` 在 skill_types 缺 Spell 位时把法术也卷进精准公式。
    // - `CannotBeEvaded`（玩家旗标）/ effective 下敌方 `CannotEvade` → 置 100% 跳过精准公式。
    // - 末端再扣敌方格挡：`HitChance = AccuracyHitChance * (1 - enemyBlockChance/100)`。
    let cannot_be_evaded = db.flag(cfg, ModName::from("CannotBeEvaded"))
        || (cfg.mode_effective && enemy_db.flag(cfg, ModName::from("CannotEvade")));
    let accuracy_hit_chance = if !cfg.is_attack() || cannot_be_evaded {
        1.0
    } else {
        hit_chance(input.enemy_evasion, accuracy)
    };
    // 敌方格挡：仅有效口径下从命中里扣（accuracy-and-enemy.md §二.3）。
    let enemy_block = if cfg.mode_effective {
        (enemy_db.sum(ModType::Base, cfg, &[ModName::from("BlockChance")]) / 100.0).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let hit_chance = round(accuracy_hit_chance * (1.0 - enemy_block));

    // 有效暴击管线（resolve_crit）：cap / 命中降级 / Lucky / Bifurcate / Inevitable /
    // 敌方 SelfCrit* / NoCritMultiplier，全部对齐 PoB2 CalcOffence.lua（见 calc/crit.rs）。
    // base_crit=0：本最小模型未引入武器底材基础暴击，全部经 db CriticalStrikeChance BASE。
    // 命中降级用 accuracy_hit_chance（格挡不参与暴击降级，PoB2 仅乘 AccuracyHitChance）。
    let crit = resolve_crit(
        db,
        enemy_db,
        cfg,
        accuracy_hit_chance,
        0.0,
        cfg.mode_effective,
    );
    let crit_chance = crit.chance;
    let crit_multiplier = crit.multiplier;

    // 伤害主体：暴击/非暴击双 pass+ T3 乘区接线
    // 击中口径 cfg：补 `KeywordFlags::HIT`（击中本就是 hit）——使 `with Hits` 类
    // keyword 词条（kw=HIT）在击中聚合中命中。kw=NONE 词条恒匹配不受影响（legacy
    // 多产 NONE，逐值不变）；ailment 缩放另经 `ailment_scoped_cfg` 剥 Hit，ignite/
    // bleed 等 DoT base 仍由含 Hit 的击中伤害派生（对齐 PoB2：DoT 继承击中增益）。
    let hit_cfg = cfg
        .clone()
        .with_keyword_flags(cfg.keyword_flags | KeywordFlags::HIT);
    // ScaledDamageEffect（DD/TD 乘区；无词条时 effect == 1.0 逐位不变，
    // m4-t3-wiring-notes §2；crit_chance 是分数入参）。
    let scaled = scaled_damage_effect(db, enemy_db, &hit_cfg, crit.chance);
    // 两腿聚合 + canDeal+ lucky+ CritBlend（vendor :4395）。
    // 无 CriticalStrike 条件词条时短路走旧单因子公式（取整顺序复刻，逐字节等价）。
    let crit_pass = run_crit_passes(
        db,
        &hit_cfg,
        input.base_hit_min,
        input.base_hit_max,
        &crit,
        &scaled,
        cfg.mode_effective,
        |pass_cfg, damage_type, raw_hit| {
            enemy_damage_multiplier(db, enemy_db, pass_cfg, damage_type, raw_hit)
        },
    );
    // 输出字段：玩家侧总击中（不含敌人减伤），保持历史口径 + 作为 ailment magnitude 源。
    let damage_components = crit_pass.non_crit_components.clone();
    let total_hit_avg = crit_pass.total_hit_avg;
    // DPS 用：有效口径下含敌人受伤链/抗性/护甲减伤的总击中。
    let total_hit_avg_for_dps = crit_pass.total_hit_avg_mitigated;

    // DPS 末端两因子（vendor :4407；无词条且技能 dpsMultiplier 未接线（None）
    // 时两因子均 1.0，逐值不变；T4 落 catalog 字段后由编排层透传）。
    let end = dps_end_factors(db, cfg, None);
    let dps = round(
        total_hit_avg_for_dps
            * action_rate
            * hit_chance
            * end.dps_multiplier
            * end.quantity_multiplier,
    );

    MinimalOutput {
        life,
        mana,
        fire_resistance,
        cold_resistance,
        lightning_resistance,
        max_fire_resistance: fire.max,
        max_cold_resistance: cold.max,
        max_lightning_resistance: lightning.max,
        fire_resistance_over_cap: fire.over_cap,
        cold_resistance_over_cap: cold.over_cap,
        lightning_resistance_over_cap: lightning.over_cap,
        crit_chance,
        pre_effective_crit_chance: crit.pre_effective_chance,
        crit_multiplier,
        damage_components,
        total_hit_avg,
        hit_chance,
        action_rate,
        dps,
        stored_crit_avg: crit_pass.stored_crit_avg,
        stored_hit_avg: crit_pass.stored_hit_avg,
        stored_combined_avg: crit_pass.stored_combined_avg,
        stored_ranges: crit_pass.stored_ranges,
        breakdown: vec![
            BreakdownStep {
                name: "life",
                value: life,
            },
            BreakdownStep {
                name: "mana",
                value: mana,
            },
            BreakdownStep {
                name: "fire_resistance",
                value: fire_resistance,
            },
            BreakdownStep {
                name: "cold_resistance",
                value: cold_resistance,
            },
            BreakdownStep {
                name: "lightning_resistance",
                value: lightning_resistance,
            },
            BreakdownStep {
                name: "fire_resistance_over_cap",
                value: fire.over_cap,
            },
            BreakdownStep {
                name: "cold_resistance_over_cap",
                value: cold.over_cap,
            },
            BreakdownStep {
                name: "lightning_resistance_over_cap",
                value: lightning.over_cap,
            },
            BreakdownStep {
                name: "crit_chance",
                value: crit_chance,
            },
            BreakdownStep {
                name: "pre_effective_crit_chance",
                value: crit.pre_effective_chance,
            },
            BreakdownStep {
                name: "crit_multiplier",
                value: crit_multiplier,
            },
            BreakdownStep {
                name: "total_hit_avg",
                value: total_hit_avg,
            },
            BreakdownStep {
                name: "hit_chance",
                value: hit_chance,
            },
            BreakdownStep {
                name: "action_rate",
                value: action_rate,
            },
            BreakdownStep {
                name: "dps",
                value: dps,
            },
        ],
    }
}

/// 旧四参 traced 入口：等价于对**空敌人 modDB** 计算（向后兼容，面板口径下与历史一致）。
///
/// 敌人侧机制（受伤链/抗性护甲减伤/格挡/`CannotEvade`/`SelfCrit*`）需要敌人 modDB，
/// 由 [`calculate_minimal_traced_vs_enemy`] 提供；`perform` 的归因路径应走后者。
pub fn calculate_minimal_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    input: &MinimalInput,
) -> TracedMinimalOutput {
    calculate_minimal_traced_vs_enemy(db, &ModDb::new(), cfg, input)
}

/// 完整 traced 入口：玩家 modDB + 敌人 modDB，与 [`calculate_minimal_vs_enemy`] 同口径。
///
/// 把 `enemy_db` 串进 traced DPS：命中 ×(1-enemy_block)、分类型敌人减伤、暴击降级用
/// 真实敌人 modDB（`resolve_crit_traced`）。敌人侧交互仅在 `cfg.mode_effective == true`
/// 时生效（面板口径与历史 traced 输出一致）。
///
/// 出处：与 [`calculate_minimal_vs_enemy`] 相同（PoB2 `CalcOffence.lua`：`enemyDB:Sum/More
/// DamageTaken`、`enemyBlockChance`、`CannotEvade`、`SelfCrit*`）。
pub fn calculate_minimal_traced_vs_enemy(
    db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    input: &MinimalInput,
) -> TracedMinimalOutput {
    let output = calculate_minimal_vs_enemy(db, enemy_db, cfg, input);
    let mut trace = TraceGraph::new();
    let mut outputs = Vec::new();

    let life = scaled_pool_traced(db, cfg, input.base_life, "MaximumLife", "Life", &mut trace);
    outputs.push(TraceOutput {
        stat: DisplayStatId::from("Life"),
        node_id: life.node_id,
    });

    let mana = scaled_pool_traced(db, cfg, input.base_mana, "MaximumMana", "Mana", &mut trace);
    outputs.push(TraceOutput {
        stat: DisplayStatId::from("Mana"),
        node_id: mana.node_id,
    });

    let fire_resistance = additive_stat_traced(
        db,
        cfg,
        input.base_fire_resistance,
        "FireResistance",
        "FireResist",
        &mut trace,
    );
    outputs.push(TraceOutput {
        stat: DisplayStatId::from("FireResist"),
        node_id: fire_resistance.node_id,
    });

    let cold_resistance = additive_stat_traced(
        db,
        cfg,
        input.base_cold_resistance,
        "ColdResistance",
        "ColdResist",
        &mut trace,
    );
    outputs.push(TraceOutput {
        stat: DisplayStatId::from("ColdResist"),
        node_id: cold_resistance.node_id,
    });

    let lightning_resistance = additive_stat_traced(
        db,
        cfg,
        input.base_lightning_resistance,
        "LightningResistance",
        "LightningResist",
        &mut trace,
    );
    outputs.push(TraceOutput {
        stat: DisplayStatId::from("LightningResist"),
        node_id: lightning_resistance.node_id,
    });

    let total_dps = total_dps_traced(db, enemy_db, cfg, input, &mut trace);
    outputs.push(TraceOutput {
        stat: DisplayStatId::from("TotalDPS"),
        node_id: total_dps.node_id,
    });

    TracedMinimalOutput {
        output,
        trace,
        outputs,
    }
}

/// Builds the `TotalDPS` formula tree, mirroring [`calculate_minimal`]'s DPS
/// pipeline while recording every contributing source into `trace`.
///
/// `TotalDPS final = total_hit_avg * action_rate * hit_chance`, where each
/// factor fans back out to the modifiers and base values that produced it.
///
/// `enemy_db` 串入与 [`calculate_minimal_vs_enemy`] 同口径的敌人交互（仅 `mode_effective`）：
/// 分类型受伤链/抗性/护甲减伤进 `total_hit_avg`、敌方格挡进 `hit_chance`、敌方 `SelfCrit*`
/// 进暴击降级。空 `enemy_db` 等价于历史三参口径（面板口径输出不变）。
fn total_dps_traced(
    db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    input: &MinimalInput,
    trace: &mut TraceGraph,
) -> TracedValue {
    // accuracy & hit chance（提前到暴击之前：mode_effective 暴击降级需命中率）
    let accuracy_names = [ModName::from("Accuracy")];
    let base_accuracy_node = trace.add_source_node(
        "base accuracy",
        input.base_accuracy,
        SourceId::new(SourceKind::CharacterBase, "base.Accuracy"),
    );
    let accuracy_base = db.sum_traced(
        ModType::Base,
        cfg,
        &accuracy_names,
        trace,
        "Accuracy BASE modifier sum",
    );
    let accuracy_inc = db.sum_traced(
        ModType::Inc,
        cfg,
        &accuracy_names,
        trace,
        "Accuracy INC modifier sum",
    );
    let accuracy_more = more_factor_traced(db, cfg, &accuracy_names, "Accuracy MORE factor", trace);
    let accuracy = round(
        (input.base_accuracy + accuracy_base.value)
            * (1.0 + accuracy_inc.value / 100.0)
            * accuracy_more.value,
    );
    let accuracy_node = trace.add_node("accuracy", accuracy, TraceOperation::Multiply);
    trace.add_edge(base_accuracy_node, accuracy_node);
    trace.add_edge(accuracy_base.node_id, accuracy_node);
    trace.add_edge(accuracy_inc.node_id, accuracy_node);
    trace.add_edge(accuracy_more.node_id, accuracy_node);

    let enemy_evasion_node = trace.add_source_node(
        "enemy evasion",
        input.enemy_evasion,
        SourceId::new(SourceKind::EnemyConfig, "enemy.evasion"),
    );
    // PoE2 非攻击必中（vendor :2611）+ 有效口径 CannotEvade（同 calculate_minimal_vs_enemy）。
    let cannot_be_evaded = db.flag(cfg, ModName::from("CannotBeEvaded"))
        || (cfg.mode_effective && enemy_db.flag(cfg, ModName::from("CannotEvade")));
    let accuracy_hit_chance = if !cfg.is_attack() || cannot_be_evaded {
        1.0
    } else {
        hit_chance(input.enemy_evasion, accuracy)
    };
    // 敌方格挡：仅有效口径下从命中里扣（accuracy-and-enemy.md §二.3）。
    let enemy_block = if cfg.mode_effective {
        (enemy_db.sum(ModType::Base, cfg, &[ModName::from("BlockChance")]) / 100.0).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let hit_chance_value = accuracy_hit_chance * (1.0 - enemy_block);
    let hit_chance_node = trace.add_node("hit chance", hit_chance_value, TraceOperation::Chance);
    trace.add_edge(accuracy_node, hit_chance_node);
    trace.add_edge(enemy_evasion_node, hit_chance_node);
    if enemy_block > 0.0 {
        let enemy_block_node = trace.add_source_node(
            "enemy block chance",
            enemy_block,
            SourceId::new(SourceKind::EnemyConfig, "enemy.block"),
        );
        trace.add_edge(enemy_block_node, hit_chance_node);
    }

    // --- crit average factor（resolve_crit_traced：与非 traced 路径同一实现，
    //     BASE/INC/MORE + 敌方 SelfCrit* 全部接入 TraceGraph）。命中降级用 accuracy_hit_chance
    //     （格挡不参与暴击降级，对齐 calculate_minimal_vs_enemy）。
    let (crit, crit_node) = resolve_crit_traced(
        db,
        enemy_db,
        cfg,
        accuracy_hit_chance,
        0.0,
        cfg.mode_effective,
        trace,
    );

    // 伤害主体：暴击/非暴击双腿子图 + CritBlend 合并
    // 数值与非 traced 路径同源（run_crit_passes，含等价性短路）；
    // 图形状 = 每腿独立子图（pass 戳 Single·Crit / Single·NonCrit，per-pass 的
    // sum_traced 各落 Input 节点——RFC §2.4 条款 3）+ CritBlend Combine 节点
    // （pass = Single·Blended，weights = [1−c, c] 冻结系数，§3.3）。
    // TODO(归因面)：DD/TD 词条暂无 Input 节点（direct 缺失、marginal 兜底）。
    // 击中口径 cfg：补 `KeywordFlags::HIT`（与非 traced 路径同源，见该处注释）。
    let hit_cfg = cfg
        .clone()
        .with_keyword_flags(cfg.keyword_flags | KeywordFlags::HIT);
    let scaled = scaled_damage_effect(db, enemy_db, &hit_cfg, crit.chance);
    let crit_pass = run_crit_passes(
        db,
        &hit_cfg,
        input.base_hit_min,
        input.base_hit_max,
        &crit,
        &scaled,
        cfg.mode_effective,
        |pass_cfg, damage_type, raw_hit| {
            enemy_damage_multiplier(db, enemy_db, pass_cfg, damage_type, raw_hit)
        },
    );
    let base_hit_avg = (input.base_hit_min + input.base_hit_max) / 2.0;
    let damage_names = [
        ModName::from("PhysicalDamage"),
        ModName::from("AttackDamage"),
        ModName::from("Damage"),
    ];
    // 非暴击腿子图。
    let cfg_hit = cfg.clone().with_condition("CriticalStrike", false);
    trace.begin_pass(crate::PassId::new(
        crate::HandTag::Single,
        crate::CritTag::NonCrit,
    ));
    let non_crit_total: f64 = crit_pass.stored_hit_avg.iter().map(|(_, avg)| avg).sum();
    let non_crit_node = {
        let damage_cfg = cfg_hit.clone().with_damage_type(DamageType::Physical);
        let inc_damage = db.sum_traced(
            ModType::Inc,
            &damage_cfg,
            &damage_names,
            trace,
            "Damage INC modifier sum (non-crit pass)",
        );
        let more_damage = more_factor_traced(
            db,
            &damage_cfg,
            &damage_names,
            "Damage MORE factor (non-crit pass)",
            trace,
        );
        let base_hit_node = trace.add_source_node(
            "base hit average (non-crit pass)",
            base_hit_avg,
            SourceId::new(SourceKind::CharacterBase, "base.Hit"),
        );
        let node = trace.add_node(
            "non-crit hit average (all damage types)",
            non_crit_total,
            TraceOperation::Multiply,
        );
        trace.add_edge(base_hit_node, node);
        trace.add_edge(inc_damage.node_id, node);
        trace.add_edge(more_damage.node_id, node);
        node
    };
    trace.end_pass();
    // 暴击腿子图（聚合条件 CriticalStrike=true；值含 ×CritMultiplier，
    // 暴击词条来源经 crit_node 入边可达）。
    let cfg_crit = cfg.clone().with_condition("CriticalStrike", true);
    trace.begin_pass(crate::PassId::new(
        crate::HandTag::Single,
        crate::CritTag::Crit,
    ));
    let crit_total: f64 = crit_pass.stored_crit_avg.iter().map(|(_, avg)| avg).sum();
    let crit_leg_node = {
        let damage_cfg = cfg_crit.clone().with_damage_type(DamageType::Physical);
        let inc_damage = db.sum_traced(
            ModType::Inc,
            &damage_cfg,
            &damage_names,
            trace,
            "Damage INC modifier sum (crit pass)",
        );
        let more_damage = more_factor_traced(
            db,
            &damage_cfg,
            &damage_names,
            "Damage MORE factor (crit pass)",
            trace,
        );
        let base_hit_node = trace.add_source_node(
            "base hit average (crit pass)",
            base_hit_avg,
            SourceId::new(SourceKind::CharacterBase, "base.Hit"),
        );
        let node = trace.add_node(
            "crit hit average (all damage types, x crit multiplier)",
            crit_total,
            TraceOperation::Multiply,
        );
        trace.add_edge(base_hit_node, node);
        trace.add_edge(inc_damage.node_id, node);
        trace.add_edge(more_damage.node_id, node);
        node
    };
    trace.end_pass();
    // crit_node（暴击几率/爆伤来源）连入暴击腿：爆伤只放大该腿（vendor :4028-4032）。
    trace.add_edge(crit_node, crit_leg_node);
    // CritBlend 合并节点（属本腿子图的 Blended 层，RFC §2.3）。
    trace.begin_pass(crate::PassId::hand_blended(crate::HandTag::Single));
    let c = crit.chance;
    let blend_node = trace.add_combine_node(
        "AverageHit crit blend",
        crit_pass.total_hit_avg,
        crate::CombineMode::CritBlend,
        &[(non_crit_node, 1.0 - c), (crit_leg_node, c)],
    );
    trace.end_pass();

    // total_hit_avg（DPS 用）：有效口径下含敌人受伤链/抗性/护甲减伤的总击中。
    let total_hit_avg = crit_pass.total_hit_avg_mitigated;
    let total_hit_node = trace.add_node(
        "total hit average (after enemy mitigation)",
        total_hit_avg,
        TraceOperation::Mitigate,
    );
    trace.add_edge(blend_node, total_hit_node);

    // action rate
    // 速度族（攻击取 AttackSpeed / 法术取 CastSpeed，SkillSpeed 始终）一个 inc/more 乘区；
    // ActionSpeed 独立乘区单独相乘；末端按固有冷却限速（min(rate, 1/effective_cooldown)）——
    // 对齐非 traced 路径。
    let speed_names = super::skill_use_time::speed_names_for(cfg);
    let action_speed_names = [ModName::from(super::skill_use_time::ACTION_SPEED)];
    let base_rate_node = trace.add_source_node(
        "base action rate",
        input.base_action_rate,
        SourceId::new(SourceKind::CharacterBase, "base.ActionRate"),
    );
    let inc_speed = db.sum_traced(
        ModType::Inc,
        cfg,
        &speed_names,
        trace,
        "Speed INC modifier sum (Attack/Cast/Skill)",
    );
    let more_speed = more_factor_traced(db, cfg, &speed_names, "Speed MORE factor", trace);
    let action_speed_mod = (1.0 + db.sum(ModType::Inc, cfg, &action_speed_names) / 100.0)
        * db.more(cfg, &action_speed_names);
    let scaled_rate = apply_total_time(
        db,
        cfg,
        input.base_action_rate * (1.0 + inc_speed.value / 100.0) * more_speed.value,
    );
    let uncapped_rate = scaled_rate * action_speed_mod;
    let action_rate = round(apply_server_tick_cap(
        db,
        cfg,
        apply_cooldown_cap(db, cfg, uncapped_rate),
    ));
    let action_rate_node = trace.add_node("action rate", action_rate, TraceOperation::Multiply);
    trace.add_edge(base_rate_node, action_rate_node);
    trace.add_edge(inc_speed.node_id, action_rate_node);
    trace.add_edge(more_speed.node_id, action_rate_node);

    // TotalDPS final
    let end = dps_end_factors(db, cfg, None);
    let end_factor = end.dps_multiplier * end.quantity_multiplier;
    let dps = round(total_hit_avg * action_rate * hit_chance_value * end_factor);
    let dps_node = trace.add_node("TotalDPS final", dps, TraceOperation::Multiply);
    trace.add_edge(total_hit_node, dps_node);
    trace.add_edge(action_rate_node, dps_node);
    trace.add_edge(hit_chance_node, dps_node);
    if end_factor != 1.0 {
        // QuantityMultiplier 词条来源进图（dpsMultiplier 技能数据侧 T4 透传后补）。
        let quantity = db.sum_traced(
            ModType::Base,
            cfg,
            &[ModName::from("QuantityMultiplier")],
            trace,
            "QuantityMultiplier BASE sum",
        );
        let end_node = trace.add_node(
            "DPS end factors (dpsMultiplier x quantityMultiplier)",
            end_factor,
            TraceOperation::Multiply,
        );
        trace.add_edge(quantity.node_id, end_node);
        trace.add_edge(end_node, dps_node);
    }

    TracedValue {
        value: dps,
        node_id: dps_node,
    }
}

/// 敌人侧对某伤害类型的**受到伤害**总乘子（有效口径）：
///
/// `mult = (1 + Σ DamageTaken_inc/100) × Π DamageTaken_more × (1 - effective_resist_frac)`
///
/// 组成（受伤链 / 抗性 / 护甲读 `enemy_db` 归因 `EnemyConfig`；穿透 / Overwhelm 读
/// **玩家** `player_db` 归因玩家来源，doc12 §4.2、damage-scaling.md §Overwhelm/Penetration）：
/// - **受伤链**：`DamageTaken` 通用 + `<Type>DamageTaken` 分类型（感电/Intimidate/凋萎/Uber 等）。
///   通过把 `cfg.damage_type` 设为该类型，使带 `DamageType` tag 的 `DamageTaken` modifier 命中。
/// - **抗性减伤（元素/混沌）**：`<Type>Resist BASE`（含曝光/降抗诅咒/Boss 加成）求和，
///   clamp 到 `[RESIST_FLOOR, ENEMY_MAX_RESIST]`；再扣**玩家穿透**：
///   `effective_resist = if resist > 0 { max(resist - pen, 0) } else { resist }`
///   （PoB2 `m_max(resist - pen, minPen)`，minPen=0：穿透不破 0、负抗不被穿透）。
///   减伤 = `(1 - effective_resist/100)`。物理无抗性穿透。
/// - **护甲减伤 / Overwhelm（物理）**：见 [`enemy_physical_multiplier`]。
///
/// `raw_hit` 用该分量的（未减伤）平均击中近似（PoB2 用每次击中量；面板近似足够），
/// 仅物理护甲减伤需要它。
fn enemy_damage_multiplier(
    player_db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    damage_type: DamageType,
    raw_hit: f64,
) -> f64 {
    let type_prefix = match damage_type {
        DamageType::Physical => "Physical",
        DamageType::Fire => "Fire",
        DamageType::Cold => "Cold",
        DamageType::Lightning => "Lightning",
        DamageType::Chaos => "Chaos",
    };
    let type_cfg = cfg.clone().with_damage_type(damage_type);

    // 受伤链：通用 + 分类型 DamageTaken（INC + MORE）
    let taken_names = [
        ModName::from("DamageTaken"),
        ModName::from(format!("{type_prefix}DamageTaken")),
    ];
    let mut taken_inc = enemy_db.sum(ModType::Inc, &type_cfg, &taken_names);
    // INC-only 追加名（vendor 只加进 takenInc，不进 takenMore）：
    // - 元素类型 += ElementalDamageTaken（CalcOffence.lua:4141）；
    // - 投射物技能 += ProjectileDamageTaken（:4152-4153）、攻击投射物再加
    //   ProjectileAttackDamageTaken（:4155-4156）——PoBR 以 cfg 的
    //   ModFlags::PROJECTILE / 攻击判定近似 vendor skillFlags.projectile/attack；
    // - trap/mine += TrapMineDamageTaken（:4158-4159）——（h3 登记）接线：
    //   以 `cfg.skill_types` 含 Trapped(33)/RemoteMined(36) 近似 vendor
    //   skillFlags.trap/mine（statSet.baseFlags 主通道；support addFlags
    //   授予通道（如 Remote Mine support 加 'mine'）PoBR 未建模，保持登记）。
    if damage_type.is_elemental() {
        taken_inc += enemy_db.sum(
            ModType::Inc,
            &type_cfg,
            &[ModName::from("ElementalDamageTaken")],
        );
    }
    if type_cfg.flags.intersects(ModFlags::PROJECTILE) {
        taken_inc += enemy_db.sum(
            ModType::Inc,
            &type_cfg,
            &[ModName::from("ProjectileDamageTaken")],
        );
        if type_cfg.is_attack() {
            taken_inc += enemy_db.sum(
                ModType::Inc,
                &type_cfg,
                &[ModName::from("ProjectileAttackDamageTaken")],
            );
        }
    }
    if type_cfg
        .skill_types
        .intersects(SkillTypes::TRAPPED | SkillTypes::REMOTE_MINED)
    {
        taken_inc += enemy_db.sum(
            ModType::Inc,
            &type_cfg,
            &[ModName::from("TrapMineDamageTaken")],
        );
    }
    let taken_more = enemy_db.more(&type_cfg, &taken_names);
    let taken_mult = (1.0 + taken_inc / 100.0) * taken_more;

    // 抗性减伤（元素/混沌，含玩家穿透）/ 护甲减伤 + Overwhelm（物理）
    let mitigation = if damage_type == DamageType::Physical {
        enemy_physical_multiplier(player_db, enemy_db, &type_cfg, raw_hit)
    } else {
        let mut resist = enemy_resist_final(enemy_db, &type_cfg, damage_type);
        // 击中视敌元素抗性为反转（Rakiata's Flow 等，vendor
        // CalcOffence.lua:4145-4148）：`invertChance = clamp(Sum(CHANCE,
        // "HitsInvertEleResChance"), 0, 1)`，仅三元素；
        // `resist = (1-c)*resist + c*(-resist) = resist - 2*c*resist`。
        // 在抗性 clamp 之后、穿透之前应用（vendor 同序：:4135 pen 取数、
        // :4145 反转、:4163 effMult 内扣 pen）。
        if damage_type.is_elemental() {
            let invert = player_db
                .sum(
                    ModType::Base,
                    &type_cfg,
                    &[ModName::from("HitsInvertEleResChance")],
                )
                .clamp(0.0, 1.0);
            if invert > 0.0 {
                resist -= 2.0 * invert * resist;
            }
        }
        let effective_resist = apply_penetration(player_db, &type_cfg, damage_type, resist);
        // 诊断：POBR_DBG_ENEMYMIT=1 时逐类型 dump 敌方减伤分解（与 oracle
        // enemyMitigation 对照：resistBase/pen/takenInc/takenMore）。
        if dbg_env!("POBR_DBG_ENEMYMIT").is_some() {
            eprintln!(
                "[POBR_ENEMYMIT] {type_prefix}: resist={resist:.2} eff_resist={effective_resist:.2} taken_inc={taken_inc:.2} taken_more={taken_more:.4}"
            );
            for m in enemy_db.iter_mods() {
                let n = m.name.as_str();
                if n == format!("{type_prefix}Resist") || n == "ElementalResist" {
                    eprintln!(
                        "[POBR_ENEMYMIT]   {n} {:?} {:?} origin={:?} tags={:?}",
                        m.mod_type, m.value, m.origin, m.tags
                    );
                }
            }
        }
        1.0 - effective_resist / 100.0
    };

    taken_mult * mitigation
}

/// 敌人对某伤害类型的 **final 抗性**（vendor `calcResistForType`，CalcOffence.lua:530-543）：
///
/// 1. `enemyDB:Override(cfg, "<Type>Resist")` 优先（config「视为 0 抗」类覆盖）；
/// 2. 否则 `Σ BASE(<Type>Resist[, ElementalResist])`（元素类型含共享名
///    `ElementalResist`，vendor :539）× `max((1 + ΣINC/100) × ΠMORE, 0)`
///    （抗性自身的 INC/MORE 缩放，`calcLib.mod` 同式、负缩放 floor 0）；
/// 3. clamp 到 `[ResistFloor(−200), maxResist]`（Data.lua:180/:200）。
///
/// maxResist（vendor :532）：基线 `EnemyMaxResist(75)`；configInput
/// `enemy<Type>Resist` **显式输入**时抬到 `min(max(输入, 75), MaxResistCap(90))`
/// ——pobr 等价取数 = enemy db 中归因 `(EnemyConfig, "config.enemy<Type>Resist")`
/// 的 BASE 条目（`config_resolve` 显式数值的唯一注入形态；档位预设/曝光等其余
/// EnemyConfig 来源 id 不同名，不参与）。`DoNotChangeMaxResFromConfig` FLAG
/// （config「Enemy Max Resistance is always 75%」，ConfigOptions.lua:2158-2159）
/// 置位时恒 75。物理不走本函数（护甲/PDR 路径见 [`enemy_physical_multiplier`]）。
pub(crate) fn enemy_resist_final(
    enemy_db: &ModDb,
    type_cfg: &CalcConfig,
    damage_type: DamageType,
) -> f64 {
    debug_assert!(damage_type != DamageType::Physical, "物理无抗性路径");
    let type_prefix = match damage_type {
        DamageType::Physical => "Physical",
        DamageType::Fire => "Fire",
        DamageType::Cold => "Cold",
        DamageType::Lightning => "Lightning",
        DamageType::Chaos => "Chaos",
    };
    let resist_name = ModName::from(format!("{type_prefix}Resist"));
    let max_resist = enemy_max_resist_for(enemy_db, type_cfg, type_prefix, &resist_name);
    let resist = match enemy_db.override_(type_cfg, resist_name.clone()) {
        Some(value) => value,
        None => {
            // 元素类型共享 `ElementalResist` 名（vendor isElemental 三元素；混沌不含）。
            let names: &[ModName] = &if damage_type.is_elemental() {
                vec![resist_name, ModName::from("ElementalResist")]
            } else {
                vec![resist_name]
            };
            let base = enemy_db.sum(ModType::Base, type_cfg, names);
            let scale = (1.0 + enemy_db.sum(ModType::Inc, type_cfg, names) / 100.0)
                * enemy_db.more(type_cfg, names);
            base * scale.max(0.0)
        }
    };
    resist.clamp(type_cfg.constants.game().resist_floor, max_resist)
}

/// 该类型抗性的 clamp 上限（vendor CalcOffence.lua:532）：
///
/// ```text
/// maxResist = Flag(DoNotChangeMaxResFromConfig) and EnemyMaxResist
///     or min(max(configInput["enemy<Type>Resist"] or EnemyMaxResist, EnemyMaxResist), MaxResistCap)
/// ```
///
/// configInput 等价取数 = enemy db 中 BASE `<Type>Resist` 且归因
/// `(EnemyConfig, "config.enemy<Type>Resist")` 的条目（config_resolve 显式数值
/// 注入形态；多条时与 BASE 聚合同口径求和）。`MaxResistCap(90)` = 注入常量
/// `resist_hard_cap`（Data.lua:181）。
fn enemy_max_resist_for(
    enemy_db: &ModDb,
    type_cfg: &CalcConfig,
    type_prefix: &str,
    resist_name: &ModName,
) -> f64 {
    if enemy_db.flag(type_cfg, ModName::from("DoNotChangeMaxResFromConfig")) {
        return ENEMY_MAX_RESIST;
    }
    let config_source_id = format!("config.enemy{type_prefix}Resist");
    let config_input: Option<f64> = enemy_db
        .iter_mods()
        .filter(|m| {
            m.mod_type == ModType::Base
                && m.name == *resist_name
                && m.origin.as_ref().is_some_and(|o| {
                    o.source_id.kind == SourceKind::EnemyConfig
                        && o.source_id.id == config_source_id
                })
        })
        .map(|m| m.value.as_number().unwrap_or(0.0))
        .fold(None, |acc, v| Some(acc.unwrap_or(0.0) + v));
    match config_input {
        Some(input) => input
            .max(ENEMY_MAX_RESIST)
            .min(type_cfg.constants.game().resist_hard_cap),
        None => ENEMY_MAX_RESIST,
    }
}

/// 玩家穿透对**已 clamp 的**敌人抗性的下调（仅元素/混沌、仅击中）。
///
/// 读玩家 db：元素 `<Type>Penetration` + 共享 `ElementalPenetration`；混沌 `ChaosPenetration`。
/// 公式（PoB2 CalcOffence.lua:4163）：
/// `effective = if resist > minPen { max(resist - pen, minPen) } else { resist }`
/// —— `minPen = Σ BASE(<El>PenetrationMinimum, ElementalPenetrationMinimum)`
/// （vendor :4140/:4144，「穿透至多压到 N」类词条；混沌无 minimum 名、恒 0）。
/// 无 minimum 词条时退化为旧式：穿透只在抗性为正时生效、不能把抗性压到 0 以下；
/// 抗性已 ≤ minPen（含负抗）时穿透全浪费。
///
/// 出处：agent-docs/damage-scaling.md §Penetration（穿透不破 0、与负抗互斥、仅击中）；
///       damage-defence-order.md §步骤 4；PoB2 `<Type>Penetration`/`ElementalPenetration`。
fn apply_penetration(
    player_db: &ModDb,
    type_cfg: &CalcConfig,
    damage_type: DamageType,
    resist: f64,
) -> f64 {
    let pen = penetration_value(player_db, type_cfg, damage_type);
    let min_pen = penetration_minimum(player_db, type_cfg, damage_type);
    if resist > min_pen {
        (resist - pen).max(min_pen)
    } else {
        resist
    }
}

/// 穿透下界 `minPen`（vendor CalcOffence.lua:4140/:4144：
/// `Sum("BASE", cfg, <El>PenetrationMinimum, ElementalPenetrationMinimum)`）。
/// 仅三元素有 minimum 名空间；混沌/物理恒 0。
fn penetration_minimum(player_db: &ModDb, type_cfg: &CalcConfig, damage_type: DamageType) -> f64 {
    let names: &[ModName] = &match damage_type {
        DamageType::Physical | DamageType::Chaos => return 0.0,
        DamageType::Fire => vec![
            ModName::from("FirePenetrationMinimum"),
            ModName::from("ElementalPenetrationMinimum"),
        ],
        DamageType::Cold => vec![
            ModName::from("ColdPenetrationMinimum"),
            ModName::from("ElementalPenetrationMinimum"),
        ],
        DamageType::Lightning => vec![
            ModName::from("LightningPenetrationMinimum"),
            ModName::from("ElementalPenetrationMinimum"),
        ],
    };
    player_db.sum(ModType::Base, type_cfg, names)
}

/// 玩家对某伤害类型的穿透值（%）。物理无穿透（物理走 Overwhelm/护甲破坏路径）。
fn penetration_value(player_db: &ModDb, type_cfg: &CalcConfig, damage_type: DamageType) -> f64 {
    let names: &[ModName] = &match damage_type {
        DamageType::Physical => return 0.0,
        DamageType::Fire => vec![
            ModName::from("FirePenetration"),
            ModName::from("ElementalPenetration"),
        ],
        DamageType::Cold => vec![
            ModName::from("ColdPenetration"),
            ModName::from("ElementalPenetration"),
        ],
        DamageType::Lightning => vec![
            ModName::from("LightningPenetration"),
            ModName::from("ElementalPenetration"),
        ],
        DamageType::Chaos => vec![ModName::from("ChaosPenetration")],
    };
    player_db.sum(ModType::Base, type_cfg, names)
}

/// 物理减伤分量（对某 raw_hit），vendor CalcOffence.lua:4074-4096 physical 段：
///
/// ```text
/// resist = clamp(  enemyDB:Sum(BASE, PhysicalDamageReduction)        -- 敌固定 PDR
///                + skillModList:Sum(BASE, EnemyPhysicalDamageReduction) -- 玩家 Overwhelm（负）
///                + armourReduction(enemyArmour, raw_hit × More(CalcArmourAsThoughDealing)),
///                  −NegArmourDmgBonusCap, EnemyPhysicalDamageReductionCap )  -- [−100, 75]
/// ```
///
/// 三项**相加**（vendor :4095，非乘法并集）；下界 −100（Data.lua:194
/// NegArmourDmgBonusCap——破甲到负后的增伤上限 +100%），上界 75
/// （monsterConstants `maximum_physical_damage_reduction_%`）。
///
/// - 敌甲取值（:4080-4081）：`Override(Armour)` 优先，否则
///   `calcLib.val = Σ BASE × (1 + ΣINC/100) × ΠMORE`；
/// - 玩家 `IgnoreEnemyArmour` flag（:4084-4085）→ 敌甲按 0 计
///   （正甲全免；vendor 对负甲不剔除，此处同样仅在 armour > 0 时生效）；
/// - `CalcArmourAsThoughDealing` MORE（:4087）：以放大后的击中量算护甲减免；
/// - 负甲（破甲过零）走 [`armour_reduction_pct_signed`] 的负分支（增伤）。
///
/// 未接（vendor 有、PoBR 当前无 producer，TODO(parity)）：`IgnoreArmour` 数值削减
/// （:4084）、`ChanceToIgnoreEnemyArmour`（:4082/:4087）、
/// `ChanceToIgnoreEnemyPhysicalDamageReduction` + MIN/MAX config 模式（:4088-4094）、
/// `PartialIgnoreEnemyPhysicalDamageReduction`（:4096）。
///
/// 出处：agent-docs/damage-scaling.md §Overwhelm；PoB2 CalcOffence.lua:4074-4096。
fn enemy_physical_multiplier(
    player_db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    raw_hit: f64,
) -> f64 {
    let armour_names = [ModName::from("Armour")];
    let mut armour = match enemy_db.override_(cfg, ModName::from("Armour")) {
        Some(value) => value,
        None => {
            enemy_db.sum(ModType::Base, cfg, &armour_names)
                * (1.0 + enemy_db.sum(ModType::Inc, cfg, &armour_names) / 100.0)
                * enemy_db.more(cfg, &armour_names)
        }
    };
    if armour > 0.0 && player_db.flag(cfg, ModName::from("IgnoreEnemyArmour")) {
        armour = 0.0;
    }
    let as_though_dealing = player_db.more(cfg, &[ModName::from("CalcArmourAsThoughDealing")]);
    let from_armour = armour_reduction_pct_signed(
        armour,
        raw_hit * as_though_dealing,
        cfg.constants.game().armour_ratio,
    );
    let flat_pdr = enemy_db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("PhysicalDamageReduction")],
    );
    // Overwhelm：玩家 EnemyPhysicalDamageReduction BASE（通常为负）直接加到敌人 PDR 上。
    let overwhelm = player_db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("EnemyPhysicalDamageReduction")],
    );
    let reduction = (flat_pdr + overwhelm + from_armour).clamp(
        -cfg.constants.game().neg_armour_dmg_bonus_cap,
        ENEMY_PHYS_DMGRED_CAP,
    );
    1.0 - reduction / 100.0
}

/// 护甲减伤（%，带符号）——vendor `calcs.armourReductionF`（CalcDefence.lua:55-64）：
/// `armour/(armour + raw × ArmourRatio) × 100`；armour < 0（破甲过零）取
/// `−(|armour|/(|armour| + raw × ratio) × 100)`（负减伤 = 增伤）；armour 与 raw
/// 均为 0 → 0。与玩家侧 [`armour_reduction`](super::armour_reduction)（fraction、
/// 负甲归 0）口径不同——敌甲路径需要负分支。
fn armour_reduction_pct_signed(armour: f64, raw_hit: f64, armour_ratio: f64) -> f64 {
    if armour == 0.0 || raw_hit <= 0.0 {
        return 0.0;
    }
    let magnitude = armour.abs();
    let pct = magnitude / (magnitude + raw_hit * armour_ratio) * 100.0;
    if armour < 0.0 { -pct } else { pct }
}

/// Records a MORE aggregation (`Π(1 + v/100)`) as a single trace node fed by one
/// source node per contributing modifier, mirroring [`ModDb::more`].
pub(crate) fn more_factor_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    names: &[ModName],
    label: impl Into<String>,
    trace: &mut TraceGraph,
) -> TracedValue {
    let contributions = db.contributions(ModType::More, cfg, names);
    let factor = contributions.iter().fold(1.0, |product, contribution| {
        product * (1.0 + contribution.value / 100.0)
    });
    let factor_node = trace.add_node(label, factor, TraceOperation::MoreProduct);

    for contribution in contributions {
        let source = contribution
            .origin
            .as_ref()
            .map(|origin| origin.source_id.clone())
            .unwrap_or_else(|| {
                SourceId::new(
                    SourceKind::Derived,
                    format!(
                        "{}.{}",
                        contribution.name,
                        contribution.mod_type.as_trace_label()
                    ),
                )
            });
        let input_label = contribution
            .raw_text
            .clone()
            .unwrap_or_else(|| format!("{} MORE {}", contribution.name, contribution.value));
        let input_node = trace.add_source_node(input_label, contribution.value, source);
        trace.add_edge(input_node, factor_node);
    }

    TracedValue {
        value: factor,
        node_id: factor_node,
    }
}

/// 附加施放/攻击时间（PoB2 `TotalCastTime` / `TotalAttackTime`，单位秒）：在速度 inc/more
/// **缩放之后**作为**加法项**计入有效时间分母（CalcOffence L2827：
/// `Speed = 1 / (baseTime / ((1+inc/100)*more) + TotalAttackTime + TotalCastTime)`）。
///
/// 这类常量来自技能 statSet 的 `total_cast_time_+_ms` / `total_attack_time_+_ms`
/// constantStat（如 Comet +1000ms = +1.0s），由 statmap 数据引擎（`crate::rules::stat_map_engine`）映射为
/// `TotalCastTime`/`TotalAttackTime` BASE 注入。无此词条时返回原速率（加法项为 0）。
///
/// `scaled_rate` 为已应用速度 inc/more（但**未应用** ActionSpeed）的速率；本函数把它转回
/// 时间、加上附加时间、再转回速率。ActionSpeed 由调用方在本函数之后单独乘上（对齐 PoB：
/// action speed 是独立乘区，作用于含附加时间的最终速率）。
fn apply_total_time(db: &ModDb, cfg: &CalcConfig, scaled_rate: f64) -> f64 {
    if scaled_rate <= 0.0 {
        return scaled_rate;
    }
    // 同时取 TotalCastTime + TotalAttackTime：PoB 按技能只注入其一（法术=cast、攻击=attack），
    // 实际每技能仅一项非零，故求和等价于取相关项。**有意不按 cfg.is_spell() 门控**——主技能的
    // SPELL/ATTACK flag 派生（skill_type_flags）对部分 build 并不可靠，门控会让 comet 等丢失
    // TotalCastTime 限速（实测倒退进攻 parity）；求和则稳健。
    let extra_time = db.sum(
        ModType::Base,
        cfg,
        &[
            ModName::from("TotalCastTime"),
            ModName::from("TotalAttackTime"),
        ],
    );
    if extra_time <= 0.0 {
        return scaled_rate;
    }
    let effective_time = 1.0 / scaled_rate + extra_time;
    1.0 / effective_time
}

/// 冷却限速：技能有固有冷却时，最终行动速率不能超过 `1/effective_cooldown`。
///
/// PoB 顺序：**先把速度全部 inc/more 算完**，再 `min(rate, 1/cooldown)`——所以本函数在
/// 速度链路末端调用（不在装配阶段预截 base_action_rate）。`effective_cooldown` 经
/// `CooldownRecovery`（INC/MORE，[`calc_cooldown`]）缩短：`base_cd / (1+Σinc/100)/Πmore`。
///
/// 例外：「绕过冷却」技能（如 Flicker Strike，消耗充能重置冷却）注入 `CooldownBypass` flag，
/// 此时不限速、按攻速出手。无 `SkillCooldownBase` 词条（base_cd≤0）时也不限速。
///
/// `pub(crate)`：perform 的 fill 阶段（`effective_action_rate`，ailment/reload 消费）
/// 与 offence 主链共用同一冷却 cap（整链单一来源）。
pub(crate) fn apply_cooldown_cap(db: &ModDb, cfg: &CalcConfig, uncapped_rate: f64) -> f64 {
    if db.flag(cfg, ModName::from("CooldownBypass")) {
        return uncapped_rate;
    }
    let base_cd = db.sum(ModType::Base, cfg, &[ModName::from("SkillCooldownBase")]);
    if base_cd <= 0.0 {
        return uncapped_rate;
    }
    // 储存次数（grenade=3 等）：>1 时冷却不向上取整到服务器帧（PoB2 CalcOffence
    // L338-345），与 perform::fill_skill_mechanics 同源读 SkillStoredUsesBase。
    let stored = db
        .sum(ModType::Base, cfg, &[ModName::from("SkillStoredUsesBase")])
        .max(0.0) as u32;
    let cd = super::skill_mechanics::calc_cooldown(db, cfg, base_cd, stored).cooldown;
    if cd <= 0.0 {
        return uncapped_rate;
    }
    // PoB2 CalcOffence L2855：冷却 cap 同样乘 Repeats（多重打击/技能重复，默认 1）。
    uncapped_rate.min(repeats(db, cfg) / cd)
}

/// 技能重复次数 Repeats（PoB2 CalcOffence L981：`1 + RepeatCount`，默认 1）。
/// multistrike / 技能重复词条注入 BASE `RepeatCount` 后此值 >1；当前未接线时恒为 1。
fn repeats(db: &ModDb, cfg: &CalcConfig) -> f64 {
    1.0 + db
        .sum(ModType::Base, cfg, &[ModName::from("RepeatCount")])
        .max(0.0)
}

/// 服务器帧速率上限（PoB2 CalcOffence L2863-2865）：非引导技能的最终行动速率不能超过
/// `ServerTickRate × Repeats`（ServerTickRate = 1/0.033 ≈ 30.3 actions/s）。引导技能
/// （`Channelling` 条件）不受此限。在冷却 cap 之后施加，与 PoB2 顺序一致。
fn apply_server_tick_cap(db: &ModDb, cfg: &CalcConfig, rate: f64) -> f64 {
    if cfg.condition("Channelling") {
        return rate;
    }
    let server_cap = (1.0 / cfg.constants.game().server_tick_seconds) * repeats(db, cfg);
    rate.min(server_cap)
}

pub(crate) fn scaled_pool(db: &ModDb, cfg: &CalcConfig, base: f64, name: &str) -> f64 {
    let names = [ModName::from(name)];
    let conv = pool_conversion_pct(db, cfg, name);
    if conv == 0.0 {
        return scaled_numeric_stat(db, cfg, base, &names);
    }
    // vendor CalcDefence.lua:92-95：`(base × (1 − conv/100) + extra) × (1+inc) × more`。
    // OVERRIDE 仍然胜过一切（ChaosInoculation 等池钳定）。
    for n in &names {
        if let Some(value) = db.override_(cfg, n.clone()) {
            return round(value);
        }
    }
    let base_value = base + db.sum(ModType::Base, cfg, &names);
    let inc = db.sum(ModType::Inc, cfg, &names);
    let more = db.more(cfg, &names);
    round(base_value * (1.0 - conv / 100.0) * (1.0 + inc / 100.0) * more)
}

/// Life/Mana 池的「N% of Maximum X Converted to <defence>」扣减率
/// （vendor CalcDefence.lua:92 `conv = m_min(Sum(BASE, res.."ConvertTo…"), 100)`）。
/// 只扣池本体；ES/Armour/Evasion 侧的**转入**由 defence 矩阵按未扣减的全局底
/// 处理（:1364 `ceil(globalBase × rate/100)`，见 calc_defence_resources）。
// ponytail: vendor 把 conv 只作用于 base 段、Extra<res> 免扣——PoBR 的矩阵转入
// 现注入为 Maximum<res> BASE，会一并被扣；fixture 集无「双向转换」build，出现时
// 再把注入名迁到 Extra<res> 通道。
fn pool_conversion_pct(db: &ModDb, cfg: &CalcConfig, name: &str) -> f64 {
    let prefix = match name {
        "MaximumLife" => "Life",
        "MaximumMana" => "Mana",
        _ => return 0.0,
    };
    db.sum(
        ModType::Base,
        cfg,
        &[
            ModName::from(format!("{prefix}ConvertToEnergyShield")),
            ModName::from(format!("{prefix}ConvertToArmour")),
            ModName::from(format!("{prefix}ConvertToEvasion")),
        ],
    )
    .min(100.0)
}

fn scaled_numeric_stat(db: &ModDb, cfg: &CalcConfig, base: f64, names: &[ModName]) -> f64 {
    // OVERRIDE 胜过 base/inc/more（PoB2 语义：关键石如 Chaos Inoculation「Maximum Life is 1」、
    // Blood Magic「You have no Mana」直接钳定池值）。后写覆盖先写，取首个匹配的 override。
    for name in names {
        if let Some(value) = db.override_(cfg, name.clone()) {
            return round(value);
        }
    }
    let base_value = base + db.sum(ModType::Base, cfg, names);
    let inc = db.sum(ModType::Inc, cfg, names);
    let more = db.more(cfg, names);
    round(base_value * (1.0 + inc / 100.0) * more)
}

fn scaled_pool_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    base: f64,
    stat_name: &str,
    output_label: &str,
    trace: &mut TraceGraph,
) -> TracedValue {
    let names = [ModName::from(stat_name)];
    // OVERRIDE 胜过 base/inc/more（PoB2 关键石池钳定语义，见 scaled_numeric_stat）。
    let (override_value, override_node) = db.override_traced(
        cfg,
        ModName::from(stat_name),
        trace,
        format!("{stat_name} OVERRIDE"),
    );
    if let Some(value) = override_value {
        let final_value = round(value);
        let final_node = trace.add_node(
            format!("{output_label} final"),
            final_value,
            TraceOperation::QueryOverride,
        );
        trace.add_edge(override_node, final_node);
        return TracedValue {
            value: final_value,
            node_id: final_node,
        };
    }
    let base_node = trace.add_source_node(
        format!("base {stat_name}"),
        base,
        SourceId::new(SourceKind::CharacterBase, format!("base.{stat_name}")),
    );
    let base_mods = db.sum_traced(
        ModType::Base,
        cfg,
        &names,
        trace,
        format!("{stat_name} BASE modifier sum"),
    );
    // Life/Mana 池转换扣减（vendor :92——与 scaled_pool 非追踪路径同式）。
    let conv_factor = 1.0 - pool_conversion_pct(db, cfg, stat_name) / 100.0;
    let base_total = (base + base_mods.value) * conv_factor;
    let base_total_node = trace.add_node(
        format!("{stat_name} base total"),
        base_total,
        TraceOperation::Add,
    );
    trace.add_edge(base_node, base_total_node);
    trace.add_edge(base_mods.node_id, base_total_node);

    let inc_mods = db.sum_traced(
        ModType::Inc,
        cfg,
        &names,
        trace,
        format!("{stat_name} INC modifier sum"),
    );
    let more_factor = db.more(cfg, &names);
    let more_node = trace.add_node(
        format!("{stat_name} MORE factor"),
        more_factor,
        TraceOperation::QueryMore,
    );
    let final_value = round(base_total * (1.0 + inc_mods.value / 100.0) * more_factor);
    let final_node = trace.add_node(
        format!("{output_label} final"),
        final_value,
        TraceOperation::Multiply,
    );
    trace.add_edge(base_total_node, final_node);
    trace.add_edge(inc_mods.node_id, final_node);
    trace.add_edge(more_node, final_node);

    TracedValue {
        value: final_value,
        node_id: final_node,
    }
}

fn additive_stat_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    base: f64,
    stat_name: &str,
    output_label: &str,
    trace: &mut TraceGraph,
) -> TracedValue {
    let names = [ModName::from(stat_name)];
    let base_node = trace.add_source_node(
        format!("base {stat_name}"),
        base,
        SourceId::new(SourceKind::CharacterBase, format!("base.{stat_name}")),
    );
    let base_mods = db.sum_traced(
        ModType::Base,
        cfg,
        &names,
        trace,
        format!("{stat_name} BASE modifier sum"),
    );
    let final_value = round(base + base_mods.value);
    let final_node = trace.add_node(
        format!("{output_label} final"),
        final_value,
        TraceOperation::Add,
    );
    trace.add_edge(base_node, final_node);
    trace.add_edge(base_mods.node_id, final_node);

    TracedValue {
        value: final_value,
        node_id: final_node,
    }
}

#[cfg(test)]
mod speed_tests {
    use super::*;
    use crate::Modifier;

    /// base rate=1, 不带任何速度词条 → action_rate 不变。
    fn input(base_rate: f64) -> MinimalInput {
        MinimalInput {
            base_action_rate: base_rate,
            ..MinimalInput::default()
        }
    }

    fn mk(name: &str, mt: ModType, v: f64) -> Modifier {
        Modifier::number(name, mt, v)
    }

    #[test]
    fn cast_speed_feeds_spell_action_rate() {
        // 法术：+50% increased Cast Speed → action_rate = 1.0 × 1.5。
        let mut db = ModDb::new();
        db.add_mod(mk("CastSpeed", ModType::Inc, 50.0));
        let cfg = CalcConfig::spell();
        let out = calculate_minimal(&db, &cfg, &input(1.0));
        assert!(
            (out.action_rate - 1.5).abs() < 1e-6,
            "got {}",
            out.action_rate
        );
    }

    /// 03-04：超过服务器帧上限（1/0.033≈30.303/s）的攻速被截断（非引导技能）。
    #[test]
    fn server_tick_caps_high_attack_rate() {
        let mut db = ModDb::new();
        db.add_mod(mk("AttackSpeed", ModType::Inc, 4000.0)); // 1×41 = 41/s
        let cfg = CalcConfig::attack();
        let out = calculate_minimal(&db, &cfg, &input(1.0));
        let server_cap = 1.0 / pobr_data::prelude::SERVER_TICK_SECONDS;
        assert!(
            (out.action_rate - server_cap).abs() < 0.02,
            "expected ~{server_cap}, got {}",
            out.action_rate
        );
    }

    /// 03-04：引导技能（Channelling）不受服务器帧 cap。
    #[test]
    fn channelling_skill_bypasses_server_tick_cap() {
        let mut db = ModDb::new();
        db.add_mod(mk("AttackSpeed", ModType::Inc, 4000.0));
        let cfg = CalcConfig::attack().with_condition("Channelling", true);
        let out = calculate_minimal(&db, &cfg, &input(1.0));
        assert!(
            out.action_rate > 40.0,
            "channelling bypass, got {}",
            out.action_rate
        );
    }

    /// 03-04 回归保护：低于帧上限的速率不变。
    #[test]
    fn low_rate_unaffected_by_server_tick_cap() {
        let mut db = ModDb::new();
        db.add_mod(mk("AttackSpeed", ModType::Inc, 50.0));
        let cfg = CalcConfig::attack();
        let out = calculate_minimal(&db, &cfg, &input(1.0));
        assert!(
            (out.action_rate - 1.5).abs() < 1e-6,
            "got {}",
            out.action_rate
        );
    }

    #[test]
    fn skill_speed_feeds_action_rate() {
        // SkillSpeed 与 CastSpeed/AttackSpeed 同 additive bucket。
        let mut db = ModDb::new();
        db.add_mod(mk("SkillSpeed", ModType::Inc, 20.0));
        db.add_mod(mk("AttackSpeed", ModType::Inc, 30.0));
        let cfg = CalcConfig::attack();
        let out = calculate_minimal(&db, &cfg, &input(1.0));
        // (1 + (20+30)/100) = 1.5
        assert!(
            (out.action_rate - 1.5).abs() < 1e-6,
            "got {}",
            out.action_rate
        );
    }

    #[test]
    fn action_speed_is_independent_multiplier() {
        // ActionSpeed 是独立乘区：speed bucket × ActionSpeedMod。
        // +100% bucket (×2) 且 +50% ActionSpeed (×1.5) → ×3。
        let mut db = ModDb::new();
        db.add_mod(mk("AttackSpeed", ModType::Inc, 100.0));
        db.add_mod(mk("ActionSpeed", ModType::Inc, 50.0));
        let cfg = CalcConfig::attack();
        let out = calculate_minimal(&db, &cfg, &input(1.0));
        assert!(
            (out.action_rate - 3.0).abs() < 1e-6,
            "got {}",
            out.action_rate
        );
    }

    #[test]
    fn cooldown_caps_rate_after_speed() {
        // SkillCooldownBase=2s → 上限 ≈0.5/s（冷却取整到服务器帧后略 <0.5）。
        // 即便速度把 uncapped 推到 2.0，也被 min 截到冷却上限，远低于 2.0。
        let mut db = ModDb::new();
        db.add_mod(mk("SkillCooldownBase", ModType::Base, 2.0));
        db.add_mod(mk("CastSpeed", ModType::Inc, 100.0)); // ×2 → uncapped 2.0
        let cfg = CalcConfig::spell();
        let out = calculate_minimal(&db, &cfg, &input(1.0));
        assert!(
            (out.action_rate - 0.5).abs() < 0.01 && out.action_rate < 2.0,
            "got {}",
            out.action_rate
        );
    }

    #[test]
    fn cooldown_does_not_raise_slow_rate() {
        // 速度未达上限时，冷却不抬升速率（min 不取更大值）。base 0.2 < 0.5 cap → 仍 0.2。
        let mut db = ModDb::new();
        db.add_mod(mk("SkillCooldownBase", ModType::Base, 2.0)); // cap 0.5
        let cfg = CalcConfig::spell();
        let out = calculate_minimal(&db, &cfg, &input(0.2));
        assert!(
            (out.action_rate - 0.2).abs() < 1e-6,
            "got {}",
            out.action_rate
        );
    }

    #[test]
    fn cooldown_recovery_raises_cap() {
        // CooldownRecovery +100% → effective_cd = 2/2 = 1s → cap ≈1.0/s（取整到帧后略 <1.0），
        // 显著高于无恢复时的 ≈0.5 上限。
        let mut db = ModDb::new();
        db.add_mod(mk("SkillCooldownBase", ModType::Base, 2.0));
        db.add_mod(mk("CooldownRecovery", ModType::Inc, 100.0));
        db.add_mod(mk("CastSpeed", ModType::Inc, 200.0)); // uncapped 3.0
        let cfg = CalcConfig::spell();
        let out = calculate_minimal(&db, &cfg, &input(1.0));
        assert!(
            (out.action_rate - 1.0).abs() < 0.03 && out.action_rate > 0.6,
            "got {}",
            out.action_rate
        );
    }

    #[test]
    fn cooldown_bypass_flag_skips_cap() {
        // CooldownBypass flag（如 Flicker）→ 不限速，按全速出手。
        let mut db = ModDb::new();
        db.add_mod(mk("SkillCooldownBase", ModType::Base, 2.0)); // 若生效 cap 0.5
        db.add_mod(Modifier::flag("CooldownBypass"));
        db.add_mod(mk("AttackSpeed", ModType::Inc, 100.0)); // uncapped 2.0
        let cfg = CalcConfig::attack();
        let out = calculate_minimal(&db, &cfg, &input(1.0));
        assert!(
            (out.action_rate - 2.0).abs() < 1e-6,
            "got {}",
            out.action_rate
        );
    }
}
