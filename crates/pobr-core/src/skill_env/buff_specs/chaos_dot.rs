//! The named Chaos DoTs that enabled skills apply for Dark Effigy's
//! selected target. Presence travels with the existing debuff/curse
//! payload, so curse rejection, limits and duplicate selection still
//! decide what actually reaches the enemy.

use pobr_data::modifier::ModType;
use pobr_data::source::{ModifierSource, SourceId, SourceKind};

use super::{BuffEnv, group_judgement};
use crate::Modifier;
use crate::calc::dark_effigy::DOT;
use crate::skill_env::{EffectStats, EnabledGroup, GemInput, support_stat_set_index};

fn marker(kind: &str, skill_id: &str, stat: &str, source: SourceKind) -> Modifier {
    // A LIST of stable identities, not a numeric multiplier: effect
    // scaling cannot multiply a type, nor may this change curse strength.
    Modifier::text(DOT, ModType::List, kind).with_origin(
        ModifierSource::new(SourceId::new(source, format!("skill.{skill_id}.{stat}")))
            .with_raw_text(format!("{skill_id}: {stat} applies the {kind} Chaos DoT")),
    )
}

fn prohibits_chaos(stats: &EffectStats) -> bool {
    stats.all().any(|s| {
        s.value != 0.0
            && matches!(
                s.stat.as_str(),
                "base_deal_no_damage" | "base_deal_no_chaos_damage"
            )
    })
}

fn supports_prohibit_chaos(group: &EnabledGroup<'_>, data: &dyn BuffEnv, skill_id: &str) -> bool {
    group_judgement(group, data, skill_id)
        .compatible
        .iter()
        .any(|support| {
            let gem = &group.gems[support.gem_index];
            prohibits_chaos(&data.effect_stats(
                &support.effect_id,
                gem.gem_level,
                gem.quality,
                support_stat_set_index(support, group.gems),
            ))
        })
}

pub(super) fn skill(
    group: &EnabledGroup<'_>,
    data: &dyn BuffEnv,
    gem: &GemInput,
    skill_id: &str,
    level: u32,
) -> Option<Modifier> {
    if !group.active_in_weapon_set {
        return None;
    }
    // These skills directly apply distinct Chaos DoTs. Do not infer one
    // from just a Chaos tag (Withered/Despair do not deal damage), or from
    // a triggered support's child effect appearing in the gem list.
    let kind = match skill_id {
        "ContagionPlayer" => "Contagion",
        "EssenceDrainPlayer" => "EssenceDrain",
        "SoulrendPlayer" => "Soulrend",
        "ProfaneRitualPlayer" => "ProfaneRitual",
        _ => return None,
    };
    let selected = data.effect_stats(skill_id, level, gem.quality, gem.stat_set_index);
    // A compatible Brutality prohibits this skill's Chaos DoT as well
    // as its hit. Presence is not just a gem-name or skill-tag lookup.
    if prohibits_chaos(&selected) || supports_prohibit_chaos(group, data, skill_id) {
        return None;
    }
    let stat = "base_chaos_damage_to_deal_per_minute";
    // Selecting ED's projectile versus its DoT only changes which panel
    // is calculated: both apply the same debuff. Read the other sets only
    // for this known effect, never import their damage into the main skill.
    let has_dot = selected.all().any(|s| s.stat == stat && s.value > 0.0)
        || data
            .unselected_set_stats(skill_id, level, gem.quality, gem.stat_set_index)
            .iter()
            .flat_map(|set| &set.stats)
            .any(|s| s.stat == stat && s.value > 0.0);
    has_dot.then(|| marker(kind, skill_id, stat, SourceKind::SkillGem))
}

pub(super) fn curse(
    group: &EnabledGroup<'_>,
    data: &dyn BuffEnv,
    skill_id: &str,
    level: u32,
) -> Option<Modifier> {
    if !group.active_in_weapon_set {
        return None;
    }
    // Native Decaying Hex: "Supported Skills trigger Profane Decay ...".
    // Attach to the compatible curse, not an unconditional extra Debuff:
    // the curse that survives the existing slot/strongest-copy selection
    // is the one that supplies Decay in this static snapshot.
    for support in group_judgement(group, data, skill_id).compatible {
        let gem = &group.gems[support.gem_index];
        let stats = data.effect_stats(
            &support.effect_id,
            gem.gem_level,
            gem.quality,
            support_stat_set_index(&support, group.gems),
        );
        let stat = "curse_triggers_decaying_hex";
        if stats.all().any(|s| s.stat == stat && s.value > 0.0) {
            // The curse itself deals no damage; the triggered child does.
            // Its own compatible supports can prohibit that Chaos damage.
            let child = "TriggeredDecayingHexPlayer";
            let decay = data.effect_stats(child, level, gem.quality, None);
            if prohibits_chaos(&decay)
                || supports_prohibit_chaos(group, data, child)
                || !decay
                    .all()
                    .any(|s| s.stat == "base_chaos_damage_to_deal_per_minute" && s.value > 0.0)
            {
                return None;
            }
            return Some(marker(
                "Decay",
                &support.effect_id,
                stat,
                SourceKind::SupportGem,
            ));
        }
    }
    None
}
