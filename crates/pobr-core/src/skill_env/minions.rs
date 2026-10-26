//! Minion wiring semantics (engine-semantics layer): summoning-gem recognition and
//! minion assembly.
//!
//! Moved from `pobr-build`'s `skill/minions.rs`: collects `Minions deal/have …` mods
//! (equipment + extra), wraps them into `MinionModifierEntry`, and spawns each
//! summoned minion into the session's minion ModDb — driven by lookup traits +
//! `SocketGroupView` + a `MinionSession` write surface.

use pobr_data::skill::SkillTypes;
use pobr_data::source::{ModifierSource, SourceId, SourceKind};

use crate::calc::minion::{AttributeInfusion, MinionModifierEntry};
use crate::rules::skill_stats;
use crate::rules::umbral_souls::{self, Soul};
use crate::skill_env::lookup::{
    EffectLookup, MinionLookup, ParserRulesLookup, SocketGroupView, StatMapLookup, StatSetLookup,
};
use crate::skill_env::resolve::{
    GemPropertyLookup, support_granted_gem_levels, support_stat_set_index,
};
use crate::skill_env::support::judge_group_supports;
use crate::skill_env::{GemInput, SupportCandidate};

/// The session write surface [`spawn_minions`] needs: the minion quantity limit's
/// player-side BASE sum, the player's flags, the per-minion spawn sink and the
/// player sink for what replaces a minion.
///
/// `pobr_core::calc::CalculationSession` implements this; the trait exists so minion
/// wiring doesn't depend on the concrete session type.
pub trait MinionSession {
    /// `Sum("BASE", nil, name)` over the player db (the minion limit stat source).
    fn base_sum(&self, name: &str) -> f64;

    /// Whether the player has the flag `name`.
    fn has_flag(&self, name: &str) -> bool;

    /// Whether the configured enemy has the flag `name` (including rarity).
    fn enemy_has_flag(&self, name: &str) -> bool;

    /// The character's final attribute `name` (`Strength`, `Dexterity`), as
    /// its attributes were prepared.
    fn attribute(&self, name: &str) -> f64;

    /// What the character grants the minions of a skill with `skill_types`
    /// (see [`crate::calc::CalculationSession::minion_modifiers_for`]).
    fn minion_modifiers_for(&self, skill_types: SkillTypes) -> Vec<MinionModifierEntry>;

    /// Gives the player modifiers granted instead of a minion.
    fn add_player_modifiers(&mut self, modifiers: Vec<crate::Modifier>);

    /// Registers a buff skill owned by the most recently spawned minion.
    fn add_last_minion_buff_skill(&mut self, spec: crate::calc::BuffSpec);

    /// Spawns one minion from its catalog definition (see
    /// [`crate::calc::CalculationSession::add_minion_from_def`]).
    #[allow(clippy::too_many_arguments)]
    fn add_minion_from_def(
        &mut self,
        def: &pobr_data::minion::MinionDef,
        gem_level: u32,
        limit: u32,
        minion_modifiers: Vec<MinionModifierEntry>,
        ally_buff_mods: Vec<crate::Modifier>,
        infusion: AttributeInfusion,
        origin: crate::calc::MinionOrigin,
    );
}

impl MinionSession for crate::calc::CalculationSession {
    fn base_sum(&self, name: &str) -> f64 {
        crate::calc::CalculationSession::base_sum(self, name)
    }

    fn has_flag(&self, name: &str) -> bool {
        crate::calc::CalculationSession::has_flag(self, name)
    }

    fn enemy_has_flag(&self, name: &str) -> bool {
        crate::calc::CalculationSession::enemy_has_flag(self, name)
    }

    fn attribute(&self, name: &str) -> f64 {
        self.config().stats.get(name).copied().unwrap_or(0.0)
    }

    fn add_player_modifiers(&mut self, modifiers: Vec<crate::Modifier>) {
        self.add_modifiers(modifiers);
    }

    fn add_last_minion_buff_skill(&mut self, spec: crate::calc::BuffSpec) {
        crate::calc::CalculationSession::add_last_minion_buff_skill(self, spec);
    }

    fn minion_modifiers_for(&self, skill_types: SkillTypes) -> Vec<MinionModifierEntry> {
        crate::calc::CalculationSession::minion_modifiers_for(self, skill_types)
    }

    #[allow(clippy::too_many_arguments)]
    fn add_minion_from_def(
        &mut self,
        def: &pobr_data::minion::MinionDef,
        gem_level: u32,
        limit: u32,
        minion_modifiers: Vec<MinionModifierEntry>,
        ally_buff_mods: Vec<crate::Modifier>,
        infusion: AttributeInfusion,
        origin: crate::calc::MinionOrigin,
    ) {
        crate::calc::CalculationSession::add_minion_from_def(
            self,
            def,
            gem_level,
            limit,
            minion_modifiers,
            ally_buff_mods,
            infusion,
            origin,
        );
    }
}

/// The lookup surface [`spawn_minions`] needs.
pub trait MinionEnv:
    MinionLookup
    + EffectLookup
    + StatSetLookup
    + StatMapLookup
    + ParserRulesLookup
    + GemPropertyLookup
    + crate::skill_env::weapon::WeaponItemLookup
{
}
impl<T> MinionEnv for T where
    T: MinionLookup
        + EffectLookup
        + crate::skill_env::weapon::WeaponItemLookup
        + StatSetLookup
        + StatMapLookup
        + ParserRulesLookup
        + GemPropertyLookup
{
}

/// Wires every summoned minion into the session (B3 + #12 companion/support payload).
///
/// - `item_texts` = every equipped item's enchant/implicit/explicit lines (the
///   `Minions deal/have …` scan surface; the caller collects them via
///   `collect_item_texts`), chained with `extra_texts`;
/// - `gem_level_bonuses` = the build's GemProperty Level bonuses (the
///   `additional_gem_levels` input, assembled once by the caller via
///   [`crate::skill_env::gem_property_bonuses`]);
/// - `main_skill` = the selected effect and its zero-based enabled-group index,
///   for summons that exist only when explicitly inspecting that effect.
///
/// Semantics: the mods don't participate in the player's own aggregation in the main
/// flow (the engine produces a `MinionModifier` LIST mod, and LIST doesn't participate
/// in sum/more/flag) — they only enter the minion ModDb here. Each line is run through
/// `parse_mod_engine`, and `extract_minion_modifier_entries` extracts the
/// `MinionModifier` wrapper from the output. Missing rules (an old data pack) = no
/// parser → produces no minion mods (matching the global "rules not injected →
/// everything Unsupported" semantics).
pub fn spawn_minions(
    session: &mut dyn MinionSession,
    groups: &dyn SocketGroupView,
    data: &dyn MinionEnv,
    gem_level_bonuses: &[crate::skill_env::GemPropertyBonus],
    equipment: &dyn crate::skill_env::EquipmentView,
    main_skill: Option<(usize, crate::skill_env::ActiveEffect<'_>)>,
) {
    use std::collections::BTreeSet;

    // Every group's summoning skill creates its own minions, as each is a
    // skill of its own (a Companion's Life is taken before the character's
    // once per group). A kind of minion another group already summons does
    // not add to how many minions the character has.
    let mut seen: BTreeSet<String> = BTreeSet::new();
    // Umbral Well: Skeletal Minions become Souls on the character instead.
    let umbral_well = session.has_flag(umbral_souls::UMBRAL_WELL);
    let facts = skill_stats::SummonedMinions {
        reviving_kinds: reviving_minion_kinds(groups, data),
        player_strength: session.attribute("Strength"),
        player_dexterity: session.attribute("Dexterity"),
    };

    let mut enabled_group = 0;
    groups.for_each_enabled_group(&mut |group| {
        let group_index = enabled_group;
        enabled_group += 1;
        // This group's each **active skill** gem.
        // Prefers `gems` (the real import path, includes both active + support);
        // falls back to `active_skill_id` when empty (the builder/test path's
        // with_active_skill, the same fallback source as resolve_main_skill).
        let candidates: Vec<GemInput> = if group.gems.is_empty() {
            group
                .active_skill_id
                .map(|id| {
                    vec![GemInput {
                        skill_id: id.to_string(),
                        gem_level: group.active_gem_level,
                        global_level_bonus: None,
                        quality: 0,
                        stat_set_index: None,
                        corrupted: false,
                        minion: None,
                        minion_skill: None,
                    }]
                })
                .unwrap_or_default()
        } else {
            group.gems.to_vec()
        };
        let mut candidates: Vec<_> = candidates.into_iter().enumerate().collect();
        let selected = main_skill
            .filter(|(index, _)| *index == group_index)
            .map(|(_, effect)| effect);
        // Selecting Pounce's summon effect previews its minion with
        // the same gem and support provenance as the primary effect.
        if let Some(effect) = selected
            && effect.skill_id == "WolfPounceSummonWolfPlayer"
            && !data.effect_minion_list(effect.skill_id).is_empty()
            && let Some((_, gem)) = candidates.get(effect.gem_index)
            && gem.skill_id != effect.skill_id
        {
            let mut summon = gem.clone();
            summon.skill_id = effect.skill_id.to_string();
            candidates.push((effect.gem_index, summon));
        }
        for (
            gem_index,
            GemInput {
                skill_id,
                gem_level,
                global_level_bonus,
                quality,
                stat_set_index,
                corrupted,
                minion: chosen,
                minion_skill: chosen_skill,
            },
        ) in candidates
        {
            // These wolves appear on the marked enemy's death. They are
            // present only when explicitly inspecting the summon effect,
            // not while calculating Pounce, its Mark, or another skill.
            // An imported skillMinion on the parent gem is a display
            // choice for the summon effect, not a summon by the attack.
            if matches!(
                skill_id.as_str(),
                "WolfPouncePlayer" | "WolfPounceMarkPlayer"
            ) || (skill_id == "WolfPounceSummonWolfPlayer"
                && !selected.is_some_and(|effect| {
                    effect.gem_index == gem_index && effect.skill_id == skill_id
                }))
            {
                continue;
            }
            // A support itself doesn't summon — its addMinionList appends to the active
            // skill's minion_list, which is deferred; the first version only wires up
            // the active skill's minion_list.
            let is_support = data
                .effect(&skill_id)
                .map(|e| e.is_support)
                .unwrap_or(false);
            if is_support {
                continue;
            }
            let minion_ids = summoned_minions(data, &skill_id, chosen.as_deref());
            if minion_ids.is_empty() {
                continue;
            }
            // Manifest Weapon: "Manifest a copy of your main hand Melee
            // Martial Weapon". Without that weapon there is no minion.
            let copies_weapon = data
                .effect(&skill_id)
                .is_some_and(|effect| effect.minion_uses.iter().any(|slot| slot == "Weapon 1"));
            let copied_item = copies_weapon
                .then(|| equipment.item(pobr_data::item::EquipmentSlot::Weapon1))
                .flatten();
            let copied_weapon = copied_item
                .and_then(|item| {
                    crate::skill_env::weapon_item_contribution(item, data.as_weapon_item_lookup())
                })
                .filter(|weapon| {
                    weapon
                        .flags
                        .intersects(pobr_data::modifier::ModFlags::WEAPON_MELEE)
                });
            if copies_weapon && copied_weapon.is_none() {
                continue;
            }
            if umbral_well
                && let Some(soul) = Soul::of_skill(&skill_id)
                && let Some(magnitude) =
                    soul_magnitude(data, &skill_id, gem_level, quality, stat_set_index)
            {
                for minion_id in minion_ids {
                    if !seen.insert(minion_id.clone()) {
                        continue;
                    }
                    let souls = data
                        .minion_def(minion_id)
                        .map_or(1, |def| minion_limit(&*session, def));
                    let origin = ModifierSource::new(SourceId::new(
                        SourceKind::Buff,
                        format!("buff.{}", soul.buff),
                    ))
                    .with_raw_text(format!("{souls} Umbral Soul ({skill_id})"));
                    session.add_player_modifiers(
                        soul.modifiers(magnitude, souls)
                            .into_iter()
                            .map(|modifier| modifier.with_origin(origin.clone()))
                            .collect(),
                    );
                }
                continue;
            }
            // Minion level uses the effective level of the originating gem,
            // including item/passive bonuses and levels granted by supports.
            // A support on any of the gem's effects grants the gem its levels.
            let gem_skill_id = group
                .gems
                .get(gem_index)
                .map_or(skill_id.as_str(), |gem| gem.skill_id.as_str());
            let judgements = super::buff_specs::gem_judgements(&group, data, gem_skill_id);
            let judgement = group_judgement(&group, data, &skill_id);
            let effective_gem_level = gem_level
                .saturating_add(global_level_bonus.unwrap_or_else(|| {
                    crate::skill_env::additional_gem_levels(
                        gem_level_bonuses,
                        data,
                        gem_skill_id,
                        corrupted,
                    )
                }))
                .saturating_add(support_granted_gem_levels(&judgements, group.gems, data));
            // A skill that says how many minions it summons ("Summons {0}
            // Hyenas") creates that many of each kind; any other creates as
            // many as the kind's limit.
            let own_count = own_minion_count(
                data,
                &skill_id,
                effective_gem_level,
                quality,
                stat_set_index,
                session,
            );
            // (#12 companion) Companion determination: the granted skill has
            // `SkillType.Companion` and not `MinionsAreUndamagable` (matching vendor
            // CalcPerform.lua:3365-3367's includeSkill predicate) → this skill's
            // minions count toward `TotalCompanionLife`.
            let is_companion = data.effect(&skill_id).is_some_and(|e| {
                e.skill_types.iter().any(|t| t == "Companion")
                    && !e.skill_types.iter().any(|t| t == "MinionsAreUndamagable")
            });
            // (#12) The minion-side payload of a compatible support in the same group
            // (vendor: a support statmap's `MinionModifier LIST` is merged into the
            // supported skill's skillModList → `addMinionModifiers` injects it into
            // **that skill's** minion modDB, in-group scope). Data channel =
            // `map_minion_life_stat` (the first batch only covers inner Life, e.g.
            // Loyalty's −30% more minion life).
            // What the character grants this skill's minions: every
            // `MinionModifier` from its items, tree, jewels and
            // configuration whose own tags hold for this skill.
            let skill_types = data.effect(&skill_id).map_or(SkillTypes::NONE, |e| {
                e.skill_types
                    .iter()
                    .filter_map(|name| SkillTypes::from_pob2_name(name))
                    .fold(SkillTypes::NONE, |all, one| all | one)
            });
            let mut group_minion_modifiers = session.minion_modifiers_for(skill_types);
            if let Some(item) = copied_item {
                for text in crate::skill_env::item_text::weapon_mod_texts(item) {
                    let text = crate::skill_env::item_text::clean_item_text(text);
                    for kind in ["Fire", "Cold", "Lightning", "Chaos"] {
                        if let Some((min, max)) =
                            crate::skill_env::item_text::parse_adds_with_suffix(
                                &text,
                                &format!("{} damage", kind.to_lowercase()),
                            )
                        {
                            for (bound, value) in [("Min", min), ("Max", max)] {
                                group_minion_modifiers.push(MinionModifierEntry {
                                    inner: crate::Modifier::number(
                                        format!("{kind}Damage{bound}"),
                                        pobr_data::modifier::ModType::Base,
                                        value,
                                    )
                                    .with_origin(stat_origin(
                                        &skill_id,
                                        SourceKind::SkillGem,
                                        "copied_weapon",
                                        value,
                                    )),
                                    minion_type: None,
                                });
                            }
                        }
                    }
                }
            }
            // What of the character's attributes this skill's minions gain:
            // "Minions gain your Strength", Sturdy Ally's "Companions gain
            // your Strength".
            let infusion = AttributeInfusion {
                player_strength: session.attribute("Strength"),
                player_dexterity: session.attribute("Dexterity"),
                strength_added: session.has_flag("StrengthAddedToMinions")
                    || (data
                        .effect(&skill_id)
                        .is_some_and(|e| e.skill_types.iter().any(|t| t == "Companion"))
                        && session.has_flag("StrengthAddedToCompanions")),
                half_strength_added: session.has_flag("HalfStrengthAddedToMinions"),
                dexterity_added: session.has_flag("DexterityAddedToMinions"),
            };
            // What the summoning skill's own stats, and those of the supports
            // that apply to it, grant its minions.
            let own_stats =
                data.effect_stats(&skill_id, effective_gem_level, quality, stat_set_index);
            group_minion_modifiers.extend(stat_minion_modifiers(
                &skill_id,
                SourceKind::SkillGem,
                own_stats.all(),
                facts,
            ));
            for sup in &judgement.compatible {
                let sup_gem = &group.gems[sup.gem_index];
                let set_index = support_stat_set_index(sup, group.gems);
                // Quality passed as 0, matching support_modifiers's semantics (supports have no quality table entries).
                let stats = data.effect_stats(&sup.effect_id, sup_gem.gem_level, 0, set_index);
                group_minion_modifiers.extend(stat_minion_modifiers(
                    &sup.effect_id,
                    SourceKind::SupportGem,
                    stats.all(),
                    facts,
                ));
                let Some(catalog) = data.stat_map_catalog() else {
                    continue;
                };
                let set_key = data.selected_set_key(&sup.effect_id, set_index);
                for ds in stats.all() {
                    if ds.value == 0.0 || skill_stats::is_minion_stat(&ds.stat) {
                        continue;
                    }
                    for inner in crate::rules::stat_map_engine::map_minion_life_stat(
                        catalog,
                        &sup.effect_id,
                        set_key.as_deref(),
                        &ds.stat,
                        ds.value,
                    ) {
                        group_minion_modifiers.push(MinionModifierEntry {
                            inner: inner.with_origin(stat_origin(
                                &sup.effect_id,
                                SourceKind::SupportGem,
                                &ds.stat,
                                ds.value,
                            )),
                            minion_type: None,
                        });
                    }
                }
            }
            let minion_level = crate::calc::minion_level_from_gem_level(effective_gem_level);
            for minion_id in minion_ids {
                let counted = seen.insert(minion_id.clone());
                let Some(def) = data.minion_def(minion_id) else {
                    // minion_list references a minion not in the catalog (the foreign
                    // key is verified to have zero dangling references — defensive
                    // skip, theoretically unreachable).
                    continue;
                };
                // The limit only affects `Multiplier:SummonedMinion` (per-minion mods +
                // DPS aggregation count), not a single minion's life/defence.
                let limit = if counted {
                    let base = own_count.unwrap_or_else(|| minion_limit(&*session, def));
                    // temporary_minion_limit_+: a minion skill with a
                    // duration summons temporary minions, unlike a
                    // persistent reservation with no duration.
                    let extra = if skill_types.contains(SkillTypes::DURATION) {
                        session.base_sum("TemporaryMinionLimit").max(0.0) as u32
                    } else {
                        0
                    };
                    base.saturating_add(extra)
                } else {
                    0
                };
                let def = def.clone();
                let skill = minion_skill(data, &def, chosen_skill, minion_level);
                let mut minion_modifiers = group_minion_modifiers.clone();
                minion_modifiers.extend(minion_skill_modifiers(&group, data, &skill, minion_level));
                // `add_minion_from_def` internally maps gem level to monster level via
                // `minion_level_from_gem_level` (the default rule at
                // CalcActiveSkill.lua:948), so the effective gem level (including the
                // +N to Level bonus) is passed here and must not be pre-resolved
                // (otherwise the mapping would apply twice).
                session.add_minion_from_def(
                    &def,
                    effective_gem_level,
                    limit,
                    minion_modifiers,
                    Vec::new(),
                    infusion,
                    crate::calc::MinionOrigin {
                        summoner: Some((group_index, gem_index)),
                        summoned_by: skill_id.clone(),
                        is_companion,
                        skill,
                        picked_monster: picks_a_monster(data, &skill_id),
                        copied_weapon: copied_weapon.map(|weapon| crate::calc::WeaponBase {
                            hit_min: weapon.phys_min,
                            hit_max: weapon.phys_max,
                            attack_rate: Some(weapon.attack_rate),
                            crit_chance: weapon.crit_chance,
                            flags: weapon.flags,
                        }),
                    },
                );
                let curses_enabled = if is_companion {
                    session.has_flag("CompanionCursesEnabled")
                } else if picks_a_monster(data, &skill_id) {
                    session.has_flag("SpectreCursesEnabled")
                } else {
                    false
                };
                if curses_enabled {
                    for spec in
                        super::buff_specs::minion_curse_specs(data, &group, &def, minion_level)
                    {
                        session.add_last_minion_buff_skill(spec);
                    }
                }
            }
        }
    });
}

/// How many different kinds of Reviving Minion the character summons: the
/// distinct skills of the `Minion` and `Persistent` types among its enabled
/// groups.
fn reviving_minion_kinds(groups: &dyn SocketGroupView, data: &dyn MinionEnv) -> u32 {
    let mut kinds = std::collections::BTreeSet::new();
    groups.for_each_enabled_group(&mut |group| {
        for gem in group.gems {
            let Some(effect) = data.effect(&gem.skill_id) else {
                continue;
            };
            let has = |name: &str| effect.skill_types.iter().any(|t| t == name);
            if !effect.is_support && has("Minion") && has("Persistent") {
                kinds.insert(gem.skill_id.clone());
            }
        }
    });
    kinds.len() as u32
}

/// Where a stat of the skill `effect_id` came from.
fn stat_origin(effect_id: &str, kind: SourceKind, stat: &str, value: f64) -> ModifierSource {
    ModifierSource::new(SourceId::new(kind, format!("minion.{effect_id}.{stat}")))
        .with_raw_text(format!("minion {effect_id} {stat} ({value})"))
}

/// What the stats `stats` of the skill `effect_id` grant its minions
/// ([`skill_stats::minion_modifiers`]).
fn stat_minion_modifiers<'a>(
    effect_id: &str,
    kind: SourceKind,
    stats: impl Iterator<Item = &'a pobr_data::catalog::SkillDamageStat>,
    facts: skill_stats::SummonedMinions,
) -> Vec<MinionModifierEntry> {
    let mut entries = Vec::new();
    for ds in stats {
        if ds.value == 0.0 {
            continue;
        }
        let modifiers =
            if ds.stat == "non_skill_base_physical_damage_%_to_convert_to_random_element" {
                // On a summoning skill this is its minions' conversion (Azmerian Swarm).
                Some(vec![crate::Modifier::number(
                    "PhysicalDamageConvertToRandom",
                    pobr_data::modifier::ModType::Base,
                    ds.value,
                )])
            } else {
                skill_stats::minion_modifiers(&ds.stat, ds.value, facts)
            };
        let Some(modifiers) = modifiers else {
            continue;
        };
        for modifier in modifiers {
            entries.push(MinionModifierEntry {
                inner: modifier.with_origin(stat_origin(
                    effect_id,
                    kind.clone(),
                    &ds.stat,
                    ds.value,
                )),
                minion_type: None,
            });
        }
    }
    entries
}

/// The skill a minion of `def` uses: the one the player chose (`chosen`,
/// counting from 1 among the monster's skills the data has, the last when
/// it is past them), else the first.
fn minion_skill(
    data: &dyn MinionEnv,
    def: &pobr_data::minion::MinionDef,
    chosen: Option<u32>,
    minion_level: u32,
) -> crate::calc::MinionSkill {
    let skills: Vec<(&String, &pobr_data::catalog::GrantedEffectDef)> = def
        .skill_list
        .iter()
        .filter_map(|id| data.effect(id).map(|effect| (id, effect)))
        .collect();
    let index = chosen.unwrap_or(1).clamp(1, skills.len().max(1) as u32) as usize - 1;
    skills
        .get(index)
        .map(|(id, effect)| crate::calc::MinionSkill {
            id: (*id).clone(),
            types: effect.skill_types.clone(),
            cast_time: effect.cast_time.map_or(0.0, |ms| f64::from(ms) / 1000.0),
            damage_multiplier: {
                let level = minion_skill_level(data, id, minion_level);
                data.selected_set_level_row(id, level, None)
                    .map(|row| row.damage_multiplier)
                    .or_else(|| {
                        data.effect_level_row(id, level)
                            .and_then(|row| row.base_multiplier)
                    })
            },
            command: data
                .effect_stats(id, 1, 0, None)
                .all()
                .any(|stat| stat.stat == skill_stats::COMMAND_SKILL_STAT),
        })
        .unwrap_or_default()
}

/// The level of the minion skill `skill_id` a minion of `minion_level`
/// uses: the highest whose level requirement it meets, the first when the
/// data gives no requirements.
pub(super) fn minion_skill_level(data: &dyn MinionEnv, skill_id: &str, minion_level: u32) -> u32 {
    let mut level = 1;
    for next in 2..=100 {
        match data.effect_level_row(skill_id, next) {
            Some(row)
                if row.level == next
                    && row.level_requirement.is_some_and(|req| req <= minion_level) =>
            {
                level = next;
            }
            _ => break,
        }
    }
    level
}

/// What a minion's own skill gets: the stats of that skill at the minion's
/// level, its cooldown and stored uses, a spell's critical hit chance, and
/// the stats of the supports in the summoning skill's group that can
/// support it. A support on a minion gem supports the skills its minions use
/// ("Supported Skills have 25% increased Attack Speed" is the minions'
/// attack speed), judged by the minion skill's types; the minion skill is
/// not a gem's, so a support for gems only does not apply.
fn minion_skill_modifiers(
    group: &crate::skill_env::EnabledGroup<'_>,
    data: &dyn MinionEnv,
    skill: &crate::calc::MinionSkill,
    minion_level: u32,
) -> Vec<MinionModifierEntry> {
    if !skill.is_known() {
        return Vec::new();
    }
    let mut ctx = crate::skill_env::StatMapCtx::data(data.stat_map_catalog());
    let level = minion_skill_level(data, &skill.id, minion_level);
    let own: Vec<_> = data
        .effect_stats_at_user_level(&skill.id, level, minion_level)
        .all()
        .cloned()
        .collect();
    let set_key = data.selected_set_key(&skill.id, None);
    // An attack crits at the monster's weapon's chance.
    let row = data.effect_level_row(&skill.id, level);
    let mut modifiers = crate::skill_env::skill_base_modifiers(
        &skill.id,
        row.and_then(|row| row.cooldown_ms)
            .map(|ms| f64::from(ms) / 1000.0),
        row.and_then(|row| row.stored_uses),
        None,
        row.and_then(|row| row.crit_chance)
            .filter(|_| !skill.uses_weapon()),
        data.selected_set_level_row(&skill.id, level, None)
            .and_then(|row| row.skill_attack_speed_more),
    );
    if skill.uses_weapon()
        && let Some(speed) = row.and_then(|row| row.attack_speed_multiplier)
    {
        modifiers.push(
            crate::Modifier::number("AttackSpeed", pobr_data::modifier::ModType::More, speed)
                .with_origin(stat_origin(
                    &skill.id,
                    SourceKind::SkillGem,
                    "attack_speed_multiplier",
                    speed,
                )),
        );
    }
    modifiers.extend(crate::skill_env::mapped_stat_modifiers(
        &mut ctx,
        &own,
        SourceKind::SkillGem,
        &skill.id,
        &skill.id,
        set_key.as_deref(),
    ));
    let judgement = group_judgement_from(group, data, &skill.id, false);
    for sup in &judgement.compatible {
        let gem = &group.gems[sup.gem_index];
        let set_index = support_stat_set_index(sup, group.gems);
        let stats: Vec<_> = data
            .effect_stats(&sup.effect_id, gem.gem_level, 0, set_index)
            .all()
            .cloned()
            .collect();
        let set_key = data.selected_set_key(&sup.effect_id, set_index);
        modifiers.extend(crate::skill_env::mapped_stat_modifiers(
            &mut ctx,
            &stats,
            SourceKind::SupportGem,
            &sup.effect_id,
            &sup.effect_id,
            set_key.as_deref(),
        ));
    }
    modifiers
        .into_iter()
        .filter(|modifier| !matches!(modifier.name.as_str(), "MinionModifier" | "CurseEffect"))
        .map(|inner| MinionModifierEntry {
            inner,
            minion_type: None,
        })
        .collect()
}

/// Supports that apply to a summoned minion's skills also charge their cost
/// multiplier to the summoning skill. Keep their offensive payload off the
/// summoner. The CreatesMinion marker lets Minion Splash require both a minion
/// source and a melee strike; other skill types belong to the minion itself.
pub(super) fn minion_supports_for_cost(
    group: &crate::skill_env::EnabledGroup<'_>,
    data: &(impl MinionLookup + EffectLookup + ?Sized),
    skill_id: &str,
) -> Vec<crate::skill_env::CompatibleSupport> {
    let mut seen = std::collections::HashSet::new();
    minion_support_judgements(group, data, skill_id)
        .into_iter()
        .flat_map(|judgement| judgement.compatible)
        .filter(|support| seen.insert((support.gem_index, support.effect_id.clone())))
        .collect()
}

/// A support can modify a summoning gem through any of its minion's skills.
/// Keep the individual judgements for gem properties too: Uhtred's support
/// count includes Chain on a Skeletal Sniper's projectile attack.
pub(super) fn minion_support_judgements(
    group: &crate::skill_env::EnabledGroup<'_>,
    data: &(impl MinionLookup + EffectLookup + ?Sized),
    skill_id: &str,
) -> Vec<crate::skill_env::GroupSupportJudgement> {
    use std::collections::HashSet;

    let Some(summon) = data.effect(skill_id).filter(|effect| {
        !effect.cannot_be_supported && effect.skill_types.iter().any(|t| t == "CreatesMinion")
    }) else {
        return Vec::new();
    };
    let summon_types: HashSet<_> = summon.skill_types.iter().cloned().collect();
    let chosen = group
        .gems
        .iter()
        .find(|gem| {
            gem.skill_id == skill_id
                || data
                    .additional_effects(&gem.skill_id)
                    .iter()
                    .any(|id| id == skill_id)
        })
        .and_then(|gem| gem.minion.as_deref());
    let mut candidates: Vec<_> = group
        .gems
        .iter()
        .enumerate()
        .flat_map(|(gem_index, gem)| {
            std::iter::once(gem.skill_id.as_str())
                .chain(
                    data.additional_effects(&gem.skill_id)
                        .iter()
                        .map(String::as_str),
                )
                .filter_map(move |effect_id| {
                    data.effect(effect_id)
                        .filter(|effect| effect.is_support)
                        .map(|effect| SupportCandidate {
                            gem_index,
                            effect_id,
                            effect,
                        })
                })
        })
        .collect();
    // Minion types cannot bypass the support's restriction on borrowing them
    // (Bursting Plague), explicit minion exclusion (Clash), or exclusions on
    // undamageable summons. Direct summoner compatibility is handled separately.
    candidates.retain(|support| {
        !support.effect.ignore_minion_types
            && !crate::rules::skill_type_expr::matches(
                &support.effect.exclude_skill_types,
                &summon_types,
            )
    });
    let mut judgements = Vec::new();
    for minion in summoned_minions(data, skill_id, chosen) {
        let Some(def) = data.minion_def(minion) else {
            continue;
        };
        for id in &def.skill_list {
            let Some(effect) = data.effect(id) else {
                continue;
            };
            let mut types: HashSet<_> = effect.skill_types.iter().cloned().collect();
            types.insert("CreatesMinion".into());
            judgements.push(judge_group_supports(
                &types,
                effect.cannot_be_supported,
                false,
                &candidates,
            ));
        }
    }
    judgements
}

/// Whether the skill `skill_id` makes a monster the player picks its minion:
/// a Spectre, or a tamed beast.
fn picks_a_monster(data: &(impl EffectLookup + ?Sized), skill_id: &str) -> bool {
    data.effect(skill_id).is_some_and(|effect| {
        matches!(
            effect.active_skill.as_deref(),
            Some("summon_beast" | "summon_spectre")
        )
    })
}

/// The minions the skill `skill_id` summons. A Spectre's or a tamed beast's
/// gem summons the monster the player chose for it (`chosen`, PoB's `<Gem
/// skillMinion>`), as does a skill with no minions of its own; any other
/// skill summons its own minions, whichever one the player shows.
fn summoned_minions<'a>(
    data: &'a (impl MinionLookup + EffectLookup + ?Sized),
    skill_id: &str,
    chosen: Option<&'a str>,
) -> Vec<&'a String> {
    let own = data.effect_minion_list(skill_id);
    match chosen {
        Some(id)
            if (picks_a_monster(data, skill_id) || own.is_empty())
                && data.minion_def(id).is_some() =>
        {
            let def = data.minion_def(id).expect("checked");
            vec![&def.id]
        }
        _ => own.iter().collect(),
    }
}

/// How much one Umbral Soul of the minions of the skill `skill_id` grants,
/// when its minions can become Souls.
fn soul_magnitude(
    data: &dyn MinionEnv,
    skill_id: &str,
    level: u32,
    quality: u32,
    set_index: Option<u32>,
) -> Option<f64> {
    let magnitude: f64 = data
        .effect_stats(skill_id, level, quality, set_index)
        .all()
        .filter(|ds| ds.stat == umbral_souls::SOUL_MAGNITUDE)
        .map(|ds| ds.value)
        .sum();
    (magnitude > 0.0).then_some(magnitude)
}

/// How many minions of each kind the skill `skill_id` creates at `level`,
/// when one of its stats says ([`skill_stats::is_summoned_minion_count`]).
fn own_minion_count(
    data: &dyn MinionEnv,
    skill_id: &str,
    level: u32,
    quality: u32,
    set_index: Option<u32>,
    session: &dyn MinionSession,
) -> Option<u32> {
    // The game's skill_cross_slash_triggered_summon_+_wolves stats add
    // wolves to the one summoned per activation: +0 for a Rare, +1 for
    // a Unique. number_of_wolves_allowed is the cap, not the spawn count.
    if skill_id == "WolfPounceSummonWolfPlayer" {
        let rarity_stat = if session.enemy_has_flag("Condition:Unique") {
            "skill_cross_slash_triggered_summon_+_wolves_if_unique"
        } else if session.enemy_has_flag("Condition:RareOrUnique") {
            "skill_cross_slash_triggered_summon_+_wolves_if_rare"
        } else {
            return Some(1);
        };
        let extra: f64 = data
            .effect_stats(skill_id, level, quality, set_index)
            .all()
            .filter(|ds| ds.stat == rarity_stat)
            .map(|ds| ds.value)
            .sum();
        return Some((1.0 + extra).max(0.0) as u32);
    }
    let mut count = None;
    let mut limit_bonus = 0.0;
    for ds in data.effect_stats(skill_id, level, quality, set_index).all() {
        if skill_stats::is_summoned_minion_count(&ds.stat) {
            *count.get_or_insert(0.0) += ds.value;
            // This stat is a base cap, unlike a pack size or spawn count.
            // Add the completed player's named bonus once, even if both the
            // skill and its quality contribute to the native cap.
            if ds.stat == "base_number_of_zombies_allowed" {
                limit_bonus = session.base_sum("ActiveZombieLimit");
            }
        }
    }
    count.map(|total: f64| (total + limit_bonus).max(0.0) as u32)
}

/// How many of the minion `def` its skill creates: the player's BASE sum of
/// the minion's limit stat (e.g. `ActiveZombieLimit`), at least one.
fn minion_limit(session: &dyn MinionSession, def: &pobr_data::minion::MinionDef) -> u32 {
    let limit_stat = def.limit.to_pob2_str();
    if limit_stat.is_empty() {
        return 1;
    }
    let summed = session.base_sum(limit_stat);
    if summed >= 1.0 { summed as u32 } else { 1 }
}

/// Runs the group support judgement for `skill_id` over an [`EnabledGroup`] (the
/// engine-semantics counterpart of `pobr-build`'s `judge_group_supports` wrapper):
/// assembles candidates from the group's gems + additional granted effects.
fn group_judgement(
    group: &crate::skill_env::EnabledGroup<'_>,
    data: &dyn MinionEnv,
    skill_id: &str,
) -> crate::skill_env::GroupSupportJudgement {
    group_judgement_from(group, data, skill_id, group.from_gem)
}

/// [`group_judgement`] for a skill that does (`from_gem`) or does not come
/// from a gem.
pub(super) fn group_judgement_from(
    group: &crate::skill_env::EnabledGroup<'_>,
    data: &dyn MinionEnv,
    skill_id: &str,
    from_gem: bool,
) -> crate::skill_env::GroupSupportJudgement {
    use std::collections::HashSet;

    let active_skill_types: HashSet<String> = data
        .effect(skill_id)
        .map(|e| e.skill_types.iter().cloned().collect())
        .unwrap_or_default();
    let cannot_be_supported = data.effect(skill_id).is_some_and(|e| e.cannot_be_supported);

    let candidates: Vec<SupportCandidate<'_>> = group
        .gems
        .iter()
        .enumerate()
        .flat_map(|(i, g)| {
            std::iter::once(g.skill_id.as_str())
                .chain(
                    data.additional_effects(&g.skill_id)
                        .iter()
                        .map(String::as_str),
                )
                .filter_map(move |id| {
                    data.effect(id)
                        .filter(|e| e.is_support)
                        .map(|effect| SupportCandidate {
                            gem_index: i,
                            effect_id: id,
                            effect,
                        })
                })
        })
        .collect();

    judge_group_supports(
        &active_skill_types,
        cannot_be_supported,
        from_gem,
        &candidates,
    )
}
