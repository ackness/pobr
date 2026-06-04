use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, TraceGraph, TraceNodeId, TraceOperation, TraceOutput, TracedValue};

use super::damage::{DamageComponent, calculate_components};
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

pub fn calculate_minimal(db: &ModDb, cfg: &CalcConfig, input: &MinimalInput) -> MinimalOutput {
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
    let non_crit_hit_avg: f64 = damage_components.iter().map(DamageComponent::avg).sum();

    let crit_chance_names = [ModName::from("CriticalStrikeChance")];
    let crit_chance_base = db.sum(ModType::Base, cfg, &crit_chance_names);
    let crit_chance_inc = db.sum(ModType::Inc, cfg, &crit_chance_names);
    let crit_chance_more = db.more(cfg, &crit_chance_names);
    let crit_chance = round(
        (crit_chance_base * (1.0 + crit_chance_inc / 100.0) * crit_chance_more / 100.0)
            .clamp(0.0, 1.0),
    );

    let crit_multiplier_names = [ModName::from("CriticalStrikeMultiplier")];
    let crit_multiplier =
        round((150.0 + db.sum(ModType::Base, cfg, &crit_multiplier_names)) / 100.0);
    let crit_average_factor = 1.0 + crit_chance * (crit_multiplier - 1.0);
    let total_hit_avg = round(non_crit_hit_avg * crit_average_factor);

    let speed_names = [ModName::from("AttackSpeed"), ModName::from("ActionSpeed")];
    let inc_speed = db.sum(ModType::Inc, cfg, &speed_names);
    let more_speed = db.more(cfg, &speed_names);
    let action_rate = round(input.base_action_rate * (1.0 + inc_speed / 100.0) * more_speed);
    let accuracy_names = [ModName::from("Accuracy")];
    let accuracy = scaled_numeric_stat(db, cfg, input.base_accuracy, &accuracy_names);
    let hit_chance = hit_chance(input.enemy_evasion, accuracy);
    let dps = round(total_hit_avg * action_rate * hit_chance);

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
    let damage_cfg = if cfg.damage_type.is_some() {
        cfg.clone()
    } else {
        cfg.clone().with_damage_type(DamageType::Physical)
    };
    let damage_names = [
        ModName::from("PhysicalDamage"),
        ModName::from("AttackDamage"),
        ModName::from("Damage"),
    ];

    let base_hit_avg = (input.base_hit_min + input.base_hit_max) / 2.0;
    let base_hit_node = trace.add_source_node(
        "base hit average",
        base_hit_avg,
        SourceId::new(SourceKind::CharacterBase, "base.Hit"),
    );
    let inc_damage = db.sum_traced(
        ModType::Inc,
        &damage_cfg,
        &damage_names,
        trace,
        "Damage INC modifier sum",
    );
    let more_damage =
        more_factor_traced(db, &damage_cfg, &damage_names, "Damage MORE factor", trace);
    let non_crit_hit_avg = base_hit_avg * (1.0 + inc_damage.value / 100.0) * more_damage.value;
    let non_crit_node = trace.add_node(
        "non-crit hit average",
        non_crit_hit_avg,
        TraceOperation::Multiply,
    );
    trace.add_edge(base_hit_node, non_crit_node);
    trace.add_edge(inc_damage.node_id, non_crit_node);
    trace.add_edge(more_damage.node_id, non_crit_node);

    // --- crit average factor ---
    let crit_chance_names = [ModName::from("CriticalStrikeChance")];
    let crit_chance_base = db.sum_traced(
        ModType::Base,
        cfg,
        &crit_chance_names,
        trace,
        "CriticalStrikeChance BASE sum",
    );
    let crit_chance_inc = db.sum(ModType::Inc, cfg, &crit_chance_names);
    let crit_chance_more = db.more(cfg, &crit_chance_names);
    let crit_chance = round(
        (crit_chance_base.value * (1.0 + crit_chance_inc / 100.0) * crit_chance_more / 100.0)
            .clamp(0.0, 1.0),
    );

    let crit_multiplier_names = [ModName::from("CriticalStrikeMultiplier")];
    let crit_multiplier_base = db.sum_traced(
        ModType::Base,
        cfg,
        &crit_multiplier_names,
        trace,
        "CriticalStrikeMultiplier BASE sum",
    );
    let crit_multiplier = round((150.0 + crit_multiplier_base.value) / 100.0);
    let crit_average_factor = 1.0 + crit_chance * (crit_multiplier - 1.0);
    let crit_node = trace.add_node(
        "crit average factor",
        crit_average_factor,
        TraceOperation::Chance,
    );
    trace.add_edge(crit_chance_base.node_id, crit_node);
    trace.add_edge(crit_multiplier_base.node_id, crit_node);

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

    // --- accuracy & hit chance ---
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
    let hit_chance_value = hit_chance(input.enemy_evasion, accuracy);
    let hit_chance_node = trace.add_node("hit chance", hit_chance_value, TraceOperation::Chance);
    trace.add_edge(accuracy_node, hit_chance_node);
    trace.add_edge(enemy_evasion_node, hit_chance_node);

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

/// Records a MORE aggregation (`Π(1 + v/100)`) as a single trace node fed by one
/// source node per contributing modifier, mirroring [`ModDb::more`].
fn more_factor_traced(
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
