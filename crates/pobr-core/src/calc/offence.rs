use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, TraceGraph, TraceNodeId, TraceOperation, TraceOutput, TracedValue};

use super::crit::{resolve_crit, resolve_crit_traced};
use super::damage::{DamageComponent, calculate_components, sum_avg};
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
            breakdown: breakdown.steps().to_vec(),
        }
    }
}

/// 单条抗性的解析结果：capped final / 最大抗性 / over-cap。
struct ResistanceResolution {
    final_value: f64,
    max: f64,
    over_cap: f64,
}

/// 解析一条元素抗性：
/// - total = base + Σ`<element>Resistance` Base
/// - max   = min(75 + Σ`Maximum<element>Resistance` + Σ`MaximumAllElementalResistances`, 90)
/// - final = min(total, max)（负抗性无下限）
/// - over  = max(total - max, 0)
fn resolve_resistance(
    db: &ModDb,
    cfg: &CalcConfig,
    base: f64,
    res_name: &str,
    max_res_name: &str,
) -> ResistanceResolution {
    let total = base + db.sum(ModType::Base, cfg, &[ModName::from(res_name)]);
    let max_bonus = db.sum(
        ModType::Base,
        cfg,
        &[
            ModName::from(max_res_name),
            ModName::from("MaximumAllElementalResistances"),
        ],
    );
    let max = (DEFAULT_MAX_RESISTANCE + max_bonus).min(HARD_MAX_RESISTANCE);
    ResistanceResolution {
        final_value: round(total.min(max)),
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
    let fire = resolve_resistance(
        db,
        cfg,
        input.base_fire_resistance,
        "FireResistance",
        "MaximumFireResistance",
    );
    let cold = resolve_resistance(
        db,
        cfg,
        input.base_cold_resistance,
        "ColdResistance",
        "MaximumColdResistance",
    );
    let lightning = resolve_resistance(
        db,
        cfg,
        input.base_lightning_resistance,
        "LightningResistance",
        "MaximumLightningResistance",
    );
    let fire_resistance = fire.final_value;
    let cold_resistance = cold.final_value;
    let lightning_resistance = lightning.final_value;

    let damage_components = calculate_components(db, cfg, input.base_hit_min, input.base_hit_max);
    // 玩家侧（未减伤）非暴击平均击中，供 breakdown / ailment magnitude 源（保持原口径）。
    let non_crit_hit_avg: f64 = damage_components.iter().map(DamageComponent::avg).sum();
    // 有效口径下，分伤害类型乘敌人受伤链 + 抗性/护甲减伤（含玩家穿透/Overwhelm）后的平均
    // 击中（用于 DPS）。穿透/Overwhelm 读 **玩家** db（`db`），敌人抗性/护甲读 `enemy_db`。
    let non_crit_hit_avg_mitigated = if cfg.mode_effective {
        damage_components
            .iter()
            .map(|component| {
                let avg = component.avg();
                avg * enemy_damage_multiplier(db, enemy_db, cfg, component.damage_type, avg)
            })
            .sum()
    } else {
        non_crit_hit_avg
    };

    let speed_names = [ModName::from("AttackSpeed"), ModName::from("ActionSpeed")];
    let inc_speed = db.sum(ModType::Inc, cfg, &speed_names);
    let more_speed = db.more(cfg, &speed_names);
    let action_rate = round(input.base_action_rate * (1.0 + inc_speed / 100.0) * more_speed);
    let accuracy_names = [ModName::from("Accuracy")];
    let accuracy = scaled_numeric_stat(db, cfg, input.base_accuracy, &accuracy_names);
    // PoE2 命中率（agent-docs/accuracy-and-enemy.md §二,§三）：
    // - 法术必中：`if not isAttack then output.AccuracyHitChance = 100`。
    // - `CannotBeEvaded`（玩家旗标）/ effective 下敌方 `CannotEvade` → 置 100% 跳过精准公式。
    // - 末端再扣敌方格挡：`HitChance = AccuracyHitChance * (1 - enemyBlockChance/100)`。
    let cannot_be_evaded = db.flag(cfg, ModName::from("CannotBeEvaded"))
        || (cfg.mode_effective && enemy_db.flag(cfg, ModName::from("CannotEvade")));
    let accuracy_hit_chance = if cfg.is_spell() || cannot_be_evaded {
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
    let crit_average_factor = crit.effect;
    // 输出字段：玩家侧总击中（不含敌人减伤），保持历史口径 + 作为 ailment magnitude 源。
    let total_hit_avg = round(non_crit_hit_avg * crit_average_factor);
    // DPS 用：有效口径下含敌人受伤链/抗性/护甲减伤的总击中。
    let total_hit_avg_for_dps = round(non_crit_hit_avg_mitigated * crit_average_factor);

    let dps = round(total_hit_avg_for_dps * action_rate * hit_chance);

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

pub fn calculate_minimal_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    input: &MinimalInput,
) -> TracedMinimalOutput {
    let output = calculate_minimal(db, cfg, input);
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

    let total_dps = total_dps_traced(db, cfg, input, &mut trace);
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
fn total_dps_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    input: &MinimalInput,
    trace: &mut TraceGraph,
) -> TracedValue {
    // --- average hit ---
    // 使用与 calculate_minimal 相同的 calculate_components 管线计算 non-crit 平均值，
    // 确保 traced 与非 traced 路径数值一致（Bug#5 traced-dps-physical-only-divergence）。
    // 出处：damage-scaling.md §核心叠加语义；calculate_components 实现在 damage.rs。
    let components = calculate_components(db, cfg, input.base_hit_min, input.base_hit_max);
    let non_crit_hit_avg = sum_avg(&components);

    // 同时 trace 物理伤害 modifier 来源（INC + MORE），确保 weapon/support 词条归因可达。
    // 其它伤害类型的分量 modifier 也按相同方式记录，以支持元素/混沌词条归因。
    let damage_cfg = cfg.clone().with_damage_type(DamageType::Physical);
    let damage_names = [
        ModName::from("PhysicalDamage"),
        ModName::from("AttackDamage"),
        ModName::from("Damage"),
    ];
    let inc_damage = db.sum_traced(
        ModType::Inc,
        &damage_cfg,
        &damage_names,
        trace,
        "Damage INC modifier sum",
    );
    let more_damage =
        more_factor_traced(db, &damage_cfg, &damage_names, "Damage MORE factor", trace);

    let base_hit_avg = (input.base_hit_min + input.base_hit_max) / 2.0;
    let base_hit_node = trace.add_source_node(
        "base hit average",
        base_hit_avg,
        SourceId::new(SourceKind::CharacterBase, "base.Hit"),
    );
    let non_crit_node = trace.add_node(
        "non-crit hit average (all damage types)",
        non_crit_hit_avg,
        TraceOperation::Multiply,
    );
    trace.add_edge(base_hit_node, non_crit_node);
    trace.add_edge(inc_damage.node_id, non_crit_node);
    trace.add_edge(more_damage.node_id, non_crit_node);

    // --- accuracy & hit chance（提前到暴击之前：mode_effective 暴击降级需命中率） ---
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
    // PoE2 法术必中（同 calculate_minimal）
    let hit_chance_value = if cfg.is_spell() {
        1.0
    } else {
        hit_chance(input.enemy_evasion, accuracy)
    };
    let hit_chance_node = trace.add_node("hit chance", hit_chance_value, TraceOperation::Chance);
    trace.add_edge(accuracy_node, hit_chance_node);
    trace.add_edge(enemy_evasion_node, hit_chance_node);

    // --- crit average factor（resolve_crit_traced：与非 traced 路径同一实现，
    //     BASE/INC/MORE + 敌方 SelfCrit* 全部接入 TraceGraph，gap crit-traced-inc-more-untraced）。
    //     traced 路径无敌人 modDB（旧三参口径），传空 enemy + base_crit=0。
    let enemy_db = ModDb::new();
    let (crit, crit_node) = resolve_crit_traced(
        db,
        &enemy_db,
        cfg,
        hit_chance_value,
        0.0,
        cfg.mode_effective,
        trace,
    );
    let crit_average_factor = crit.effect;

    let total_hit_avg = round(non_crit_hit_avg * crit_average_factor);
    let total_hit_node =
        trace.add_node("total hit average", total_hit_avg, TraceOperation::Multiply);
    trace.add_edge(non_crit_node, total_hit_node);
    trace.add_edge(crit_node, total_hit_node);

    // --- action rate ---
    let speed_names = [ModName::from("AttackSpeed"), ModName::from("ActionSpeed")];
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
        "Speed INC modifier sum",
    );
    let more_speed = more_factor_traced(db, cfg, &speed_names, "Speed MORE factor", trace);
    let action_rate =
        round(input.base_action_rate * (1.0 + inc_speed.value / 100.0) * more_speed.value);
    let action_rate_node = trace.add_node("action rate", action_rate, TraceOperation::Multiply);
    trace.add_edge(base_rate_node, action_rate_node);
    trace.add_edge(inc_speed.node_id, action_rate_node);
    trace.add_edge(more_speed.node_id, action_rate_node);

    // --- TotalDPS final ---
    let dps = round(total_hit_avg * action_rate * hit_chance_value);
    let dps_node = trace.add_node("TotalDPS final", dps, TraceOperation::Multiply);
    trace.add_edge(total_hit_node, dps_node);
    trace.add_edge(action_rate_node, dps_node);
    trace.add_edge(hit_chance_node, dps_node);

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

    // --- 受伤链：通用 + 分类型 DamageTaken（INC + MORE） ---
    let taken_names = [
        ModName::from("DamageTaken"),
        ModName::from(format!("{type_prefix}DamageTaken")),
    ];
    let taken_inc = enemy_db.sum(ModType::Inc, &type_cfg, &taken_names);
    let taken_more = enemy_db.more(&type_cfg, &taken_names);
    let taken_mult = (1.0 + taken_inc / 100.0) * taken_more;

    // --- 抗性减伤（元素/混沌，含玩家穿透）/ 护甲减伤 + Overwhelm（物理） ---
    let mitigation = if damage_type == DamageType::Physical {
        enemy_physical_multiplier(player_db, enemy_db, &type_cfg, raw_hit)
    } else {
        let resist = enemy_db
            .sum(
                ModType::Base,
                &type_cfg,
                &[ModName::from(format!("{type_prefix}Resist"))],
            )
            .clamp(RESIST_FLOOR, ENEMY_MAX_RESIST);
        let effective_resist = apply_penetration(player_db, &type_cfg, damage_type, resist);
        1.0 - effective_resist / 100.0
    };

    taken_mult * mitigation
}

/// 玩家穿透对**已 clamp 的**敌人抗性的下调（仅元素/混沌、仅击中）。
///
/// 读玩家 db：元素 `<Type>Penetration` + 共享 `ElementalPenetration`；混沌 `ChaosPenetration`。
/// 公式（PoB2 CalcOffence.lua，minPen=0）：
/// `effective = if resist > 0 { max(resist - pen, 0) } else { resist }`
/// —— 穿透只在抗性为正时生效、不能把抗性压到 0 以下；抗性已 ≤0（负抗）时穿透全浪费。
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
    if resist > 0.0 {
        (resist - pen).max(0.0)
    } else {
        resist
    }
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

/// 物理护甲减伤分量（对某 raw_hit），含玩家 **Overwhelm**：
///
/// 敌人 `Armour`（→ 护甲减伤）与敌人固定 `PhysicalDamageReduction BASE` 取并集
/// （PoB2: `1-(1-a)(1-b)`），再**加上**玩家 `EnemyPhysicalDamageReduction BASE`
/// （Overwhelm = 负值，下调敌人 PDR），最后 clamp 到 `[0, ENEMY_PHYS_DMGRED_CAP]`。
/// 返回 `(1 - reduction_frac)` 乘子。
///
/// 出处：agent-docs/damage-scaling.md §Overwhelm（玩家 "Overwhelm N%" → `EnemyPhysicalDamageReduction
///       BASE -N`，加进敌人 PDR 后 clamp，不破 0%）；PoB2 CalcOffence.lua physical resist 段。
fn enemy_physical_multiplier(
    player_db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    raw_hit: f64,
) -> f64 {
    let armour = enemy_db.sum(ModType::Base, cfg, &[ModName::from("Armour")]);
    let from_armour = super::armour_reduction(armour, raw_hit) * 100.0;
    let flat_pdr = enemy_db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("PhysicalDamageReduction")],
    );
    // 护甲减伤与敌人固定 PDR 取并集（PoB2: 1-(1-a)(1-b)）。
    let combined = (1.0 - (1.0 - from_armour / 100.0) * (1.0 - flat_pdr / 100.0)) * 100.0;
    // Overwhelm：玩家 EnemyPhysicalDamageReduction BASE（通常为负）直接加到敌人 PDR 上。
    let overwhelm = player_db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("EnemyPhysicalDamageReduction")],
    );
    let reduction = (combined + overwhelm).clamp(0.0, ENEMY_PHYS_DMGRED_CAP);
    1.0 - reduction / 100.0
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

fn scaled_pool(db: &ModDb, cfg: &CalcConfig, base: f64, name: &str) -> f64 {
    let names = [ModName::from(name)];
    scaled_numeric_stat(db, cfg, base, &names)
}

fn scaled_numeric_stat(db: &ModDb, cfg: &CalcConfig, base: f64, names: &[ModName]) -> f64 {
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
    let base_total = base + base_mods.value;
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
