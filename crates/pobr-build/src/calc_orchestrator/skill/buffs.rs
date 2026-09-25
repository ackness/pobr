//! buffs — herald/aura/buff specs + spirit reservation (pure migration, no logic change).

use pobr_core::Modifier;
use pobr_core::calc::BuffSpec;

use super::super::context::CalculationContext;
use crate::build::Build;
use crate::build_data::BuildData;

/// The buff display names of all **herald active skills** among the enabled groups
/// (deduplicated by name, deterministically sorted).
///
/// Matches vendor (CalcPerform.lua:1792-1805): walks activeSkillList, and for every
/// `skillTypes[SkillType.Herald]` skill whose skillName hasn't been counted yet, records
/// its name into heraldList. The display name is derived from [`buff_skill_name`]'s
/// snake_case form, with connector words (of/the) kept lowercase to match vendor's
/// `buff.name:gsub(" ","")` condition naming ("Herald of Plague" →
/// `AffectedByHeraldofPlague`, matching the shape of oracle condVars).
pub(crate) fn herald_skill_names(build: &Build, data: &BuildData) -> Vec<String> {
    pobr_core::skill_env::herald_skill_names(build, data)
}

/// Builds every **enabled aura / curse skill** into a [`BuffSpec`] (per the contract),
/// injected via `session.add_buff_skill` and consumed by pobr-core's buff_pass
/// (env_finalize stage 4).
///
/// Classification rule (§2.4 contract 1):
/// - `skill_types` includes `Aura` → [`BuffKind::Aura`], with mods = the defensive
///   buffs mapped by [`map_aura_buff_stat`] (the same fetch/attribution semantics as
///   the static direct injection `aura_buff_modifiers` had before the C5 switch — the
///   dual-run already proved both channels are value-equal for the same source);
/// - `skill_types` includes a Mark/Curse-family token (`Mark` / `AppliesCurse`,
///   confirmed against the actual token expression list) → [`BuffKind::Curse`]
///   (`is_mark` = includes `Mark`). Curse-carrying mods: the curse payload stats in the
///   granted_effect statset are mapped, through the statmap data channel
///   ([`stat_map_engine::map_curse_stat`], the `GlobalEffect effectType=Curse` entries
///   of each vendor curse statSet), into **enemy-side** modifiers, written to the enemy
///   db by buff_pass's curse path after applying the CurseEffect multiplier zone
///   (CalcPerform.lua:2286-2316 / :2969-2984). Curse payload stats that can't be mapped
///   land in a visibility report via Compare mode ([`curse_stat_modifiers`]).
/// - Every other active skill: if its statset stats map, via
///   [`debuff_stat_modifiers`] (the debuff domain's `GlobalEffect effectType=Debuff`),
///   into a nonempty enemy-side payload → [`BuffKind::Debuff`] (vendor's buff loop walks
///   **every** activeSkillList entry, CalcPerform.lua:1847 / the Debuff branch
///   :2219-2285 — non-main skills also inject against the enemy). In the same scan, if
///   it maps, via [`player_buff_stat_modifiers`] (the buff domain's
///   `GlobalEffect effectType=Buff`), into a nonempty **player-side** payload →
///   [`BuffKind::Buff`] (vendor's Buff branch :1949-1962; typically an item-granted
///   skill like Pinnacle of Power's `<El>Can<Ailment>` flag family + the numeric
///   allowlist). Both kinds of payload can be produced at once (vendor's buffList also
///   allows mixing).
///
/// `slot` = the socket group's raw slot name (the PoB XML `slot` attr, e.g. `Weapon 1`,
/// the same source as curse_priority.json's slot weight key); `socket_index` = the
/// gem's ordinal in the group (1-based, matching vendor's `ipairs(gemList)` order).
/// The same effect appearing in multiple groups is deduplicated by id (matching the
/// existing injection semantics).
pub(crate) fn buff_skill_specs(
    context: &mut CalculationContext,
    build: &Build,
    data: &BuildData,
) -> Vec<BuffSpec> {
    let bonuses = super::resolve::gem_property_bonuses(build, data);
    let mut ctx = context.stat_map_ctx();
    pobr_core::skill_env::buff_skill_specs(&mut ctx, build, data, &bonuses)
}

/// The **player-side buff** granted by a support → [`BuffSpec`] (kind =
/// [`BuffKind::Buff`], applied by buff_pass's Buff branch,
/// CalcPerform.lua:1949-1962, which applies the BuffEffect multiplier zone before
/// merging into the player db).
///
/// Vendor semantics: a support like Precision I/II's (`sup_dex.lua:4181-4250`) own
/// statSet's statMap produces a `GlobalEffect effectType=Buff` mod (e.g.
/// `support_precision_accuracy_rating_+%` → `Accuracy INC`, feeding into
/// CalcOffence.lua:2557's accuracy aggregation), which applies to the player as the
/// supported Persistent Buff skill (Herald/Malice/Banner…) activates. Applicability is
/// data-driven: [`judge_group_supports`] (require_skill_types =
/// `Persistent+Buff+AND`'s four-stage judgement) is checked against every enabled
/// active skill in the group, and the support is injected if any is compatible; the
/// same support effect appearing in multiple groups is deduplicated by id (buff_pass's
/// mergeBuff falls back to "same-name keeps the stronger" as a backstop).
///
/// Data fetching goes through the statmap buff domain's data channel
/// ([`player_buff_stat_modifiers`], the player-side allowlist's first batch =
/// `Accuracy`); a support with no buff payload (the vast majority) produces empty mods
/// → skipped. Simplification: BuffSpec.name uses [`buff_skill_name`] (a support has no
/// active_skill → falls back to the effect id), while vendor uses statMap's
/// effectName (this only affects `AffectedBy<name>` condition naming, which has no
/// consumer currently).
pub(crate) fn support_buff_specs(
    context: &mut CalculationContext,
    build: &Build,
    data: &BuildData,
) -> Vec<BuffSpec> {
    let mut ctx = context.stat_map_ctx();
    pobr_core::skill_env::support_buff_specs(&mut ctx, build, data)
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
/// so it iterates every enabled socket group's gem_skills, deduplicated by id to avoid
/// double injection.
pub(crate) fn self_buff_offensive_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    pobr_core::skill_env::self_buff_offensive_modifiers(build, data)
}

/// Aggregates the Spirit reservation of every **enabled persistent-reservation effect**
/// into a `SkillSpiritReservationBase` BASE modifier (one per effect, SkillGem
/// attributed), summed by perform's `fill_skill_mechanics` into
/// [`pobr_core::OutputTable`]'s `spirit_reserved`. Overload is only **reported, not
/// blocked** (matching PoB2: it's calculated and highlighted red, with no pool-side clamping).
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
/// [`aura_buff_modifiers`]'s semantics); support contributions are currently taken in
/// full per group (to be tightened along with `support_modifiers`'s semantics once the
/// T3.6 compatible-list merge lands).
pub(crate) fn spirit_reservation_modifiers(
    build: &Build,
    data: &BuildData,
    db: &pobr_core::ModDb,
) -> Vec<Modifier> {
    // GemlingQuality (the Gemling ascendancy's "Gem Quality grants Socketed Skills an
    // additional effect"): when active, a gem's altQualityStats quality stats apply
    // (CalcTools.lua:147-152), and reservation efficiency gets some through this (e.g.
    // Mirage Archer's alt `base_reservation_efficiency_+%` ×2, Eternal Rage's alt
    // `base_spirit_reservation_efficiency_+%` ×0.75).
    let use_alt_quality = super::resolve::gemling_quality_flag(build, data);
    pobr_core::skill_env::spirit_reservation_modifiers(build, data, db, use_alt_quality)
}

/// (Pre-existing #9) Builds every **enabled warcry active skill** into a
/// [`pobr_core::calc::WarcrySpec`], injected via `session.add_warcry_skill` and
/// consumed by pobr-core's `calc::warcry` (before perform's hand pass), scaled by
/// uptime (matching vendor CalcOffence.lua:3203-3256 + CalcPerform.lua:2116-2142; see
/// warcry.rs's module doc for the mechanic breakdown and oracle-pinned values).
///
/// Spec assembly (all through existing data channels, zero per-skill hardcoding):
/// - **skill-local mods** = the skill's own statSet stats (including the quality
///   segment) mapped via statmap (`mapped_stat_modifiers` — e.g. Infernal Cry's per-set
///   `infernal_cry_exerted_attack_all_damage_%_to_gain_as_fire_%` →
///   `InfernalExtraFireDamageMultiplier`, and the constant stat
///   `warcry_empowers_per_X_...` → `WarcryPowerPer/Cap`) + the group's **compatible
///   support** payload (`support_modifiers`, e.g. Cooldown Recovery II →
///   `CooldownRecovery INC 30`) + `WarcryCastTime BASE` (the effect's `cast_time`,
///   matching vendor skillModList's "Base" entry, the summation source at
///   CalcOffence.lua:351).
/// - **Fetch level** = gem level + any applicable `+N to Level of ...`
///   (`additional_gem_levels`, matching vendor's applyGemMods — confirmed with smith:
///   Infernal Cry 21+1=22 level → gain 51 + quality trunc(0.5×23)=11 → 62, matching
///   oracle exactly).
/// - cooldown / storedUses = the granted_effect_levels row (`resolve_skill_level`).
///
/// The same effect appearing in multiple groups is deduplicated by id (matching
/// vendor's `not globalOutput.<X>CryCalculated` responsibility).
pub(crate) fn warcry_skill_specs(
    context: &mut CalculationContext,
    build: &Build,
    data: &BuildData,
) -> Vec<pobr_core::calc::WarcrySpec> {
    let bonuses = super::resolve::gem_property_bonuses(build, data);
    let mut ctx = context.stat_map_ctx();
    pobr_core::skill_env::warcry_skill_specs(&mut ctx, build, data, &bonuses)
}
