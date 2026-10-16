//! Buff/spirit semantics (engine-semantics layer): herald name enumeration, buff name
//! derivation, Mark self-offensive buffs, and Spirit reservation aggregation.
//!
//! Moved from `pobr-build`'s `skill/buffs.rs`. The context-dependent spec builders
//! (`buff_skill_specs`/`support_buff_specs`/`warcry_skill_specs`) stay in the
//! orchestrator for now because they emit `Compare`-mode observation records through
//! `CalculationContext`.

use pobr_data::modifier::ModType;
use pobr_data::source::{ModifierSource, SourceId, SourceKind};

use crate::Modifier;
use crate::skill_env::buff_stat_map::map_self_buff_offensive_stat;
use crate::skill_env::lookup::{EffectLookup, MinionLookup, SocketGroupView, StatSetLookup};
use crate::skill_env::mods::GemPropertyBonus;
use crate::skill_env::mods::skill_type_bits;
use crate::skill_env::resolve::GemPropertyLookup;

/// The buff display names of all **herald active skills** among the enabled groups
/// (deduplicated by name, deterministically sorted).
///
/// Matches vendor (CalcPerform.lua:1792-1805): walks activeSkillList, and for every
/// `skillTypes[SkillType.Herald]` skill whose skillName hasn't been counted yet, records
/// its name into heraldList. The display name is derived from [`buff_skill_name`]'s
/// snake_case form, with connector words (of/the) kept lowercase to match vendor's
/// `buff.name:gsub(" ","")` condition naming ("Herald of Plague" →
/// `AffectedByHeraldofPlague`, matching the shape of oracle condVars).
pub fn herald_skill_names(groups: &dyn SocketGroupView, data: &dyn EffectLookup) -> Vec<String> {
    use std::collections::BTreeSet;
    let mut names: BTreeSet<String> = BTreeSet::new();
    groups.for_each_enabled_group(&mut |group| {
        for gem in group.gems {
            let Some(effect) = data.effect(&gem.skill_id) else {
                continue;
            };
            if effect.is_support || !effect.skill_types.iter().any(|t| t == "Herald") {
                continue;
            }
            let name = buff_skill_name(data, &gem.skill_id)
                .split(' ')
                .map(|w| {
                    if w.eq_ignore_ascii_case("of") || w.eq_ignore_ascii_case("the") {
                        w.to_ascii_lowercase()
                    } else {
                        w.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            names.insert(name);
        }
    });
    names.into_iter().collect()
}

/// The name of the buff the skill `skill_id` gives: the name the game shows
/// for it (`displayed_name`, "Sniper's Mark"; used for the `AffectedBy<name
/// with spaces stripped>` condition and the curse priority `curse_base`
/// lookup key). A name with a placeholder (a Companion's "Companion: {0}"),
/// or a snapshot without displayed names, falls back to the `active_skill`'s
/// snake_case name (`temporal_chains` → `Temporal Chains`), and that to the
/// granted effect id.
pub fn buff_skill_name(data: &dyn EffectLookup, skill_id: &str) -> String {
    let effect = data.effect(skill_id);
    if let Some(name) = effect
        .and_then(|e| e.displayed_name.as_deref())
        .filter(|name| !name.contains('{'))
    {
        return name.to_string();
    }
    let snake = effect.and_then(|e| e.active_skill.as_deref());
    let Some(snake) = snake else {
        return skill_id.to_string();
    };
    snake
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Maps the **self offensive buff** every **enabled gem** grants the player (the
/// gain-as-extra on a Mark trigger), via [`map_self_buff_offensive_stat`], into
/// SkillGem-attributed `DamageGainAs<Type>` BASE modifiers.
///
/// Matches PoB2's `mod("DamageGainAs<Type>","BASE",{type="GlobalEffect",effectType="Buff"})`:
/// a buff triggered by a Mark hit applies to self, and under default config is folded
/// unconditionally into the main skill's gain matrix. Data-driven, zero hardcoding by
/// gem name — the buff's identity is determined by the stat name's semantics
/// (`*_damage_buff_damage_%_to_gain_as_<type>`). This buff is a **global** self-effect,
/// so it iterates every enabled socket group's gems, deduplicated by id to avoid
/// double injection.
pub fn self_buff_offensive_modifiers(
    groups: &dyn SocketGroupView,
    data: &dyn StatSetLookup,
) -> Vec<Modifier> {
    use std::collections::HashSet;
    let mut mods = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    groups.for_each_enabled_group(&mut |group| {
        for gem in group.gems {
            if !seen.insert(gem.skill_id.clone()) {
                continue;
            }
            // The quality segment is folded into the value (matching the aura path's semantics); finer-grained GemQuality attribution is deferred.
            let es = data.effect_stats(
                &gem.skill_id,
                gem.gem_level,
                gem.quality,
                gem.stat_set_index,
            );
            for ds in es.all() {
                let Some(mapped) = map_self_buff_offensive_stat(&ds.stat) else {
                    continue;
                };
                if ds.value == 0.0 {
                    continue;
                }
                let origin = ModifierSource::new(SourceId::new(
                    SourceKind::SkillGem,
                    format!("buff.{}.{}", gem.skill_id, ds.stat),
                ))
                .with_raw_text(format!("buff {} {} ({})", gem.skill_id, ds.stat, ds.value));
                mods.push(
                    Modifier::number(mapped.mod_name.as_str(), mapped.mod_type, ds.value)
                        .with_origin(origin),
                );
            }
        }
    });
    mods
}

/// The read-only `ModDb` surface [`spirit_reservation_modifiers`] aggregates over:
/// flag lookup + inc/more/base sums under a per-gem cfg.
///
/// `pobr_core::ModDb` implements this; the trait exists so the reservation semantics
/// don't depend on the concrete session type.
pub trait ReservationDb {
    /// Whether `flag` is set (vendor `modDB:Flag(nil, "Condition:X")`).
    fn flag(&self, cfg: &crate::CalcConfig, name: &str) -> bool;
    /// `Sum("INC"/"BASE", cfg, names…)` under the per-gem cfg.
    fn sum(
        &self,
        mod_type: ModType,
        cfg: &crate::CalcConfig,
        names: &[pobr_data::prelude::ModName],
    ) -> f64;
    /// `More(cfg, names…)` product under the per-gem cfg.
    fn more(&self, cfg: &crate::CalcConfig, names: &[pobr_data::prelude::ModName]) -> f64;
}

impl ReservationDb for crate::ModDb {
    fn flag(&self, cfg: &crate::CalcConfig, name: &str) -> bool {
        crate::ModDb::flag(self, cfg, name)
    }

    fn sum(
        &self,
        mod_type: ModType,
        cfg: &crate::CalcConfig,
        names: &[pobr_data::prelude::ModName],
    ) -> f64 {
        crate::ModDb::sum(self, mod_type, cfg, names)
    }

    fn more(&self, cfg: &crate::CalcConfig, names: &[pobr_data::prelude::ModName]) -> f64 {
        crate::ModDb::more(self, cfg, names)
    }
}

/// Aggregates the Spirit reservation of every **enabled persistent-reservation effect**
/// into a `SkillSpiritReservationBase` BASE modifier (one per effect, SkillGem
/// attributed), summed by perform's `fill_skill_mechanics` into
/// [`crate::OutputTable`]'s `spirit_reserved`. Overload is only **reported, not
/// blocked**: the requested total is not capped at the Spirit pool. PoB2 caps
/// its displayed `SpiritReserved`, while retaining the uncapped skill rows
/// and a negative `SpiritUnreserved`.
///
/// Semantics (matching PoB2's `CalcDefence.lua:192-249` Reservation section):
/// - Selected = `skill_types` includes `HasReservation` and excludes
///   `ReservationBecomesCost` (`CalcDefence.lua:194`; the latter covers cases like
///   Divine Blessing's "reservation becomes cost");
/// - `flat_total` = the effect's own per-level `spirit_reservation_flat` + the same
///   group's supports' `spirit_reservation_flat` (PoB2's support side injects
///   `ExtraSpirit` BASE, `CalcActiveSkill.lua:698-700`; `CalcDefence.lua:213-214` folds
///   it into baseFlat);
/// - Multiplier = Π(1 + reservation_multiplier/100), covering both the effect's own
///   (`CalcActiveSkill.lua:754-756`) and the same group's supports' (`:692-694`)
///   `ReservationMultiplier` MORE, with the product **truncated to 4 decimal places**
///   (`CalcDefence.lua:197`'s `floor(More("ReservationMultiplier"), 4)`);
/// - Per effect: `reserved = max(round(flat_total × multiplier), 0)` (a subset of
///   `CalcDefence.lua:246-249` — the `Reserved`/`ReservationEfficiency` inc/more mod
///   family and the Spirit pool's own value/unreserved are handled elsewhere, decided
///   §4-12).
///
/// The same effect appearing in multiple groups is deduplicated by id (matching
/// `aura_buff_modifiers`'s semantics). A group's
/// [`crate::skill_env::EnabledGroup::reservation_free_skill`] is left out before that: it reserves
/// nothing, and a gem of the same skill in another group still reserves. Support
/// contributions are currently taken in
/// full per group (to be tightened along with `support_modifiers`'s semantics once the
/// T3.6 compatible-list merge lands).
/// The lookup surface [`spirit_reservation_modifiers`] needs.
pub trait ReservationLookup:
    StatSetLookup + EffectLookup + GemPropertyLookup + MinionLookup
{
}
impl<T: StatSetLookup + EffectLookup + GemPropertyLookup + MinionLookup> ReservationLookup for T {}

///
/// `character` holds the character's conditions, multipliers and stats, under
/// which each gem's lines are read ("2% increased Reservation Efficiency of
/// Skills per Idol in your Equipment").
pub fn spirit_reservation_modifiers(
    groups: &dyn SocketGroupView,
    data: &dyn ReservationLookup,
    db: &dyn ReservationDb,
    character: &crate::CalcConfig,
    use_alt_quality: bool,
    gem_level_bonuses: &[GemPropertyBonus],
) -> Vec<Modifier> {
    use std::collections::HashSet;
    let mut mods = Vec::new();
    // A skill reserves once however many groups hold it. A Companion or a
    // Spectre reserves once per group and monster: each group's summons its
    // own (`spawn_minions`).
    let mut seen: HashSet<(String, Option<String>, Option<usize>)> = HashSet::new();
    let mut group_ordinal = 0usize;
    // Ancestral Bond (tree node 45202, "Totems reserve N Spirit each", produces an
    // `AncestralBond` FLAG): SummonsTotem skills are pulled into the reservation loop
    // because of it (vendor CalcDefence.lua:197's `isTotemAndAncestralBond`). The flag
    // carries no tag, so any cfg can look it up.
    let ancestral_bond = db.flag(&crate::CalcConfig::new(), "AncestralBond");
    groups.for_each_enabled_group(&mut |group| {
        group_ordinal += 1;
        for gem in group.gems {
            let Some(effect) = data.effect(&gem.skill_id) else {
                continue;
            };
            let has = |t: &str| effect.skill_types.iter().any(|x| x == t);
            let totem_under_bond = ancestral_bond && has("SummonsTotem");
            let picks_a_monster = matches!(
                effect.active_skill.as_deref(),
                Some("summon_beast" | "summon_spectre")
            );
            // A skill an item grants without reservation reserves nothing, and
            // does not stand for a gem of the same skill: that one reserves.
            if group.reservation_free_skill == Some(gem.skill_id.as_str()) {
                continue;
            }
            if effect.is_support
                || !(has("HasReservation") || totem_under_bond)
                || has("ReservationBecomesCost")
            {
                continue;
            }
            // The level the skill is used at: "+N to Level of all Minion
            // Skills" lowers what a minion skill reserves.
            let level = gem.gem_level + gem.global_levels(gem_level_bonuses, data);
            let es = data.effect_stats(&gem.skill_id, level, gem.quality, gem.stat_set_index);
            // Only skills explicitly marked by the game keep reserving when
            // their weapon set is inactive (Blink, Convalescence, etc.). Do
            // this before deduplication so an inactive copy cannot hide one
            // in the current set.
            if !group.active_in_weapon_set
                && !es.all().any(|s| {
                    s.stat == "display_skill_reserves_in_all_weapon_sets" && s.value != 0.0
                })
            {
                continue;
            }
            if !seen.insert((
                gem.skill_id.clone(),
                gem.minion.clone(),
                picks_a_monster.then_some(group_ordinal),
            )) {
                continue;
            }
            let own = data.effect_level_row(&gem.skill_id, level);
            let mut flat = own.and_then(|r| r.spirit_reservation_flat).unwrap_or(0.0);
            // The monster a Spectre or a tamed beast's gem summons sets what
            // it reserves: a Spectre that monster's Spirit, a tamed Companion
            // a share of the Spirit pool that grows with the beast's
            // experience multiplier (`companion_reservation`). Other minion
            // skills reserve what their level does, whichever minion they name.
            let chosen = gem.minion.as_deref().and_then(|id| data.minion_def(id));
            let mut percent = 0.0;
            match (chosen, effect.active_skill.as_deref()) {
                (Some(monster), Some("summon_beast")) => percent = monster.companion_reservation,
                (Some(monster), Some("summon_spectre")) => flat = monster.spectre_reservation,
                _ => {}
            }
            let mut mult = 1.0 + own.and_then(|r| r.reservation_multiplier).unwrap_or(0.0) / 100.0;
            // Spirit→Life reservation conversion (vendor CalcDefence.lua:248-254, added
            // in 0.5.4b; Atziri's Communion support's constant stat
            // `skill_reserves_X_life_permyriad_per_spirit_instead_of_spirit` = 66,
            // SkillStatMap div=100 → every point of Spirit reserved becomes 0.66% Life
            // reserved). When present, this skill's entire Spirit reservation converts
            // to a Life percentage reservation (Spirit set to 0).
            let mut spirit_to_life = 0.0;
            // The same group's supports: spirit flat (ExtraSpirit) + reservation_multiplier MORE.
            for sup in group.gems {
                if data.effect(&sup.skill_id).is_none_or(|e| !e.is_support) {
                    continue;
                }
                if let Some(row) = data.effect_level_row(&sup.skill_id, sup.gem_level) {
                    flat += row.spirit_reservation_flat.unwrap_or(0.0);
                    mult *= 1.0 + row.reservation_multiplier.unwrap_or(0.0) / 100.0;
                }
                spirit_to_life += data
                    .effect_stats(
                        &sup.skill_id,
                        sup.gem_level,
                        sup.quality,
                        sup.stat_set_index,
                    )
                    .all()
                    .filter(|s| {
                        s.stat == "skill_reserves_X_life_permyriad_per_spirit_instead_of_spirit"
                    })
                    .map(|s| s.value / 100.0)
                    .sum::<f64>();
            }
            // Blasphemy's per-curse reservation (vendor CalcDefence.lua:229-239): an
            // `IsBlasphemy` effect adds `blasphemy_base_spirit_reservation_per_socketed_curse`
            // (constant stat = 60) once **per supported curse** (vendor's
            // `supportEffect.isSupporting` count ≙ the same group's AppliesCurse active
            // skill count). 0.5.4b vendor semantics = **first fold into baseFlat, then
            // round once as a whole** (:236-238's
            // `values.baseFlat += flat × instances`; essence-drain gives
            // round(180/1.1)=164, not the old per-instance round(60/1.1)=55×3=165 —
            // pinned at 164 by oracle spiritReservedBreakdown). The supported curse's
            // own reservation is 0 (its levels have no flat, matching vendor — no extra
            // exclusion needed).
            if has("IsBlasphemy") {
                let per_curse: f64 = es
                    .all()
                    .filter(|s| s.stat == "blasphemy_base_spirit_reservation_per_socketed_curse")
                    .map(|s| s.value)
                    .sum();
                let curse_count = group
                    .gems
                    .iter()
                    .filter(|g| {
                        data.effect(&g.skill_id).is_some_and(|e| {
                            !e.is_support && e.skill_types.iter().any(|t| t == "AppliesCurse")
                        })
                    })
                    .count();
                flat += per_curse * curse_count as f64;
            }
            // Reservation efficiency (vendor :240-243/:251's `/(1 + efficiency/100)`,
            // clamped ≥ −100):
            // - the gem's own quality stats `base_reservation_efficiency_+%` /
            //   `base_spirit_reservation_efficiency_+%` (q20 Blasphemy=10%; the latter
            //   is Spirit-pool-scoped, mapped by statmap → SpiritReservationEfficiency,
            //   and both names feed the same `/(1+eff/100)` for Spirit reservation);
            // - a GemlingQuality build additionally stacks the same-named stat from
            //   altQualityStats (Mirage Archer ×2 / Eternal Rage ×0.75, pinned at
            //   oracle gemling 62/23);
            // - tree/item mod families (`Spirit`/bare `ReservationEfficiency` INC,
            //   domain-scoped via `ModTag::SkillTypes` matching — the per-gem cfg
            //   carries this effect's type bits, matching vendor's skillCfg Sum
            //   semantics; "Meta Skills have N% increased Reservation Efficiency" (tree
            //   nodes 42245/63236) applies to Meta effects like Blasphemy/Archmage).
            const EFFICIENCY_STATS: [&str; 2] = [
                "base_reservation_efficiency_+%",
                "base_spirit_reservation_efficiency_+%",
            ];
            let alt_quality = if use_alt_quality {
                data.alt_quality_stats(&gem.skill_id, gem.quality)
            } else {
                Vec::new()
            };
            let quality_eff: f64 = es
                .all()
                .chain(alt_quality.iter())
                .filter(|s| EFFICIENCY_STATS.contains(&s.stat.as_str()))
                .map(|s| s.value)
                .sum();
            let mut gem_cfg =
                crate::CalcConfig::new().with_skill_types(skill_type_bits(&effect.skill_types));
            gem_cfg.conditions.clone_from(&character.conditions);
            gem_cfg.multipliers.clone_from(&character.multipliers);
            gem_cfg.stats.clone_from(&character.stats);
            let eff_names = [
                pobr_data::prelude::ModName::from("SpiritReservationEfficiency"),
                pobr_data::prelude::ModName::from("ReservationEfficiency"),
            ];
            let mod_eff = db.sum(ModType::Inc, &gem_cfg, &eff_names);
            let efficiency = (quality_eff + mod_eff).max(-100.0);
            let eff_more = db.more(&gem_cfg, &eff_names);
            // The reservation amount's inc/more bucket (matching vendor
            // :240-241/:252's `Sum("INC"/More, skillCfg, "SpiritReserved", "Reserved")`;
            // the Tactician ascendancy's "Persistent Buffs have 50% less Reservation"
            // hits this bucket via its Persistent+Buff dual tag).
            // vendor's gate: more ≤ 0 or inc ≤ −100 → reservation is 0.
            let reserved_names = [
                pobr_data::prelude::ModName::from("SpiritReserved"),
                pobr_data::prelude::ModName::from("Reserved"),
            ];
            let res_inc = db.sum(ModType::Inc, &gem_cfg, &reserved_names);
            let res_more = db.more(&gem_cfg, &reserved_names);
            let res_factor = if res_more > 0.0 && res_inc > -100.0 {
                (100.0 + res_inc) / 100.0 * res_more
            } else {
                0.0
            };
            // Mod-side ExtraSpirit (vendor :217's per-skill Sum, folded into baseFlat;
            // e.g. Ancestral Bond's `ExtraSpirit 75 + SkillType(SummonsTotem)` only
            // hits totem skills). A support's data-side flat goes through the
            // level_row path above (the two don't overlap).
            flat += db.sum(
                ModType::Base,
                &gem_cfg,
                &[pobr_data::prelude::ModName::from("ExtraSpirit")],
            );
            // A skill whose minions each reserve (`MultipleReservation`)
            // reserves its base amount once per minion it keeps:
            // `skill_desired_initial_amount` of them, one when the skill does
            // not say. "First {0} minions summoned have no base spirit
            // reservation" (`first_X_minions_have_0_base_spirit_reservation`)
            // takes that many out: Skeletal Warriors keep two and exempt two,
            // so they reserve nothing. The Spirit that supports add is taken
            // as part of the base.
            if has("MultipleReservation") {
                let stat =
                    |id: &str| -> f64 { es.all().filter(|s| s.stat == id).map(|s| s.value).sum() };
                let minions = stat("skill_desired_initial_amount").max(1.0);
                let exempt = stat("first_X_minions_have_0_base_spirit_reservation");
                flat *= (minions - exempt).max(0.0);
            }
            // PoB2 truncates the reservation multiplier product to 4 decimal places before multiplying by base (floor(x, 4)).
            let mult = (mult * 10000.0).floor() / 10000.0;
            // Spirit→Life conversion branch (vendor CalcDefence.lua:248-254 + the
            // per-pool loop's name="Life"): Life.basePercent = Spirit.baseFlat × the
            // per-point conversion rate; the factor switches to the Life pool's names
            // (LifeReserved/Reserved, LifeReservationEfficiency/ReservationEfficiency;
            // the gem quality efficiency term still applies regardless of pool);
            // percentage rounded to 2 places (vendor :312). Produces a
            // `LifeReservedPercent` INC consumed by perform's reservation stage
            // (ritualist example: Eternal Rage 155×0.66×0.9 = 92.07% → LifeReserved
            // 270 / LifeUnreserved 23, matching golden).
            if spirit_to_life > 0.0 {
                let life_reserved_names = [
                    pobr_data::prelude::ModName::from("LifeReserved"),
                    pobr_data::prelude::ModName::from("Reserved"),
                ];
                let l_inc = db.sum(ModType::Inc, &gem_cfg, &life_reserved_names);
                let l_more = db.more(&gem_cfg, &life_reserved_names);
                let l_factor = if l_more > 0.0 && l_inc > -100.0 {
                    (100.0 + l_inc) / 100.0 * l_more
                } else {
                    0.0
                };
                let l_eff_names = [
                    pobr_data::prelude::ModName::from("LifeReservationEfficiency"),
                    pobr_data::prelude::ModName::from("ReservationEfficiency"),
                ];
                let l_eff =
                    (quality_eff + db.sum(ModType::Inc, &gem_cfg, &l_eff_names)).max(-100.0);
                let l_eff_more = db.more(&gem_cfg, &l_eff_names);
                let percent =
                    (flat * spirit_to_life * mult * l_factor / (1.0 + l_eff / 100.0) / l_eff_more
                        * 100.0)
                        .round()
                        / 100.0;
                if percent > 0.0 {
                    let origin = ModifierSource::new(SourceId::new(
                        SourceKind::SkillGem,
                        format!("spirit.{}", gem.skill_id),
                    ))
                    .with_raw_text(format!(
                        "life reservation from spirit {} ({} × {spirit_to_life}%)",
                        gem.skill_id, flat
                    ));
                    mods.push(
                        Modifier::number("LifeReservedPercent", ModType::Inc, percent)
                            .with_origin(origin),
                    );
                }
                continue;
            }
            // A share of the pool, to two places; the shares of all skills
            // add up before the pool's reserved amount is taken.
            let share = (percent * mult * res_factor / (1.0 + efficiency / 100.0) / eff_more
                * 100.0)
                .round()
                / 100.0;
            if share > 0.0 {
                let origin = ModifierSource::new(SourceId::new(
                    SourceKind::SkillGem,
                    format!("spirit.{}", gem.skill_id),
                ))
                .with_raw_text(format!(
                    "spirit reservation {} ({percent}% × {mult})",
                    gem.skill_id
                ));
                mods.push(
                    Modifier::number("SkillSpiritReservationPercent", ModType::Base, share)
                        .with_origin(origin),
                );
            }
            let reserved = (flat * mult * res_factor / (1.0 + efficiency / 100.0) / eff_more)
                .round()
                .max(0.0);
            if reserved <= 0.0 {
                continue;
            }
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::SkillGem,
                format!("spirit.{}", gem.skill_id),
            ))
            .with_raw_text(format!(
                "spirit reservation {} ({} × {})",
                gem.skill_id, flat, mult
            ));
            mods.push(
                Modifier::number("SkillSpiritReservationBase", ModType::Base, reserved)
                    .with_origin(origin),
            );
        }
    });
    mods
}
