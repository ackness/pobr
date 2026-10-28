//! Optional aggregation of the active effects a build marks for Full DPS.

use std::collections::BTreeMap;

use pobr_core::calc::output::{FullDpsOutput, FullDpsSkill};
use pobr_data::constants::DOT_DPS_CAP;
use pobr_data::item::EquipmentSlot::{Weapon1, Weapon2};

use super::{
    CalculationContext, DataOrchestratorOptions, calculate_prepared_with_context,
    group_active_effects, prepared_build_view,
};
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
    calculate_prepared(build, &prepared, data, options)
}

pub(super) fn calculate_prepared(
    build: &Build,
    prepared: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
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
        let view = if group
            .weapon_set
            .is_some_and(|set| set != build.weapon_set.number())
        {
            other.as_ref().unwrap_or(prepared)
        } else {
            prepared
        };
        let mut scoped = view.clone();
        scoped.main_socket_group = Some(group_index + 1);
        // Each effect of the group is calculated from this build, whose selection alone changes.
        let _scope = super::selection_memo::SelectionScope::enter(&scoped, data);
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
                calculate_prepared_with_context(
                    &scoped,
                    data,
                    options,
                    &mut CalculationContext::new(data, options),
                )
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
                    if ground > beira.as_ref().map_or(0.0, |source| source.dps) {
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
                        for (sources, (dps, stacked, active, cap)) in ailments.iter_mut().zip([
                            (
                                out.bleed_dps,
                                out.bleed_stacked_dps,
                                out.bleed_active_stacks,
                                out.bleed_max_stacks,
                            ),
                            (
                                out.ignite_dps,
                                out.ignite_stacked_dps,
                                out.ignite_active_stacks,
                                out.ignite_max_stacks,
                            ),
                            (
                                out.poison_dps,
                                out.poison_stacked_dps,
                                out.poison_active_stacks,
                                out.poison_max_stacks,
                            ),
                        ]) {
                            // The ailment pass already applies magnitude, duration and
                            // chance. Keep its estimate; more sources can fill the same
                            // limit, but cannot create a new limit for each skill.
                            if stacked > 0.0 && active > 0.0 {
                                sources.push(StackSource {
                                    index,
                                    dps: stacked / active.min(cap.max(1.0)),
                                    stacks: active * count,
                                    cap: cap.max(1.0),
                                });
                            } else if dps > 0.0 {
                                sources.push(StackSource {
                                    index,
                                    dps,
                                    stacks: 1.0,
                                    cap: cap.max(1.0),
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
                        let uncull = out.dps + out.total_dot_dps;
                        if uncull > 0.0 {
                            cull = cull.max(out.combined_dps / uncull);
                        }
                    }
                }
            }
            result.skills.push(row);
        }
    }
    // Cross-source multi-Ignite budgets are unverified, as in the selected
    // skill view. Ordinary Ignites compete for one enemy-wide instance.
    if !mixed_ignite_stacks && let Some(source) = beira {
        ailments[1].push(source);
    }
    // Bleeding/Ignite/Poison each share an enemy-wide active limit. The game's
    // `number_of_additional_poison_stacks` increases that limit, not the number
    // of independent poison pools. Only the strongest allowed instances damage.
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
    cap: f64,
}

fn allocate_stacks(sources: &mut [StackSource], rows: &mut [FullDpsSkill]) {
    sources.sort_by(|a, b| b.dps.total_cmp(&a.dps));
    let mut remaining = sources.iter().map(|source| source.cap).fold(0.0, f64::max);
    for source in sources {
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
        swapped.jewels.retain(|(socket, item)| {
            if let JewelSocket::Node(node) = socket
                && !nodes.iter().any(|id| id.0 == *node)
            {
                let radius = build
                    .radius_jewels
                    .iter()
                    .find(|r| r.socket_node == *node)
                    .cloned();
                swapped
                    .granted_socket_jewels
                    .push((*node, item.clone(), radius));
                false
            } else {
                true
            }
        });
        swapped
            .radius_jewels
            .retain(|radius| nodes.iter().any(|id| id.0 == radius.socket_node));
        swapped
            .granted_socket_jewels
            .retain(|(node, item, radius)| {
                if nodes.iter().any(|id| id.0 == *node) {
                    swapped
                        .jewels
                        .push((JewelSocket::Node(*node), item.clone()));
                    if let Some(radius) = radius {
                        swapped.radius_jewels.push(radius.clone());
                    }
                    false
                } else {
                    true
                }
            });
    }
    swapped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strongest_instances_share_a_limit_and_fill_unused_stacks() {
        let mut rows: Vec<_> = (0..2)
            .map(|index| FullDpsSkill {
                group_index: index,
                active_skill_index: 1,
                skill_id: "Poison".into(),
                count: 1.0,
                hit_dps: 0.0,
                dot_dps: 0.0,
                dps: 0.0,
                unsupported_modifiers: Vec::new(),
                error: None,
            })
            .collect();
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
}
