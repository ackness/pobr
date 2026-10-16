//! A summoned monster casts its own curses at its own level.

use super::*;
use crate::skill_env::minions::{MinionEnv, group_judgement_from, minion_skill_level};

pub(in crate::skill_env) fn minion_curse_specs(
    data: &dyn MinionEnv,
    group: &EnabledGroup<'_>,
    def: &pobr_data::minion::MinionDef,
    minion_level: u32,
) -> Vec<BuffSpec> {
    let mut ctx = StatMapCtx::data(data.stat_map_catalog());
    def.skill_list
        .iter()
        .filter_map(|id| {
            let effect = data.effect(id)?;
            if !effect.skill_types.iter().any(|kind| kind == "AppliesCurse") {
                return None;
            }
            let level = minion_skill_level(data, id, minion_level);
            let stats = data.effect_stats_at_user_level(id, level, minion_level);
            let key = data.selected_set_key(id, None);
            let mut mods = crate::skill_env::stat_map::minion_curse_stat_modifiers(
                &mut ctx,
                &stats,
                id,
                key.as_deref(),
            );
            if mods.is_empty() {
                return None;
            }
            for modifier in &mut mods {
                if let Some(origin) = &mut modifier.origin {
                    origin.source_id = SourceId::new(
                        SourceKind::Buff,
                        format!("minion.{}.{}", def.id, origin.source_id.id),
                    );
                }
            }
            let (mut local_effect_inc, mut local_effect_more) = (0.0, 1.0);
            if let Some(catalog) = ctx.catalog {
                let mut absorb =
                    |effect_id: &str, key: Option<&str>, stats: &crate::skill_env::EffectStats| {
                        for stat in stats.all() {
                            let (inc, more) = stat_map_engine::curse_local_effect(
                                catalog, effect_id, key, &stat.stat, stat.value,
                            );
                            local_effect_inc += inc;
                            local_effect_more *= more;
                        }
                    };
                absorb(id, key.as_deref(), &stats);
                for support in group_judgement_from(group, data, id, false).compatible {
                    let gem = &group.gems[support.gem_index];
                    let set = support_stat_set_index(&support, group.gems);
                    let stats =
                        data.effect_stats(&support.effect_id, gem.gem_level, gem.quality, set);
                    let key = data.selected_set_key(&support.effect_id, set);
                    absorb(&support.effect_id, key.as_deref(), &stats);
                }
            }
            Some(BuffSpec {
                name: buff_skill_name(data, id),
                kind: BuffKind::Curse,
                skill_id: id.clone(),
                mods,
                magnitude: 1.0,
                slot: None,
                socket_index: 1,
                is_mark: effect.skill_types.iter().any(|kind| kind == "Mark"),
                ignore_curse_limit: false,
                local_effect_inc,
                local_effect_more,
                skill_types: skill_type_bits(&effect.skill_types),
            })
        })
        .collect()
}
