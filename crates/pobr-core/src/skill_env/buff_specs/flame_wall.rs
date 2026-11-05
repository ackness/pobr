//! Flame Wall's explicit projectile-through-wall snapshot. Only the projectile
//! set's added fire (and lightning for an explicitly infused wall) belongs to
//! the recipient; the wall's damage/ignite, quality damage multipliers and
//! damage supports do not modify that outgoing flat buff.

use super::*;
use crate::CalcConfig;

/// Builds Flame Wall's added-damage buff only when the existing config declares
/// that the projectile passed through an available wall. No wall lifetime,
/// Lightning Infusion consumption or lingering ignite is inferred.
pub fn flame_wall_buff_specs(
    groups: &dyn SocketGroupView,
    data: &dyn BuffEnv,
    gem_level_bonuses: &[GemPropertyBonus],
    cfg: &CalcConfig,
) -> Vec<BuffSpec> {
    if !cfg.condition("FlameWallAddedDamage") {
        return Vec::new();
    }
    let mut specs = Vec::new();
    groups.for_each_enabled_group(&mut |group| {
        if !group.active_in_weapon_set {
            return;
        }
        for (index, gem) in group.gems.iter().enumerate() {
            if gem.skill_id != "FlameWallPlayer" {
                continue;
            }
            let level = gem.gem_level
                + gem.global_levels(gem_level_bonuses, data)
                + support_granted_gem_levels(
                    &gem_judgements(&group, data, &gem.skill_id),
                    group.gems,
                    data,
                );
            // Resolve from the primary set even if the UI selected a different
            // set. The stable additional-set id identifies the buff, independent
            // of display order; no other unselected payload becomes global.
            let Some(set) = data
                .unselected_set_stats(&gem.skill_id, level, gem.quality, None)
                .into_iter()
                .find(|set| set.set_id == "FlameWallProjectileBuffPlayer")
            else {
                continue;
            };
            let mods = set
                .stats
                .into_iter()
                .filter_map(|stat| {
                    let modifier =
                        skill_stats::flame_wall_projectile_modifier(&stat.stat, stat.value, cfg)?;
                    let origin = ModifierSource::new(SourceId::new(
                        SourceKind::SkillGem,
                        format!("buff.{}.{}", gem.skill_id, stat.stat),
                    ))
                    .with_raw_text(format!(
                        "Flame Wall level {level}: {} ({})",
                        stat.stat, stat.value
                    ));
                    Some(modifier.with_origin(origin))
                })
                .collect::<Vec<_>>();
            if mods.is_empty() {
                continue;
            }
            // Keep every usable copy: buff_pass merges equal-name buffs by the
            // strongest modifier, preserving its source instead of summing walls
            // or depending on socket-group order.
            specs.push(BuffSpec {
                name: buff_skill_name(data, &gem.skill_id),
                kind: BuffKind::Buff,
                skill_id: gem.skill_id.clone(),
                mods,
                magnitude: 1.0,
                slot: group.slot.map(str::to_owned),
                socket_index: (index + 1) as u32,
                is_mark: false,
                ignore_curse_limit: false,
                local_effect_inc: 0.0,
                local_effect_more: 1.0,
                skill_types: data
                    .effect(&gem.skill_id)
                    .map(|effect| effect.skill_type_bits())
                    .unwrap_or_default(),
            });
        }
    });
    specs
}
