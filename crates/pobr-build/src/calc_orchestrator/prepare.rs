//! Resolve build inputs before constructing the calculation session.

use pobr_core::CalcConfig;
use pobr_core::calc::MinimalInput;
use pobr_data::item::EquipmentSlot;
use pobr_data::modifier::{ModFlags, ModType};
use pobr_data::monster::EnemyTier;
use pobr_data::skill::SkillTypes;

use super::DataOrchestratorOptions;
use super::conditions::{apply_condition_implications, build_has_companion_skill};
use super::conditions::{
    combat_conditions, damage_keywords, main_hand_offhand_is_shield, skill_type_bits,
    skill_type_flags, weapon_cfg_flags, weapon_type_conditions,
};
use crate::build::{Build, SocketGroup};
use crate::build_data::{BuildData, ResolvedSkillLevel};
use crate::support::judge_group_supports;
use pobr_data::catalog::GrantedEffectDef;

use super::item::weapon::{
    WeaponContribution, dual_wield_off_hand_contribution, weapon_contribution,
};
use super::skill::resolve::resolve_main_skill;
use super::skill::triggers::recognize_trigger_config;

pub(super) struct ResolvedMainSkill<'a> {
    pub main_skill: Option<(ResolvedSkillLevel, &'a SocketGroup, &'a str)>,
    pub main_effect: Option<&'a GrantedEffectDef>,
    pub main_skill_types: Vec<String>,
    pub skill_flags: ModFlags,
    pub skill_type_bits: SkillTypes,
}

pub(super) struct ResolvedWeapons {
    pub base_input: MinimalInput,
    pub dmg_mult: f64,
    pub hand_weapon: Option<pobr_core::calc::WeaponBase>,
    pub off_hand_weapon: Option<pobr_core::calc::WeaponBase>,
    pub bypasses_cooldown: bool,
}

/// Stage 1: main skill resolution — per-level parameters, the final skillTypes fixed
/// point (matching vendor CalcActiveSkill.lua:179-214), damage flags / classification
/// bits. Must run first: the action rate needs to go into base_input
/// (stage_weapon_bases), and the type flags / combat conditions need to go into cfg
/// (stage_build_cfg) — both consume this stage's output.
pub(super) fn stage_resolve_main_skill<'a>(
    build: &'a Build,
    data: &'a BuildData,
) -> ResolvedMainSkill<'a> {
    // The main skill's per-level parameters (cast/attack time → action rate; cost /
    // cooldown injected via BASE mods). Resolved before building the session, so the
    // action rate can be written into base_input + the cfg damage flags can be set based on its type.
    let main_skill = resolve_main_skill(build, data);

    // Main skill type → cfg damage flags (Attack/Spell/Projectile/Area/Melee), making
    // `increased <Projectile|Area|Spell|Melee> Damage` apply to this skill (damage
    // aggregation picks these up by flag name). Main skill's effect definition: uses the
    // **real main skill id** resolved by resolve_main_skill (meta/trigger shells already
    // skipped), not the first gem's active_skill_id in the group (which is a meta shell
    // in a multi-active-skill group, causing flag/damage-type mismatches).
    let main_effect = main_skill
        .as_ref()
        .and_then(|(_, _, skill_id)| data.granted_effects.get(*skill_id));
    // The main skill's **final** type set = its own skill_types + the addSkillTypes
    // fixed point over compatible supports (matching vendor CalcActiveSkill.lua:179-214,
    // which merges addSkillTypes into activeSkill.skillTypes, with every downstream
    // flag/condition derivation using the final set — e.g. Cast on Critical adds
    // `Triggered` to the triggered spell, making the "Triggered Spells deal …" mod
    // family hit + the combat-condition trigger exemption apply per vendor :248).
    // Sorted for determinism.
    let main_skill_types = main_skill
        .as_ref()
        .map(|(_, group, skill_id)| {
            let mut types: Vec<String> =
                judge_group_supports(group, data, skill_id, group.from_gem())
                    .final_skill_types
                    .into_iter()
                    .collect();
            // A meta trigger shell's `Triggered`: vendor injects this from the gem's
            // **support half** (e.g. Cast on Critical → SupportMetaCastOnCritPlayer's
            // addSkillTypes=[Triggered]); PoBR's cataloged data doesn't model a gem's
            // second granted-effect half (skill_gems only has the primary
            // grantedEffect half), so this backfills equivalently using the existing
            // trigger recognition (trigger_configs's four-level key, the same
            // determination as trigger_modifiers).
            if !types.iter().any(|t| t == "Triggered")
                && recognize_trigger_config(data, group, skill_id).is_some()
            {
                types.push("Triggered".to_string());
            }
            types.sort();
            types
        })
        .unwrap_or_default();
    let skill_flags = main_effect
        .map(|_| skill_type_flags(&main_skill_types))
        .unwrap_or(ModFlags::NONE);
    // Main skill type → `cfg.skill_types` classification bits: `is_attack()` drives the
    // hit-chance check (only attacks do an accuracy/evasion check, vendor
    // CalcOffence.lua:2611); see skill_type_bits's doc.
    let skill_type_bits = main_effect
        .map(|_| skill_type_bits(&main_skill_types))
        .unwrap_or(SkillTypes::NONE);
    ResolvedMainSkill {
        main_skill,
        main_effect,
        main_skill_types,
        skill_flags,
        skill_type_bits,
    }
}

/// Stage 2: closes out config consumption (the primary-path switch) — goes through
/// `config_interpreter::interpret` when a ConfigCatalog is available (raw_inputs →
/// conditions/multipliers/scalar wrapping/Config-attributed modifiers); falls back to
/// the legacy parse_config output when the catalog is missing (tolerant of a missing
/// table). Produces the base cfg (including backfilling the config multiplier bridge
/// for the Effective gate); stage_build_cfg layers skill-derived pieces on top of it.
pub(super) fn stage_resolve_config(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
) -> (crate::config_resolve::ResolvedConfig, CalcConfig) {
    let resolved_config =
        crate::config_resolve::resolve_config(build, data.config_catalog.as_deref());
    let mut base_cfg = resolved_config.config.to_calc_config();
    // The config multiplier bridge for the Effective gate: the interpreter's bare-effect
    // Condition bridge only accepts "tagless" entries, so a count-type placeholder for
    // `Multiplier:<X>` carrying a `Condition:Effective` tag (e.g. vendor
    // ConfigOptions.lua:1642's `multiplierDifferentGrenadeFired`'s
    // defaultPlaceholderState=1) doesn't land in cfg.multipliers. Vendor's semantics =
    // `GetMultiplier` queries modDB directly (the tag is evaluated against cfg; under
    // EFFECTIVE mode, Effective is always true, CalcSetup.lua:583-588); PoBR's
    // multiplier goes through a cfg snapshot → backfilled here after evaluating against
    // mode_effective (only for the single-tag Effective shape; other tag shapes stay on
    // the mod channel).
    if options.mode_effective {
        for m in &resolved_config.player_mods {
            // Only accepts the shape "has tags and they're all Effective" — **an empty-tag
            // entry must be excluded**: a bare `Multiplier:` effect is already backfilled
            // into cfg.multipliers by the interpreter's bare-effect path
            // (config_interpreter.rs:362-377); adding it again here would double-count
            // (confirmed: sigilOfPowerStages's placeholder 1 got boosted to 2 under
            // effective semantics, making Sigil of Power's per-stage MORE falsely go
            // from 17→34). `Combat` and `Effective` share the same gate (vendor's main
            // output env has both always true, CalcSetup.lua:583-588 + mode_combat;
            // e.g. `multiplierNearbyAlly`'s `Multiplier:NearbyAlly BASE +
            // Condition{Combat}` — the denominator for the NearbyAlly≥1 threshold row,
            // ConfigOptions.lua:1018).
            if m.mod_type == ModType::Base
                && let Some(var) = m.name.as_str().strip_prefix("Multiplier:")
                && let pobr_core::ModValue::Number(n) = m.value
                && !m.tags.is_empty()
                && m.tags.iter().all(|t| {
                    matches!(t, pobr_core::ModTag::Condition { var, negated: false, actor: None } if var == "Effective" || var == "Combat")
                })
            {
                *base_cfg.multipliers.entry(var.to_string()).or_insert(0.0) += n;
            }
        }
    }
    (resolved_config, base_cfg)
}

/// Stage 3: cfg assembly — layers the main skill's damage flags / classification bits /
/// display name / keywords / mode toggles onto the base cfg (matching vendor
/// CalcSetup.lua:583-597's buffMode "EFFECTIVE" semantics), then adds combat
/// conditions, enemy tier conditions, PoB2's condition implication chain, and
/// build-state equipment/weapon conditions. Depends on stage 1/2's output
/// (skill_flags / base cfg), must run before session creation (with_config replaces cfg wholesale).
pub(super) fn stage_build_cfg(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    main: &ResolvedMainSkill<'_>,
    resolved_config: &crate::config_resolve::ResolvedConfig,
    base_cfg: CalcConfig,
) -> (CalcConfig, EnemyTier) {
    let base_flags = base_cfg.flags;
    let mut cfg = base_cfg
        .with_flags(base_flags | main.skill_flags)
        .with_skill_types(main.skill_type_bits)
        // Main skill's display name (matching vendor's `skillCfg.skillName`): the
        // matching semantics for the special channel's `SkillName` tag. Same source as
        // gem_level_category_matches (skill_name_from_id, lowercase).
        .with_skill_name(
            main.main_skill
                .as_ref()
                .map(|(_, _, skill_id)| super::skill::resolve::skill_name_from_id(skill_id)),
        )
        .with_damage_keywords(damage_keywords(
            build,
            data,
            main.main_effect
                .map(|_| main.main_skill_types.as_slice())
                .unwrap_or(&[]),
        ))
        .with_mode_effective(options.mode_effective)
        // Vendor's buffMode is always "EFFECTIVE" outside CALCS mode
        // (CalcSetup.lua:583-597 → env.mode_buffs = true), so mode_buffs is always set
        // here — enabling buff_pass (the aura multiplier zone / curse priority+limit).
        // mode_effective still follows the caller's option.
        .with_mode_buffs(true)
        // Same as above (CalcSetup.lua:583-597's buffMode "EFFECTIVE" →
        // env.mode_combat = true). Activation surface: automatic combat condition
        // setting (combat_conditions below) + env_finalize stage 3's flask/charm merge +
        // stage 6's buff_expander.
        .with_mode_combat(true);
    // DistanceRamp's skillDist (matching vendor CalcActiveSkill.lua:671+684, 0.22.0):
    // `effectiveRange = env.configInput.enemyDistance or env.configPlaceholder.enemyDistance`,
    // `skillDist = env.mode_effective and effectiveRange`. From 0.22.0 on, **a
    // placeholder feeds skillDist as a fallback** (old vendor only read the explicit
    // `<Input>` — back then, the demo suite was all placeholders → None → the Close
    // Combat distance MORE was skipped entirely). The fallback chain matches vendor
    // ConfigTab: explicit `<Input>` → XML `<Placeholder>` → the catalog's
    // `defaultPlaceholderState` (ConfigTab.lua:559 pre-fills a placeholder default for
    // an entry with no value, enemyDistance = 20).
    let skill_distance = options
        .mode_effective
        .then(|| {
            let raw = &build.config.raw_inputs;
            raw.values
                .get("enemyDistance")
                .or_else(|| raw.placeholders.get("enemyDistance"))
                .and_then(|v| v.as_number())
                .or_else(|| {
                    data.config_catalog
                        .as_deref()
                        .and_then(|c| c.get("enemyDistance"))
                        .and_then(|def| def.default.as_ref())
                        .and_then(|d| d.placeholder_number)
                })
        })
        .flatten();
    cfg = cfg.with_skill_distance(skill_distance);
    // SkillStatMap's skill_can_fire_arrows -> skillFlags.arrow ->
    // CalcActiveSkill's KeywordFlag.Arrow. Use the selected stat set, since
    // secondary projectiles need not be arrows even when fired from a bow.
    if main.main_skill.as_ref().is_some_and(|(skill, _, _)| {
        skill
            .base_damage
            .iter()
            .any(|stat| stat.stat == "skill_can_fire_arrows" && stat.value != 0.0)
    }) {
        cfg.keyword_flags = cfg.keyword_flags | pobr_data::modifier::KeywordFlags::ARROW;
    }
    // Main skill-derived combat conditions (read directly from vendor
    // CalcPerform.lua:242-266's `if env.mode_combat` section): attack/spell/Movement/
    // Minion/Vaal/Channel → "...Recently"/Channelling conditions;
    // triggered/trap/mine/totem exempted (using the **final** type set — a meta
    // support's addSkillTypes `Triggered` makes the exemption apply, matching vendor :248).
    if main.main_effect.is_some() {
        for cond in combat_conditions(&main.main_skill_types, main.skill_flags) {
            cfg = cfg.with_condition(cond, true);
        }
    }
    // Enemy tier (19-G3 wiring): the build XML Config's explicitly saved `enemyIsBoss`
    // takes priority; falls back to the caller's orchestrator option when omitted
    // (PoB2's defaultIndex=3 = Pinnacle, matching existing callers).
    let enemy_tier = resolved_config
        .config
        .enemy_tier
        .unwrap_or(options.enemy_tier);
    // Enemy rarity condition: the default DPS view vs. Boss/Pinnacle/Uber (= Unique) →
    // set true, making condition-type damage boosts like "... against Rare or Unique
    // Enemies" apply (PoB's boss-DPS semantics).
    if matches!(
        enemy_tier,
        EnemyTier::Boss | EnemyTier::Pinnacle | EnemyTier::Uber
    ) {
        cfg = cfg
            .with_condition("Unique", true)
            .with_condition("RareOrUnique", true);
    }

    // PoB2's condition implication chain (ConfigOptions.lua's `implyCond`/
    // `implyCondList`): a parent condition checked in build config automatically sets
    // several child conditions true. PoBR only reads build config's parent condition
    // names, so implications must be filled in here, or child-condition-type mods
    // (already parsed by PoBR as condition tags) wouldn't apply. Generic, independent of build/skill.
    cfg = apply_condition_implications(cfg);

    // PoB2's `Condition:UsingShield` (CalcSetup: set true when the off-hand is a
    // shield). Determined from whether the current active equipment group's off-hand
    // slot has a shield-category base — a build-state default, consistent across the
    // whole build, not specialized.
    if main_hand_offhand_is_shield(build, data) {
        cfg = cfg.with_condition("UsingShield", true);
    }
    // Enemy within Presence (matching vendor CalcPerform.lua:524's
    // `condList["EnemyInPresence"] = PresenceRadius >= enemyDistance`): the default
    // Presence radius (a few meters) is always greater than the default enemy distance
    // → true by default, making the "Enemies in your Presence ..." enemy-side mod
    // family apply.
    // ponytail: pobr doesn't model a numeric PresenceRadius/enemyDistance comparison,
    // always sets it true; if a user pulls enemyDistance out far, the semantics gap is
    // left for the parity gate to flag before being wired up.
    if !cfg.conditions.contains_key("EnemyInPresence") {
        cfg = cfg.with_condition("EnemyInPresence", true);
    }
    // Companion-in-presence condition (matching vendor ConfigOptions.lua:1012-1014's
    // `companionInPresence`, defaultState=true, gated by ifSkillType=CreatesCompanion):
    // set true by default when an enabled skill includes `CreatesCompanion`, making the
    // "while your Companion is in your Presence" mod family apply (twister's tree node
    // Tree:37769's +10 INC). An explicit config input (the XML's `companionInPresence`)
    // takes priority; falls back to the default only when absent.
    if !cfg.conditions.contains_key("CompanionInPresence") && build_has_companion_skill(build, data)
    {
        cfg = cfg.with_condition("CompanionInPresence", true);
    }
    // The equipment condition for the "Body Armour grants <mod>" prefix family
    // (matching PoB2 ModParser.lua:1418 / :3255-3268's
    // `ItemCondition{itemSlot="Body Armour", rarityCond="NORMAL"}`): set true when the
    // body armour slot has an item equipped with Normal rarity. A build-state default,
    // consistent across the whole build, not specialized.
    if build
        .items
        .get(&EquipmentSlot::BodyArmour)
        .is_some_and(|item| item.rarity == pobr_data::item::ItemRarity::Normal)
    {
        cfg = cfg.with_condition("NormalBodyArmourEquipped", true);
    }
    // Main-hand weapon category → grip conditions (makes tree/mods like "... with
    // Quarterstaves" or "while Dual Wielding" apply). Cooldown-limited main skills
    // (grenades) are no longer a special case — the old "attack-speed compensates
    // throughput" approximation has been removed; the end of the speed chain uniformly
    // uses `min(rate, repeats/effective_cooldown)` (matching vendor's ordering), so
    // weapon-category attack-speed mods no longer incorrectly amplify grenade rate;
    // weapon-category conditions / weapon bit flags are enabled fully, matching vendor.
    for var in weapon_type_conditions(build, data) {
        cfg = cfg.with_condition(var, true);
    }
    // Main-hand weapon bits → cfg.flags: derived from the **same source**
    // (weapon_type_info table) with the **same gating** as the Using* conditions above
    // — the mod-side dual-written weapon-bit channel doesn't get a separate activation
    // path outside the condition channel.
    let weapon_bits = weapon_cfg_flags(build, data);
    if !weapon_bits.is_empty() {
        cfg.flags |= weapon_bits;
    }
    (cfg, enemy_tier)
}

/// Stage 4: weapon base assembly — main skill's use_time → action rate, skill damage
/// multiplier, main-/off-hand weapon base contribution converted into
/// [`pobr_core::calc::WeaponBase`] for HandSource, cooldown-bypass determination.
/// Depends on stage 1's main skill output; must run before session creation (base_input goes into `CalculationSession::new`).
pub(super) fn stage_weapon_bases(
    build: &Build,
    data: &BuildData,
    main: &ResolvedMainSkill<'_>,
    mut base_input: MinimalInput,
) -> ResolvedWeapons {
    if let Some((skill, _, skill_id)) = &main.main_skill
        && let Some(use_time) = skill.use_time_s
        && use_time > 0.0
    {
        if pobr_core::dbg_env!("POBR_DBG_SPEED").is_some() {
            eprintln!("[POBR_DBG_SPEED] main skill_id={skill_id} use_time={use_time}");
        }
        base_input.base_action_rate = 1.0 / use_time;
    }

    // Skill damage multiplier (PoB's baseMultiplier, e.g. a grenade's 7.57): scales weapon hit + added damage.
    let dmg_mult = main
        .main_skill
        .as_ref()
        .map(|(s, _, _)| s.damage_multiplier)
        .filter(|m| *m > 0.0)
        .unwrap_or(1.0);

    // Weapon base contribution (attack skills only): hit physical damage (× skill
    // multiplier) + attack rate override. Uses the resolved real main skill id (meta
    // shells skipped), ensuring correct attack/spell determination and weighting.
    //
    // Weapon base no longer folds directly into `base_input`; it's now assembled into a
    // `HandSource` and injected via `set_hand_sources`, and `perform`'s internal
    // `run_hand_passes` injects the same set of values into a per-hand `MinimalInput`
    // copy — a single HandSource is value-for-value equal to the old conversion (a
    // direct pass-through, pinned by an equivalence test). The conversion semantics are
    // unchanged: phys × dmg_mult, attack_rate × attackSpeedMultiplier (matching
    // CalcOffence L2721-2723).
    let weapon = main
        .main_skill
        .as_ref()
        .and_then(|(skill, _, skill_id)| weapon_contribution(build, data, skill_id, skill));
    // Dual-wielding off-hand: when the main hand is a real one-handed weapon and
    // Weapon2 is also a weapon base, assembles a second off-hand weapon source
    // (matching vendor's weapon2Attack pass, CalcOffence.lua:2369-2449).
    let off_weapon = weapon
        .as_ref()
        .and_then(|_| dual_wield_off_hand_contribution(build, data, main.main_effect));
    let asm = main
        .main_skill
        .as_ref()
        .and_then(|(s, _, _)| s.attack_speed_multiplier)
        .map_or(1.0, |m| 1.0 + m / 100.0);
    let to_hand_base = |w: &WeaponContribution| pobr_core::calc::WeaponBase {
        hit_min: w.phys_min * dmg_mult,
        hit_max: w.phys_max * dmg_mult,
        attack_rate: (w.attack_rate > 0.0).then_some(w.attack_rate * asm),
        crit_chance: w.crit_chance,
        flags: w.flags,
    };
    let hand_weapon = weapon.as_ref().map(to_hand_base);
    let off_hand_weapon = off_weapon.as_ref().map(to_hand_base);

    // Cooldown-limited rate: PoB's order — first fully compute every speed inc/more,
    // then apply `min(rate, 1/effective_cooldown)` (effective_cooldown shortened via
    // `CooldownRecovery`). This min is pushed down into offence.rs's
    // `apply_cooldown_cap`, which reads `SkillCooldownBase` BASE (injected by
    // `skill_base_modifiers`) + `CooldownRecovery` (aggregated across the whole
    // statmap/quality/tree/quest chain) + `SkillStoredUsesBase` (no rounding to a
    // frame when stored uses >1). Spells and cooldown-limited attacks (grenades)
    // uniformly go through this semantics (the old "attack speed compensates
    // throughput" pre-truncation approximation has been removed — the throughput
    // multiplier is now handled by GrenadeActivateTwice → dps_end_factors, matching
    // vendor CalcOffence.lua:2852-2856's ordering).
    //
    // Exception (bypasses cooldown): a skill whose cooldown resets by consuming charges
    // (e.g. Flicker Strike's `SkillConsumesPowerChargesOnUse`) → PoB2's Cooldown=nil,
    // fires at attack speed unrestricted → `CooldownBypass`.
    //
    // Whether the main skill bypasses cooldown (used the instant a charge is consumed,
    // e.g. Flicker) → injects `CooldownBypass` (single source).
    let bypasses_cooldown = main
        .main_effect
        .map(|e| {
            e.skill_types
                .iter()
                .any(|t| t == "SkillConsumesPowerChargesOnUse")
        })
        .unwrap_or(false);
    ResolvedWeapons {
        base_input,
        dmg_mult,
        hand_weapon,
        off_hand_weapon,
        bypasses_cooldown,
    }
}
