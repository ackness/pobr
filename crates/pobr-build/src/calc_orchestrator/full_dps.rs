//! Optional aggregation of the active effects a build marks for Full DPS.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use pobr_core::calc::AilmentStageTerms;
use pobr_core::calc::ailment::apply_dot_dps_cap;
use pobr_core::calc::output::{FullDpsOutput, FullDpsSkill};
use pobr_data::constants::{AilmentType, DOT_DPS_CAP};
use pobr_data::item::EquipmentSlot::{Weapon1, Weapon2};

use super::{
    CalculationContext, DataOrchestratorOptions, StatMapMode, calculate_prepared_with_context,
    group_active_effects, prepared_build_view,
};
use crate::calc_cache::TriggerMemo;
use crate::{Build, BuildData, BuildError, WeaponSet, build::JewelSocket};

/// Evaluates only included groups. Preparation is shared by all effects using
/// the same weapon set; trigger, support and minion handling stays in the normal
/// per-effect pipeline. Counts describe simultaneous sources, not a rotation.
pub fn calculate(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
) -> Result<FullDpsOutput, BuildError> {
    let prepared = prepared_build_view(build, data, options);
    calculate_prepared(build, &prepared, data, options, None)
}

pub(super) fn calculate_prepared(
    build: &Build,
    prepared: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    trigger_memo: Option<Rc<RefCell<TriggerMemo>>>,
) -> Result<FullDpsOutput, BuildError> {
    let needs_other = prepared.socket_groups.iter().any(|group| {
        group.enabled
            && group.include_in_full_dps
            && group
                .weapon_set
                .is_some_and(|set| set != build.weapon_set.number())
    });
    let other = needs_other.then(|| {
        let swapped = other_weapon_set(build);
        prepared_build_view(&swapped, data, options).into_owned()
    });
    let trigger_memo = (options.stat_map_mode != StatMapMode::Compare)
        .then(|| trigger_memo.unwrap_or_else(|| Rc::new(RefCell::new(TriggerMemo::new(64)))));
    let mut primary = prepared.clone();
    // Every primary-set effect is calculated from this one build, whose selection alone changes.
    let _primary_scope = super::selection_memo::SelectionScope::enter(&primary, data);
    let mut result = FullDpsOutput::default();
    let mut ailments: [Vec<StackSource>; 3] = std::array::from_fn(|_| Vec::new());
    let mut skill_dots: BTreeMap<String, Vec<StackSource>> = BTreeMap::new();
    let mut cull: f64 = 1.0;
    let mut beira: Option<StackSource> = None;
    let mut mixed_ignite_stacks = false;
    for (group_index, group) in prepared.socket_groups.iter().enumerate() {
        if !group.enabled || !group.include_in_full_dps {
            continue;
        }
        let use_other = group
            .weapon_set
            .is_some_and(|set| set != build.weapon_set.number())
            && other.is_some();
        let view = if use_other {
            other.as_ref().expect("other weapon set prepared")
        } else {
            prepared
        };
        // Alternate-weapon-set groups have different equipment/passives and must
        // never be adopted into `primary`'s SelectionScope.
        let mut other_scoped: Option<Build> = None;
        let (_other_scope, scoped): (Option<super::selection_memo::SelectionScope>, &mut Build) =
            if use_other {
                let other_build = other_scoped.insert(view.clone());
                other_build.main_socket_group = Some(group_index + 1);
                let scope = super::selection_memo::SelectionScope::enter(other_build, data);
                (Some(scope), other_build)
            } else {
                primary.main_socket_group = Some(group_index + 1);
                (None, &mut primary)
            };
        let saved_active_skill = scoped.socket_groups[group_index].main_active_skill;
        for effect in group_active_effects(data, &view.socket_groups[group_index]) {
            // An item's InbuiltTrigger is one intrinsic ability. Exports can retain
            // its generated Item: row beside the editable group holding its supports.
            // Counting that mirror again would create a second trigger on the item.
            if group
                .source
                .as_deref()
                .is_some_and(|source| source.starts_with("Item:"))
                && data
                    .granted_effects
                    .get(effect.skill_id)
                    .is_some_and(|skill| {
                        skill
                            .skill_types
                            .iter()
                            .any(|kind| kind == "InbuiltTrigger")
                    })
                && prepared.socket_groups.iter().any(|editable| {
                    editable.enabled
                        && editable.include_in_full_dps
                        && editable.from_gem()
                        && editable.weapon_set.unwrap_or(build.weapon_set.number())
                            == group.weapon_set.unwrap_or(build.weapon_set.number())
                        && group_active_effects(data, editable).any(|other| {
                            other.skill_id == effect.skill_id
                                && editable
                                    .group_count
                                    .or_else(|| {
                                        editable
                                            .gem_skills
                                            .get(other.gem_index)
                                            .and_then(|gem| gem.count)
                                    })
                                    .unwrap_or(1.0)
                                    > 0.0
                        })
                })
            {
                continue;
            }
            let count = group
                .group_count
                .or_else(|| {
                    group
                        .gem_skills
                        .get(effect.gem_index)
                        .and_then(|gem| gem.count)
                })
                .unwrap_or(1.0);
            if !count.is_finite() || count <= 0.0 {
                continue;
            }
            scoped.socket_groups[group_index].main_active_skill = Some(effect.active_skill_index);
            let index = result.skills.len();
            let mut row = FullDpsSkill {
                group_index,
                active_skill_index: effect.active_skill_index,
                skill_id: effect.skill_id.to_owned(),
                count,
                hit_dps: 0.0,
                dot_dps: 0.0,
                dps: 0.0,
                unsupported_modifiers: Vec::new(),
                error: None,
            };
            let resolved = data.granted_effects.contains_key(effect.skill_id)
                && data
                    .granted_effect_levels
                    .get(effect.skill_id)
                    .is_some_and(|levels| !levels.is_empty());
            let calculation = if resolved {
                let mut context = CalculationContext::new(data, options);
                context.trigger_memo = trigger_memo.clone();
                calculate_prepared_with_context(scoped, data, options, &mut context)
            } else {
                Err(BuildError::Calc(format!(
                    "Cannot resolve included effect {}",
                    effect.skill_id
                )))
            };
            match calculation {
                Err(error) => row.error = Some(error.to_string()),
                Ok(session) => {
                    row.unsupported_modifiers = session.unsupported_modifier_texts().to_vec();
                    let out = session.output();
                    // A charm is one independent player source. Keep its strongest
                    // snapshot once, never multiply it by a skill/group count.
                    let ground = out
                        .utility_stats
                        .get("BeiraIgniteDPS")
                        .copied()
                        .unwrap_or(0.0);
                    if ground > beira.as_ref().map_or_default(|source| source.dps) {
                        beira = Some(StackSource {
                            index,
                            dps: ground,
                            stacks: 1.0,
                            cap: 1.0,
                        });
                    }
                    mixed_ignite_stacks |= out.ignite_max_stacks > 1.0;
                    let enabled_group = scoped.socket_groups[..group_index]
                        .iter()
                        .filter(|g| g.enabled)
                        .count();
                    let minions: Vec<_> = out
                        .minions
                        .iter()
                        .filter(|m| {
                            m.summoned_by == effect.skill_id
                                && m.summoner == Some((enabled_group, effect.gem_index))
                        })
                        .collect();
                    if !minions.is_empty() {
                        row.hit_dps = minions.iter().map(|minion| minion.dps).sum();
                    } else {
                        row.hit_dps = out.dps;
                        for (sources, (kind, dps, stacked, active, cap)) in
                            ailments.iter_mut().zip([
                                (
                                    AilmentType::Bleed,
                                    out.bleed_dps,
                                    out.bleed_stacked_dps,
                                    out.bleed_active_stacks,
                                    out.bleed_max_stacks,
                                ),
                                (
                                    AilmentType::Ignite,
                                    out.ignite_dps,
                                    out.ignite_stacked_dps,
                                    out.ignite_active_stacks,
                                    out.ignite_max_stacks,
                                ),
                                (
                                    AilmentType::Poison,
                                    out.poison_dps,
                                    out.poison_stacked_dps,
                                    out.poison_active_stacks,
                                    out.poison_max_stacks,
                                ),
                            ])
                        {
                            // A merged uncapped result can still include a finite
                            // other hand. Keep those contributions separate so only
                            // the weapon ignoring the limit bypasses the shared pool.
                            if cap == 0.0
                                && let Some((_, stage)) = out
                                    .ailment_terms
                                    .iter()
                                    .find(|(ailment, _)| *ailment == kind)
                            {
                                sources.extend(uncapped_ailment_sources(
                                    stage,
                                    index,
                                    count,
                                    stacked.max(dps),
                                ));
                                continue;
                            }
                            // The ailment pass already applies magnitude, duration and
                            // chance. Keep its estimate; more sources can fill the same
                            // limit, but cannot create a new limit for each skill.
                            if stacked > 0.0 && active > 0.0 {
                                let affecting = if cap == 0.0 {
                                    active
                                } else {
                                    active.min(cap.max(1.0))
                                };
                                sources.push(StackSource {
                                    index,
                                    dps: stacked / affecting,
                                    stacks: active * count,
                                    cap: if cap == 0.0 { 0.0 } else { cap.max(1.0) },
                                });
                            } else if dps > 0.0 {
                                sources.push(StackSource {
                                    index,
                                    dps,
                                    stacks: if cap == 0.0 { count } else { 1.0 },
                                    cap: if cap == 0.0 { 0.0 } else { cap.max(1.0) },
                                });
                            }
                        }
                        if out.skill_total_dot > 0.0 {
                            if session.mod_db().flag(session.config(), "DotCanStack") {
                                row.dot_dps += out.skill_total_dot * count;
                            } else {
                                // Reapplying the same non-stacking skill debuff only
                                // replaces it. Different skills' debuffs remain distinct,
                                // even when they deal the same damage type.
                                skill_dots
                                    .entry(effect.skill_id.to_owned())
                                    .or_default()
                                    .push(StackSource {
                                        index,
                                        dps: out.skill_total_dot,
                                        stacks: 1.0,
                                        cap: 1.0,
                                    });
                            }
                        }
                        if let Some(skill_cull) = skill_cull_multiplier(out) {
                            cull = cull.max(skill_cull);
                        }
                    }
                }
            }
            result.skills.push(row);
        }
        scoped.socket_groups[group_index].main_active_skill = saved_active_skill;
    }
    // Cross-source multi-Ignite budgets are unverified, as in the selected
    // skill view. Ordinary Ignites compete for one enemy-wide instance.
    if !mixed_ignite_stacks && let Some(source) = beira {
        ailments[1].push(source);
    }
    // Bleeding/Ignite/Poison each share an enemy-wide active limit. The game's
    // `number_of_additional_poison_stacks` increases that limit, not the number
    // of independent poison pools. Only the strongest allowed instances damage.
    // A weapon's uncapped poisons coexist outside that finite shared pool.
    for sources in ailments.iter_mut().chain(skill_dots.values_mut()) {
        allocate_stacks(sources, &mut result.skills);
    }
    let raw_dot: f64 = result.skills.iter().map(|row| row.dot_dps).sum();
    let dot_scale = if raw_dot > DOT_DPS_CAP {
        DOT_DPS_CAP / raw_dot
    } else {
        1.0
    };
    for row in &mut result.skills {
        row.dot_dps *= dot_scale;
        row.dps = (row.hit_dps * row.count + row.dot_dps) * cull;
    }
    result.dot_dps = result.skills.iter().map(|row| row.dot_dps).sum();
    result.dps = result.skills.iter().map(|row| row.dps).sum();
    Ok(result)
}

struct StackSource {
    index: usize,
    dps: f64,
    stacks: f64,
    /// Zero means this source bypasses the finite shared stack pool.
    cap: f64,
}

/// Project an uncapped skill snapshot into independently capped hand sources.
/// A finite-only skill keeps its existing strongest-hand merge instead.
fn uncapped_ailment_sources(
    stage: &AilmentStageTerms,
    index: usize,
    count: f64,
    merged_dps: f64,
) -> Vec<StackSource> {
    let mut total = 0.0;
    let mut sources = Vec::new();
    for (_, terms) in &stage.passes {
        let stacks = if terms.stacks > 0.0 {
            terms.stacks
        } else {
            // Same missing-rate fallback as the selected-skill calculation:
            // the finite ceiling, or a single instance when uncapped.
            terms.max_stacks.max(1.0)
        };
        let affecting = if terms.max_stacks == 0.0 {
            stacks
        } else {
            stacks.min(terms.max_stacks)
        };
        let dps = if terms.stacks > 0.0 {
            terms.dps
        } else {
            apply_dot_dps_cap(terms.dps * affecting, terms.cap)
        };
        if dps > 0.0 && affecting > 0.0 {
            total += dps;
            sources.push(StackSource {
                index,
                dps: dps / affecting,
                stacks: stacks * count,
                cap: terms.max_stacks,
            });
        }
    }
    // Each hand was capped already, and their merged snapshot may hit the
    // DoT cap again. Preserve that snapshot before count scaling and the
    // existing summed Full DPS cap, without exempting its finite hand.
    if total > merged_dps {
        let scale = merged_dps / total;
        for source in &mut sources {
            source.dps *= scale;
        }
    }
    sources
}

fn allocate_stacks(sources: &mut [StackSource], rows: &mut [FullDpsSkill]) {
    sources.sort_by(|a, b| b.dps.total_cmp(&a.dps));
    let mut remaining = sources
        .iter()
        .filter(|source| source.cap > 0.0)
        .map(|source| source.cap)
        .fold(0.0, f64::max);
    for source in sources {
        if source.cap == 0.0 {
            rows[source.index].dot_dps += source.dps * source.stacks;
            continue;
        }
        let active = source.stacks.min(source.cap).min(remaining);
        rows[source.index].dot_dps += source.dps * active;
        remaining -= active;
    }
}

pub(super) fn other_weapon_set(build: &Build) -> Build {
    let mut swapped = build.clone();
    for slot in [Weapon1, Weapon2] {
        swapped.items.remove(&slot);
        if let Some(item) = build.other_weapons.get(&slot) {
            swapped.items.insert(slot, item.clone());
        }
    }
    swapped.other_weapons = build
        .items
        .iter()
        .filter(|(slot, _)| matches!(slot, Weapon1 | Weapon2))
        .map(|(slot, item)| (*slot, item.clone()))
        .collect();
    swapped.weapon_set = if build.weapon_set == WeaponSet::First {
        WeaponSet::Second
    } else {
        WeaponSet::First
    };
    if let Some(nodes) = &build.other_weapon_set_nodes {
        swapped.tree.allocated_nodes = nodes.clone();
        swapped.other_weapon_set_nodes = Some(build.tree.allocated_nodes.clone());
        // Socketed jewels follow allocation too. Retain their raw records so the
        // normal preparation step can resolve sockets granted by items/passives.
        let removed_jewels: Vec<_> = swapped
            .jewels
            .extract_if(.., |(socket, _)| {
                matches!(socket, JewelSocket::Node(node) if !nodes.iter().any(|id| id.0 == *node))
            })
            .collect();
        for (socket, item) in removed_jewels {
            if let JewelSocket::Node(node) = socket {
                let radius = swapped
                    .radius_jewels
                    .extract_if(.., |r| r.socket_node == node)
                    .next();
                swapped.granted_socket_jewels.push((node, item, radius));
            }
        }
        swapped
            .radius_jewels
            .retain(|radius| nodes.iter().any(|id| id.0 == radius.socket_node));
        for (node, item, radius) in swapped
            .granted_socket_jewels
            .extract_if(.., |(node, _, _)| nodes.iter().any(|id| id.0 == *node))
        {
            swapped.jewels.push((JewelSocket::Node(node), item));
            if let Some(radius) = radius {
                swapped.radius_jewels.push(radius);
            }
        }
    }
    swapped
}

/// Exact cull/reservation DPS multiplier for a damaging skill; returns `None` when
/// the skill deals no hit or DoT damage so untriggered 0-DPS skills cannot donate
/// local Culling Strike to the rest of Full DPS.
fn skill_cull_multiplier(out: &pobr_core::calc::OutputTable) -> Option<f64> {
    let uncull = out.dps + out.total_dot_dps;
    (uncull > 0.0).then_some(out.cull_multiplier * out.reservation_dps_multiplier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strongest_instances_share_a_limit_and_fill_unused_stacks() {
        let mut rows = Vec::from_fn(2, |index| FullDpsSkill {
            group_index: index,
            active_skill_index: 1,
            skill_id: "Poison".into(),
            count: 1.0,
            hit_dps: 0.0,
            dot_dps: 0.0,
            dps: 0.0,
            unsupported_modifiers: Vec::new(),
            error: None,
        });
        let mut sources = [
            StackSource {
                index: 0,
                dps: 10.0,
                stacks: 5.0,
                cap: 3.0,
            },
            StackSource {
                index: 1,
                dps: 100.0,
                stacks: 1.5,
                cap: 3.0,
            },
        ];
        allocate_stacks(&mut sources, &mut rows);
        assert_eq!(rows[1].dot_dps, 150.0);
        assert_eq!(rows[0].dot_dps, 15.0);
    }

    #[test]
    fn skill_cull_multiplier_uses_exact_multipliers_and_ignores_zero_dps_skills() {
        let zero_dps = pobr_core::calc::OutputTable {
            dps: 0.0,
            total_dot_dps: 0.0,
            cull_multiplier: 1.25,
            reservation_dps_multiplier: 1.1,
            ..Default::default()
        };
        assert_eq!(skill_cull_multiplier(&zero_dps), None);

        let damaging = pobr_core::calc::OutputTable {
            dps: 100.0,
            total_dot_dps: 25.0,
            combined_dps: 126.0, // rounded combined_dps must not perturb exact multiplier
            cull_multiplier: 1.0,
            reservation_dps_multiplier: 1.0,
            ..Default::default()
        };
        assert_eq!(skill_cull_multiplier(&damaging), Some(1.0));

        let culled = pobr_core::calc::OutputTable {
            dps: 100.0,
            total_dot_dps: 0.0,
            cull_multiplier: 1.25,
            reservation_dps_multiplier: 1.2,
            ..Default::default()
        };
        assert!((skill_cull_multiplier(&culled).unwrap() - 1.5).abs() < 1e-12);
    }
}
