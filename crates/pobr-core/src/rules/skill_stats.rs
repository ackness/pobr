//! What a skill's stats grant, written from the game's own data.
//!
//! Each rule is keyed by the game's stat id and states, in the engine's own
//! terms, what that stat does. Its provenance is the stat's description text
//! in the game and the skill's data under `data/<version>/base/`. A stat with
//! a rule here is not looked up in the stat map extracted from PoB2: this is
//! where that map is replaced, one stat at a time.

use pobr_data::catalog::SkillDamageStat;
use pobr_data::prelude::*;

use crate::{ModTag, ModValue, Modifier};

/// The ailment each element builds up on its own.
const ELEMENTAL_AILMENTS: [(&str, &str); 3] = [
    ("Fire", "Ignite"),
    ("Cold", "Freeze"),
    ("Lightning", "Shock"),
];

/// The stat behind "Benefits from consuming Charges for Supported Skill have
/// {0}% chance to be doubled", on a skill or on a support that applies to it.
pub const CONSUMED_CHARGE_BENEFIT_DOUBLE_CHANCE: &str =
    "chance_%_to_double_effect_of_removing_charges";

/// What the rules for a buff skill's stats need to know about that skill.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BuffSkill {
    /// The chance (%) that the benefits of the charges the skill consumes are
    /// doubled: [`CONSUMED_CHARGE_BENEFIT_DOUBLE_CHANCE`] summed over the
    /// skill and its supports.
    pub consumed_charge_benefit_double_chance: f64,
}

impl BuffSkill {
    /// The average benefit of a consumed charge, relative to a charge whose
    /// benefit is never doubled.
    fn consumed_charge_benefit(self) -> f64 {
        1.0 + self.consumed_charge_benefit_double_chance.clamp(0.0, 100.0) / 100.0
    }
}

/// The stat behind "Each Cursed Ground area from Supported Skills spawns a
/// Witchtoad every {0} seconds" (Doedre's Undoing).
const WITCHTOAD_SPAWN_INTERVAL_MS: &str = "doedre_cursed_ground_toad_spawn_interval_ms";

/// The stats that say a skill inflicts Withered: on the skill itself, or on a
/// support for the skills it supports. "Minions have {0}% chance to inflict
/// Withered on Hit" is not among them: it is about the minions.
const INFLICTS_WITHERED: [&str; 9] = [
    // "{0}% chance to inflict Withered on Hit" and its fixed-duration forms.
    "withered_on_hit_chance_%",
    "withered_on_hit_for_2_seconds_%_chance",
    "withered_on_chaos_damage_hit_chance_%",
    "wither_on_hit_chance_rollovercapped",
    // Withering Touch: a chance per 100% of the target's Ailment Threshold
    // dealt as Chaos damage.
    "withered_on_hit_chance_%_for_every_100%_target_ailment_threshold_dealt_as_chaos_damage",
    // "Inflicts {0} Withered on Hit" (Chaos Bolt granted by a weapon).
    "apply_x_wither_on_hit",
    "archon_of_chayula_withered_on_hit_for_4_seconds_%_chance",
    // Withering Presence: "Every {0} seconds, inflicts Withered on enemies
    // in your Presence".
    "skill_withering_presence_frequency_ms",
    // Wither, His Foul Emergence: the duration of the Withered they inflict.
    "active_skill_withered_base_duration_ms",
];

/// Whether the stat `stat`, on a skill or on a support that applies to it,
/// says the skill inflicts Withered. A character that uses such a skill can
/// have Withered on the enemy, whichever skill is being calculated: the
/// build is searched for one (`skill_env::skill_inflicting_withered`), and
/// the stat grants nothing to the skill itself.
pub fn inflicts_withered(stat: &str) -> bool {
    INFLICTS_WITHERED.contains(&stat)
}

/// Whether the corpse's Life the stat names is damage the skill only poisons
/// with, without hitting: Decompose's cloud "Poisons enemies as though dealing
/// Chaos damage equal to {0}% of Corpse's maximum Life". Without a hit there
/// is no critical hit.
pub fn corpse_damage_without_hit(stat: &str) -> bool {
    stat == "corpse_explosion_monster_life_permillage_chaos"
}

/// What a corpse explosion deals, by the stat of the skill that says so: the
/// type of its damage and the share of the corpse's maximum Life it is.
///
/// - `corpse_explosion_monster_life_permillage_<type>`, in thousandths:
///   Detonate Dead's "Deals Physical damage equal to {0}% of Corpse's maximum
///   Life", Decompose's "Poisons enemies as though dealing Chaos damage equal
///   to {0}% of Corpse's maximum Life";
/// - `corpse_explosion_monster_life_%`, in hundredths: Fiery Death's
///   "Explosion deals base Fire Damage equal to {0}% of the Corpse's Maximum
///   Life";
/// - `corpse_explosion_monster_life_%_<type>`, in hundredths: "Explosion
///   deals base Lightning Damage equal to {0}% of corpse's Maximum Life".
pub fn corpse_explosion_damage(stat: &str, value: f64) -> Option<(DamageType, f64)> {
    let damage_type = |name: &str| match name {
        "physical" => Some(DamageType::Physical),
        "fire" => Some(DamageType::Fire),
        "cold" => Some(DamageType::Cold),
        "lightning" => Some(DamageType::Lightning),
        "chaos" => Some(DamageType::Chaos),
        _ => None,
    };
    let rest = stat.strip_prefix("corpse_explosion_monster_life_")?;
    if let Some(name) = rest.strip_prefix("permillage_") {
        return Some((damage_type(name)?, value / 1000.0));
    }
    match rest.strip_prefix('%')? {
        "" => Some((DamageType::Fire, value / 100.0)),
        typed => Some((damage_type(typed.strip_prefix('_')?)?, value / 100.0)),
    }
}

/// The stats that say how many minions a summoning skill creates, of each
/// kind of minion it summons.
const SUMMONED_MINION_COUNTS: [&str; 1] = [
    // Cackling Companions: "Summons {0} Hyenas".
    "skill_hyena_cackle_size",
];

/// Whether the stat `stat` of a summoning skill is the number of minions it
/// creates. They are counted where the skill creates them
/// (`skill_env::spawn_minions`); the stat grants the skill nothing.
pub fn is_summoned_minion_count(stat: &str) -> bool {
    SUMMONED_MINION_COUNTS.contains(&stat)
}

/// What a summoning skill's minions are when its minion stats are judged:
/// the facts about the character's minions those stats depend on.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SummonedMinions {
    /// How many different kinds of Reviving Minion the character summons:
    /// the distinct skills of the `Minion` and `Persistent` types it uses.
    pub reviving_kinds: u32,
    /// The summoner's final attributes, before any sharing with minions.
    pub player_strength: f64,
    pub player_dexterity: f64,
}

/// The modifiers the stat `stat` of a summoning skill, or of a support that
/// applies to it, grants that skill's minions at `value`, when this table
/// defines the stat. The skill itself gets nothing from it.
pub fn minion_modifiers(stat: &str, value: f64, minions: SummonedMinions) -> Option<Vec<Modifier>> {
    let modifier = match stat {
        // "Minions deal {0}% more damage", of the summoning skill itself.
        "active_skill_minion_damage_+%_final"
        // Feeding Frenzy: "Minions from Supported Skills deal {0}% more
        // Damage".
        | "feeding_frenzy_minion_damage_+%_final" => {
            Modifier::number("Damage", ModType::More, value)
        }
        // Feeding Frenzy: "Minions from Supported Skills take {0}% more
        // Damage".
        "feeding_frenzy_minion_damage_taken_+%_final" => {
            Modifier::number("DamageTaken", ModType::More, value)
        }
        // "Supported Skills have {0}% increased Minion Attack Speed".
        "minion_attack_speed_+%" => Modifier::number("AttackSpeed", ModType::Inc, value),
        // "Minions from this skill have 1% increased Damage per {0} of your Strength".
        "minion_1%_damage_+%_per_X_player_strength" => Modifier::number(
            "Damage", ModType::Inc,
            if value > 0.0 { (minions.player_strength / value).floor() } else { 0.0 },
        ),
        // "Minions from this skill have 1% increased Attack Speed per {0} of your Dexterity".
        "active_skill_minion_1%_attack_speed_+%_per_X_player_dexterity" => Modifier::number(
            "AttackSpeed", ModType::Inc,
            if value > 0.0 { (minions.player_dexterity / value).floor() } else { 0.0 },
        ),
        "minion_1%_area_of_effect_+%_per_X_player_dexterity" => Modifier::number(
            "AreaOfEffect", ModType::Inc,
            if value > 0.0 { (minions.player_dexterity / value).floor() } else { 0.0 },
        ),
        // Raging Spirits: "Minions convert {0}% of Physical Damage to Fire Damage".
        "minion_skill_physical_damage_%_to_convert_to_fire" => {
            Modifier::number("SkillPhysicalDamageConvertToFire", ModType::Base, value)
        }
        // Tangmazu's Thurible: "Companions from Supported Skills are
        // Gigantic"; a summoning skill's "Minions are Gigantic".
        "companions_are_gigantic" | "minions_are_gigantic" => {
            Modifier::flag(crate::calc::minion::GIGANTIC)
        }
        // Bidding: "Supported Minions deal {0}% more Damage with Command
        // Skills".
        "support_command_skill_damage_+%_final" => Modifier::number("Damage", ModType::More, value)
            .with_tag(ModTag::condition(COMMAND_SKILL, false)),
        // "Minions have {0}% increased Cooldown Recovery Rate for Command
        // Skills".
        "minion_command_skill_cooldown_speed_+%" => {
            Modifier::number("CooldownRecovery", ModType::Inc, value)
                .with_tag(ModTag::condition(COMMAND_SKILL, false))
        }
        // Muster: "Supported Minions deal {0}% more damage for each different
        // type of Reviving Minion you have summoned".
        "support_varied_troops_damage_+%_final_per_different_persistent_ominion" => {
            Modifier::number(
                "Damage",
                ModType::More,
                value * f64::from(minions.reviving_kinds),
            )
        }
        _ => return None,
    };
    Some(vec![modifier])
}

/// Sacrificial Offering: "Supported Offering Skills have {0}% increased Buff
/// effect", of the buff the Offering grants.
pub const OFFERING_BUFF_EFFECT: &str = "offering_spells_effect_+%";

/// "Flames of Chayula have {0}% increased effect" on Into the Breach,
/// "Remnants from Supported Skills have {0}% increased effect" on a support:
/// the effect of the Remnants the skill creates, and of the buff they grant.
pub const REMNANT_EFFECT: &str = "remnant_effect_+%";

/// The Purple Flames of Chayula the character has collected.
pub const PURPLE_FLAMES: &str = "PurpleFlamesCount";

/// The Purple Flames' buff "stacks up to 10 times".
pub const MAXIMUM_PURPLE_FLAMES: f64 = 10.0;

/// An aura of the character's that only its allies have, the minions among
/// them (`calc::perform`'s `ExtraAura`): `modifiers` are what each has.
fn allies_aura(modifiers: Vec<Modifier>) -> Modifier {
    Modifier::new("ExtraAura", ModType::List, ModValue::NestedMods(modifiers))
}

/// The valueless stat of a Command skill, a skill a minion uses when the
/// character commands it (Navira's Fracturing, the Water Djinn's). What is
/// said of Command skills ("Minions deal 20% increased Damage with Command
/// Skills", Bidding's "Supported Minions deal 30% more Damage with Command
/// Skills") holds while a minion uses one: the condition [`COMMAND_SKILL`].
/// Embitter (`all_damage_gained_as_cold_instead`): "all sources of Damage
/// Gained [are] instead … treated as Damage Gained as extra Cold for
/// Supported Skills" (`calc::damage`'s gain table).
pub const GAINS_ONLY_COLD: &str = "DamageGainIsOnlyCold";

pub const COMMAND_SKILL_STAT: &str = "is_commandable_skill";

/// The condition a minion's Command skill sets (`CommandableSkill`).
pub const COMMAND_SKILL: &str = "CommandableSkill";

/// Whether the stat `stat` is about the minions of the skill it is on (see
/// [`minion_modifiers`]).
pub fn is_minion_stat(stat: &str) -> bool {
    minion_modifiers(stat, 0.0, SummonedMinions::default()).is_some()
}

/// Whether a skill with the stat `stat` Crushes the targets it hits: Styrn's
/// Ferocity, "Supported Skills Crush targets they Hit for 8 seconds". Crushed
/// lowers the target's Physical Damage Reduction (`calc::setup_env`).
pub fn crushes_on_hit(stat: &str) -> bool {
    stat == "support_knights_ferocity_crush_on_hit_ms"
}

/// The modifiers the stat `stat` of a skill, or of a support that applies to
/// it, grants that skill at `value`, when this table defines the stat. `None`
/// leaves the stat to the extracted stat map.
pub fn skill_modifiers(stat: &str, value: f64) -> Option<Vec<Modifier>> {
    match stat {
        // The selected effect deals no damage; secondary explosions have
        // their own stat sets without this stat.
        "base_deal_no_damage" => Some(vec![Modifier::flag("DealNoDamage")]),
        // Explosive Demise detonates an allied minion below this life
        // threshold. With no minion present there is nothing to detonate.
        "command_minion_target_ally_below_x_life" => {
            Some(vec![Modifier::flag("DealNoDamage").with_tag(
                ModTag::MultiplierThreshold {
                    var: "MinionPresenceCount".into(),
                    threshold: 0.0,
                    upper: true,
                },
            )])
        }
        "base_minimum_channel_time_ms" => Some(vec![Modifier::number(
            "ChannelMinimumTime",
            ModType::Base,
            value / 1000.0,
        )]),
        "channel_start_lock_cancelling_of_attack_time_%" => Some(vec![Modifier::number(
            "ChannelStartLock",
            ModType::Base,
            value,
        )]),
        "channel_end_duration_as_%_of_attack_time" => Some(vec![Modifier::number(
            "ChannelReleaseFraction",
            ModType::Base,
            value,
        )]),
        "channel_skill_end_animation_duration_override_ms" => Some(vec![Modifier::number(
            "ChannelReleaseTime",
            ModType::Base,
            value / 1000.0,
        )]),
        "channelled_slam_max_stages" | "detonating_arrow_max_number_of_stages" => {
            Some(vec![Modifier::number(
                "ChannelMaxStages",
                ModType::Base,
                value,
            )])
        }
        // "40% more Damage with Hits for each stage"; not ailment damage.
        "channelled_slam_damage_+%_final_per_stage" => Some(vec![
            Modifier::number("Damage", ModType::More, value)
                .with_keyword_flags(KeywordFlags::HIT)
                .with_tag(ModTag::multiplier(crate::calc::channel::STAGES, 1.0, None)),
        ]),
        "detonating_arrow_all_damage_%_to_gain_as_fire_per_stage" => Some(vec![
            Modifier::number("DamageGainAsFire", ModType::Base, value)
                .with_tag(ModTag::multiplier(crate::calc::channel::STAGES, 1.0, None)),
        ]),
        // Supporting Fire: "{0} Volleys" per command, each dealing
        // the attack's damage. Volley count changes DPS, not each hit.
        "death_from_above_number_of_volleys" => Some(vec![Modifier::number(
            "DPS",
            ModType::More,
            (value.max(1.0) - 1.0) * 100.0,
        )]),
        // Dark Pact adds a percentage of the target skeleton's maximum Life.
        "skeletal_chains_aoe_%_health_dealt_as_chaos_damage" => Some(vec![Modifier::number(
            "DarkPactLifeAsChaos",
            ModType::Base,
            value,
        )]),
        // The burning stat is a percentage per minute, not per second.
        "herald_of_ash_burning_%_overkill_damage_per_minute" => Some(vec![Modifier::number(
            "HeraldOfAshBurningPerMinute",
            ModType::Base,
            value,
        )]),
        "support_inevitable_criticals_critical_strike_chance_+%_per_second" => {
            Some(vec![Modifier::number(
                "InexorableCritChancePerSecond",
                ModType::Base,
                value,
            )])
        }
        "support_inevitable_criticals_critical_strike_chance_+%_cap" => {
            Some(vec![Modifier::number(
                "InexorableCritChanceCap",
                ModType::Base,
                value,
            )])
        }
        "support_inevitable_criticals_critical_strike_multiplier_+%_final_per_second" => {
            Some(vec![Modifier::number(
                "InexorableCritBonusPerSecond",
                ModType::Base,
                value,
            )])
        }
        "support_inevitable_criticals_critical_strike_multiplier_+%_final_cap" => {
            Some(vec![Modifier::number(
                "InexorableCritBonusCap",
                ModType::Base,
                value,
            )])
        }
        // "{0}% more damage with Hits for blasts originating from a Unique enemy".
        "snap_damage_+%_final_if_created_from_unique" => Some(vec![
            Modifier::number("Damage", ModType::More, value)
                .with_flags(ModFlags::HIT)
                .with_tag(ModTag::Condition {
                    var: "Unique".into(),
                    negated: false,
                    actor: None,
                }),
        ]),
        stat if inflicts_withered(stat) => Some(Vec::new()),
        stat if is_summoned_minion_count(stat) => Some(Vec::new()),
        // Granted to the skill's minions where they are created
        // (`skill_env::spawn_minions`).
        stat if is_minion_stat(stat) => Some(Vec::new()),
        // Read where a minion's skill is chosen (`calc::MinionSkill`).
        COMMAND_SKILL_STAT => Some(Vec::new()),
        // Read by the weapon source resolver: a shield's defence replaces
        // the character's unarmed damage for Raise Shield and Parry.
        "replace_off_hand_unarmed_attack_stats_with_shield_type" => Some(Vec::new()),
        // "Your Hits can't be Evaded" (including the implicit stat on palms).
        "global_always_hit" => Some(vec![Modifier::flag("CannotBeEvaded")]),
        // Mace Strike hits once with each weapon. Keep each hit's damage and
        // critical chance, but count both hits when dual wielding.
        "skill_double_hits_when_dual_wielding" => Some(vec![
            Modifier::number("DPS", ModType::More, 100.0)
                .with_tag(ModTag::condition("DualWielding", false)),
        ]),
        // Reap: "Culling Strike" (`calc::skill_dot::cull_multiplier`).
        "kill_enemy_on_hit_if_under_10%_life" => Some(vec![Modifier::flag("CanCull")]),
        // Detonate Living: "Your Spells have Culling Strike".
        "spells_have_culling_strike" => {
            Some(vec![Modifier::flag("CanCull").with_flags(ModFlags::SPELL)])
        }
        // Put on the enemy whichever skill crushes it
        // (`skill_env::skill_crushing_on_hit`).
        stat if crushes_on_hit(stat) => Some(Vec::new()),
        // Decompose: "Poisons enemies as though dealing Chaos damage equal
        // to {0}% of Corpse's maximum Life": what it deals poisons.
        "corpse_explosion_monster_life_permillage_chaos" => {
            Some(vec![Modifier::number("PoisonChance", ModType::Base, 100.0)])
        }
        // The corpse's Life the skill deals, where the corpse is known
        // (`calc_orchestrator::skill::mods::corpse_explosion_modifiers`).
        stat if corpse_explosion_damage(stat, 0.0).is_some() => Some(Vec::new()),
        // Put on the enemy as the skill's debuff (`debuff_modifiers`).
        stat if debuff_modifiers(stat, 0.0).is_some() => Some(Vec::new()),
        // Raise the buff of the Offering, or the effect of the Remnants, of
        // the skill they are on, where that buff is made
        // (`skill_env::buff_specs`).
        OFFERING_BUFF_EFFECT | REMNANT_EFFECT => Some(Vec::new()),
        "all_damage_gained_as_cold_instead" => Some(vec![Modifier::flag(GAINS_ONLY_COLD)]),
        // "{0}% increased Projectile Speed" (Spark's quality) and Projectile
        // Acceleration's "Supported Skills have {0}% more Projectile Speed".
        "base_projectile_speed_+%" => Some(vec![Modifier::number(
            "ProjectileSpeed",
            ModType::Inc,
            value,
        )]),
        "support_faster_projectiles_projectile_speed_+%_final" => Some(vec![Modifier::number(
            "ProjectileSpeed",
            ModType::More,
            value,
        )]),
        // A meta gem's triggered skills cost nothing (Fire Spell on Hit's
        // `no_cost`). It is their cost, not the meta gem's reservation of
        // Spirit, which `HasNoCost` would also waive.
        "no_cost" => Some(vec![Modifier::number("Cost", ModType::More, -100.0)]),
        // Efficiency: "Supported Skills have {0}% more Cost" / "less Cost",
        // of whatever the skill costs.
        "support_inspiration_cost_+%_final" => {
            Some(vec![Modifier::number("Cost", ModType::More, value)])
        }
        // "Supported Skills have {0}% increased Mana Cost Efficiency".
        "base_mana_cost_efficiency_+%" => Some(vec![Modifier::number(
            "ManaCostEfficiency",
            ModType::Inc,
            value,
        )]),
        // "Supported Skills have {0}% of Mana Costs Converted to Life Costs".
        "base_skill_cost_life_instead_of_mana_%" => Some(vec![Modifier::number(
            "HybridManaAndLifeCost_Life",
            ModType::Base,
            value,
        )]),
        // Says nothing about the curse the support applies to: the Witchtoads
        // use the skill the support grants (see `granted_skill_modifiers`).
        WITCHTOAD_SPAWN_INTERVAL_MS => Some(Vec::new()),
        // "Spells Cast by Totems have {0}% increased Cast Speed" and "Attacks
        // used by Totems have {0}% increased Attack Speed" (Urgent Totems):
        // for the skills a totem uses, as the same line on a passive is.
        "totem_skill_cast_speed_+%" => Some(vec![
            Modifier::number("CastSpeed", ModType::Inc, value)
                .with_flags(ModFlags::SPELL)
                .with_keyword_flags(KeywordFlags::TOTEM),
        ]),
        "totem_skill_attack_speed_+%" => Some(vec![
            Modifier::number("AttackSpeed", ModType::Inc, value)
                .with_keyword_flags(KeywordFlags::TOTEM),
        ]),
        // Rain of Arrows: "One arrow lands every {0} seconds", with
        // "Modifiers to Projectile speed apply to arrow landing frequency"
        // (`calc::rain`).
        "rain_of_arrows_delay_per_arrow" => Some(vec![Modifier::number(
            crate::calc::rain::LANDING_INTERVAL,
            ModType::Base,
            value,
        )]),
        // "Can Hit each enemy only once every {0} seconds", of what a rain or
        // a storm drops. Only a rain whose landings the engine knows reads it.
        "rain_hit_delay_ms" => Some(vec![Modifier::number(
            crate::calc::rain::HIT_INTERVAL,
            ModType::Base,
            value,
        )]),
        // Spell Echo: "Supported Spells Echo {0} times". Each echo repeats
        // the spell, so a use under a cooldown is that many more casts
        // (`calc::offence::apply_cooldown_cap`).
        "support_spell_echo_number_of_echo_cascades" => {
            Some(vec![Modifier::number("RepeatCount", ModType::Base, value)])
        }
        // Rigwald's Ferocity: increased Attack Speed and more Damage for
        // Supported Skills, each only while using one of the two weapon sets.
        "support_rigwald_attack_speed_+%_in_weapon_set_one" => Some(vec![
            Modifier::number("AttackSpeed", ModType::Inc, value)
                .with_tag(while_using_weapon_set(1)),
        ]),
        "support_rigwald_attack_speed_+%_in_weapon_set_two" => Some(vec![
            Modifier::number("AttackSpeed", ModType::Inc, value)
                .with_tag(while_using_weapon_set(2)),
        ]),
        "support_rigwald_damage_+%_final_in_weapon_set_one" => Some(vec![
            Modifier::number("Damage", ModType::More, value).with_tag(while_using_weapon_set(1)),
        ]),
        "support_rigwald_damage_+%_final_in_weapon_set_two" => Some(vec![
            Modifier::number("Damage", ModType::More, value).with_tag(while_using_weapon_set(2)),
        ]),
        _ => None,
    }
}

/// "while using Weapon Set `set`": the condition the build sets for the
/// weapon set in use, `WeaponSet1` or `WeaponSet2`.
fn while_using_weapon_set(set: u8) -> ModTag {
    ModTag::condition(format!("WeaponSet{set}"), false)
}

/// The modifiers the stat `stat` of a support grants, at `value`, to the
/// active skill that the same gem grants. `stats` is every stat of the
/// support. Empty when this table has nothing to say about the stat.
pub fn granted_skill_modifiers(stat: &str, value: f64, stats: &[SkillDamageStat]) -> Vec<Modifier> {
    let sibling = |id: &str| stats.iter().find(|ds| ds.stat == id).map(|ds| ds.value);
    match stat {
        // Doedre's Undoing: "Each Cursed Ground area from Supported Skills
        // spawns a Witchtoad every {0} seconds", with "Limit {0} areas of
        // Cursed Ground". The skill the gem grants, Dark Consequences, "is
        // Triggered when Witchtoads leap at enemies": it is used once per
        // Witchtoad, so at the rate they spawn, whatever the character's
        // cast speed. The enemy is taken to stand in every area the
        // character may have.
        WITCHTOAD_SPAWN_INTERVAL_MS if value > 0.0 => {
            let areas = sibling("maximum_curse_zones_allowed")
                .unwrap_or(1.0)
                .max(1.0);
            vec![Modifier::number(
                "SkillUseRate",
                ModType::Base,
                areas * 1000.0 / value,
            )]
        }
        _ => Vec::new(),
    }
}

/// Predator's Mark, the mark Pounce applies: "Marked target takes {0}%
/// increased damage for each of its enemies within 6 metres of it, up to
/// {1}%". The stat of `{0}`.
/// Oil Barrage: its Oil Ground applies Exposure, lowering each elemental
/// Resistance of the enemies in it by this much.
const OIL_EXPOSURE: &str = "skill_base_oil_exposure_-_to_total_elemental_resistance";

/// The modifiers the stat `stat` of a skill puts on the enemies it affects at
/// `value`, its quality added, when this table defines the stat. They are the
/// skill's debuff (`skill_env::buff_specs::buff_skill_specs`). `None` leaves
/// the stat to the extracted stat map.
pub fn debuff_modifiers(stat: &str, value: f64) -> Option<Vec<Modifier>> {
    match stat {
        // The enemy is taken to stand in the Oil. Exposure of each element
        // is reduced with the others (`calc::setup_env::reduce_enemy_exposure`).
        OIL_EXPOSURE => Some(
            ["FireExposure", "ColdExposure", "LightningExposure"]
                .into_iter()
                .map(|name| Modifier::number(name, ModType::Base, value))
                .collect(),
        ),
        _ => None,
    }
}

const PREDATORS_MARK_PER_ENEMY: &str = "skill_wolf_mark_damage_taken_+%_per_nearby_enemy";

/// The stat of `{1}` in [`PREDATORS_MARK_PER_ENEMY`]'s text, the maximum.
const PREDATORS_MARK_MAXIMUM: &str = "skill_wolf_mark_damage_taken_+%_cap";

/// The modifiers the stat `stat` of a curse or a mark puts on the enemy at
/// `value`, when this table defines the stat. `stats` is every stat of the
/// skill, each with its quality added. `None` leaves the stat to the
/// extracted stat map.
pub fn curse_modifiers(stat: &str, value: f64, stats: &[SkillDamageStat]) -> Option<Vec<Modifier>> {
    let sibling = |id: &str| stats.iter().find(|ds| ds.stat == id).map(|ds| ds.value);
    match stat {
        // Predator's Mark: the enemies of the marked target near it are the
        // character's minions in its Presence. The wolves Pounce summons
        // when the marked target dies come after it, so they are not among
        // them.
        PREDATORS_MARK_PER_ENEMY => Some(vec![
            Modifier::number("DamageTaken", ModType::Inc, value)
                .with_tag(per_minion_in_presence(sibling(PREDATORS_MARK_MAXIMUM))),
        ]),
        // The maximum of the line above.
        PREDATORS_MARK_MAXIMUM => Some(Vec::new()),
        _ => None,
    }
}

/// "for each Minion in your Presence", the total up to `maximum`. The count
/// is `MinionPresenceCount`, set by `CalculationSession::bridge_minion_presence`.
fn per_minion_in_presence(maximum: Option<f64>) -> ModTag {
    ModTag::Multiplier {
        var: "MinionPresenceCount".into(),
        div: 1.0,
        limit: maximum,
        actor: None,
        limit_var: None,
        limit_actor: None,
        invert: false,
        limit_total: true,
    }
}

/// Elemental Exposure a planted banner applies per 1000 of the character's
/// Armour (Refraction III).
pub const ELEMENTAL_EXPOSURE_PER_1000_ARMOUR: &str = "BannerElementalExposurePer1000Armour";

/// The player modifiers the stat of the buff skill `skill` grants at `value`,
/// when this table defines the stat. `None` leaves the stat to the extracted
/// stat map.
pub fn player_buff_modifiers(stat: &str, value: f64, skill: BuffSkill) -> Option<Vec<Modifier>> {
    match stat {
        // Refraction III: the banner applies Elemental Exposure to the
        // enemies in its area, this much per 1000 of the character's Armour,
        // while it is planted. The exposure is reduced with the others once
        // the Armour is known (`calc::setup_env::reduce_enemy_exposure`).
        "support_tempered_valour_banner_applies_%_elemental_exposure_per_1000_armour" => {
            Some(vec![
                Modifier::number(ELEMENTAL_EXPOSURE_PER_1000_ARMOUR, ModType::Base, value)
                    .with_tag(ModTag::condition("BannerPlanted", false)),
            ])
        }
        // Not a buff on the character (see `inflicts_withered`).
        stat if inflicts_withered(stat) => Some(Vec::new()),
        // Read where the buff is made (see `skill_modifiers`).
        REMNANT_EFFECT => Some(Vec::new()),
        // Into the Breach: a Purple Flame of Chayula "grants 7% of Damage as
        // extra Chaos Damage", a buff that stacks up to 10 times. How many
        // the character has is the configured number of Purple Flames
        // collected (`PurpleFlamesCount`). The stat is per Flame; the Flame
        // is a Remnant, so what raises the effect of Remnants raises it.
        "breach_flame_chaos_addition_%" => Some(vec![
            Modifier::number("DamageGainAsChaos", ModType::Base, value).with_tag(
                ModTag::multiplier(PURPLE_FLAMES, 1.0, Some(MAXIMUM_PURPLE_FLAMES)),
            ),
        ]),
        // Pain Offering: "Aura grants Minions {0}% increased damage" and
        // "Aura grants Minions {0}% increased Attack and Cast Speed". The
        // bone spike's aura affects minions, not the character: an aura of
        // the character's that only its allies have (`ExtraAura`).
        "pain_offering_damage_+%" => Some(vec![allies_aura(vec![Modifier::number(
            "Damage",
            ModType::Inc,
            value,
        )])]),
        "pain_offering_attack_and_cast_speed_+%" => Some(vec![allies_aura(vec![
            Modifier::number("AttackSpeed", ModType::Inc, value),
            Modifier::number("CastSpeed", ModType::Inc, value),
        ])]),
        // Cast on Critical, from its quality on a character whose gem quality
        // grants the additional effect: "Buff grants {0}% increased Critical
        // Hit chance".
        "cast_on_crit_global_critical_hit_chance_granted_+%" => Some(vec![Modifier::number(
            "CriticalStrikeChance",
            ModType::Inc,
            value,
        )]),
        // Archmage: "Non-Channelling Spells cost an additional {0}% of your
        // maximum Mana". The stat is in hundredths of a percent. The cost is
        // added to the spell's own after its supports' cost multipliers,
        // which are multipliers of the gem's cost.
        "archmage_max_mana_permyriad_to_add_to_non_channelled_spell_mana_cost" => {
            let spell = SkillTypes::from_pob2_name("Spell")?;
            let channel = SkillTypes::from_pob2_name("Channel")?;
            Some(vec![
                Modifier::number("ManaCostNoMult", ModType::Base, value / 100.0)
                    .with_tag(ModTag::SkillTypes(spell))
                    .with_tag(ModTag::SkillTypesNeg(channel))
                    .with_tag(ModTag::PercentStat {
                        stat: "Mana".into(),
                        percent: Some(1.0),
                    }),
            ])
        }
        // Sorcery Ward (see `sorcery_ward_barrier`).
        SORCERY_WARD_BARRIER => Some(sorcery_ward_barrier(value)),
        // Magma Barrier, "While active, increases your Block Chance
        // passively": "Grants {0}% increased Block chance".
        MAGMA_BARRIER_BLOCK => Some(vec![Modifier::number("BlockChance", ModType::Inc, value)]),
        // Not a buff on the character (see `granted_skill_modifiers`).
        WITCHTOAD_SPAWN_INTERVAL_MS => Some(Vec::new()),
        // Charge Regulation: "{0}% more Armour, Evasion and Energy Shield
        // while you have an Endurance Charge".
        "charge_mastery_armour_evasion_energy_shield_+%_final_with_endurance_charges" => Some(
            ["Armour", "Evasion", "EnergyShield"]
                .into_iter()
                .map(|defence| {
                    Modifier::number(defence, ModType::More, value)
                        .with_tag(while_holding("EnduranceCharge"))
                })
                .collect(),
        ),
        // "{0}% more Critical Hit Chance while you have a Power Charge".
        "charge_mastery_crit_chance_+%_final_with_power_charges" => Some(vec![
            Modifier::number("CriticalStrikeChance", ModType::More, value)
                .with_tag(while_holding("PowerCharge")),
        ]),
        // "{0}% increased Skill Speed while you have a Frenzy Charge".
        "charge_mastery_skill_speed_+%_with_frenzy_charges" => Some(
            SKILL_SPEED
                .into_iter()
                .map(|speed| {
                    Modifier::number(speed, ModType::Inc, value)
                        .with_tag(while_holding("FrenzyCharge"))
                })
                .collect(),
        ),
        // From quality: "Buff grants {0}% increased damage per Charge", of
        // any kind.
        "charge_regulation_damage_per_charge_granted_+%" => Some(
            ["PowerCharge", "FrenzyCharge", "EnduranceCharge"]
                .into_iter()
                .map(|charge| {
                    Modifier::number("Damage", ModType::Inc, value)
                        .with_tag(ModTag::multiplier(charge, 1.0, None))
                })
                .collect(),
        ),
        // "Consumes one of each Charge every {0} seconds": upkeep, with no
        // effect on what the character has while the buff is up.
        "consume_frenzy_power_and_endurance_charge_every_x_ms" => Some(Vec::new()),
        // Pinnacle of Power: "{0}% more Elemental Damage per Power Charge
        // Consumed" and "Buff causes all Elemental Damage to Contribute to
        // Flammability and Ignite Magnitudes, Shock chance, and Freeze
        // buildup". The skill consumes every Power Charge the character can
        // lose, so the count is the charges above the minimum. The damage is
        // by damage type: Fire, Cold and Lightning, whatever the skill.
        "elemental_power_elemental_damage_+%_final_per_power_charge" => Some(
            std::iter::once(
                Modifier::number(
                    "ElementalDamage",
                    ModType::More,
                    value * skill.consumed_charge_benefit(),
                )
                .with_tag(ModTag::multiplier("RemovablePowerCharge", 1.0, None)),
            )
            .chain(every_element_contributes_to_every_elemental_ailment())
            .collect(),
        ),
        // Charged Staff: "Buff grants {0} to {1} Added Quarterstaff Attack
        // Lightning damage per Power Charge Consumed, up to your maximum
        // Power Charges".
        "charged_staff_attack_minimum_added_lightning_damage_per_stack" => {
            Some(vec![charged_staff_added_lightning(
                "LightningDamageMin",
                value,
                skill,
            )])
        }
        "charged_staff_attack_maximum_added_lightning_damage_per_stack" => {
            Some(vec![charged_staff_added_lightning(
                "LightningDamageMax",
                value,
                skill,
            )])
        }
        // "Buff duration is {0} seconds per Power Charge Consumed": how long
        // the buff lasts, with no effect on what it grants while it is up.
        "charged_staff_buff_duration_per_stack_ms" => Some(Vec::new()),
        // Barrage: "Empowered Attacks Repeat {0:+d} times, and a further
        // {1:+d} times per Frenzy Charge consumed" (this is {1}). The skill
        // consumes every Frenzy Charge the character can lose, so the count is
        // the charges above the minimum; a consumed charge whose benefit is
        // doubled adds twice the repeats.
        "empower_barrage_number_of_barrage_repeats_per_frenzy_charge" => Some(vec![
            Modifier::number(
                "BarrageRepeats",
                ModType::Base,
                value * skill.consumed_charge_benefit(),
            )
            .with_tag(ModTag::multiplier("RemovableFrenzyCharge", 1.0, None)),
        ]),
        _ => None,
    }
}

/// One end of the Lightning damage Charged Staff adds to Quarterstaff
/// attacks, `value` per Power Charge consumed: `LightningDamageMin` or
/// `LightningDamageMax`, shaped as the line "Adds # to # Lightning Damage to
/// Quarterstaff Attacks" parses. The skill consumes every Power Charge the
/// character can lose, so the count is the charges above the minimum. The
/// text caps it at the maximum Power Charges, so a charge that only counts
/// as consumed ("Quarterstaff Skills that consume Power Charges count as
/// consuming an additional Power Charge") adds nothing at the maximum;
/// below it, that charge is not modelled.
fn charged_staff_added_lightning(name: &str, value: f64, skill: BuffSkill) -> Modifier {
    let per_charge = value * skill.consumed_charge_benefit();
    Modifier::number(name, ModType::Base, per_charge)
        .with_flags(ModFlags::STAFF)
        .with_tag(ModTag::DamageType(DamageType::Lightning))
        .with_tag(ModTag::condition("UsingQuarterstaff", false))
        .with_tag(ModTag::multiplier("RemovablePowerCharge", 1.0, None))
}

/// The stat behind Magma Barrier's "Grants {0}% increased Block chance" (25,
/// plus 0.25 per quality).
const MAGMA_BARRIER_BLOCK: &str = "skill_igneous_shield_grants_block_chance_+%";

/// The stat behind Sorcery Ward's "Barrier can take Elemental Damage equal to
/// {0}% of your Armour and Evasion Rating".
const SORCERY_WARD_BARRIER: &str = "aegis_unique_shield_max_value_from_%_armour_evasion";

/// The condition "Grants Skill: Sorcery Ward" sets (`Condition:SorceryWard`,
/// bridged by `CalculationSession::bridge_player_conditions`). Sorcery Ward is
/// an Ascendancy skill: the character has it only while something grants it
/// (the Witchhunter's Obsessive Rituals), so a skill group that lists the gem
/// without the grant gives no Barrier.
pub const SORCERY_WARD: &str = "SorceryWard";

/// The condition "Sorcery Ward's Barrier can also take Physical and Chaos
/// Damage from Hits" sets (`Condition:CeremonialAblution`, bridged by
/// `CalculationSession::bridge_player_conditions`).
pub const CEREMONIAL_ABLUTION: &str = "CeremonialAblution";

/// Sorcery Ward: "Elemental Damage from Hits is taken from the Barrier before
/// your Life, Mana or Energy Shield. Barrier can take Elemental Damage equal
/// to {0}% of your Armour and Evasion Rating, up to a maximum of 32,000".
/// With "Sorcery Ward's Barrier can also take Physical and Chaos Damage from
/// Hits" the same Barrier takes every type of damage. The engine's aegis pools
/// take the strongest source, so a second Sorcery Ward does not add to it. The
/// maximum of 32,000 is not applied: no tag caps a percentage of a stat.
fn sorcery_ward_barrier(percent: f64) -> Vec<Modifier> {
    let of_armour_and_evasion = || ModTag::PercentStat {
        stat: "Armour|Evasion".into(),
        percent: Some(1.0),
    };
    vec![
        Modifier::number("ElementalAegisValue", ModType::Base, percent)
            .with_tag(ModTag::condition(SORCERY_WARD, false))
            .with_tag(ModTag::condition(CEREMONIAL_ABLUTION, true))
            .with_tag(of_armour_and_evasion()),
        Modifier::number("AegisValue", ModType::Base, percent)
            .with_tag(ModTag::condition(SORCERY_WARD, false))
            .with_tag(ModTag::condition(CEREMONIAL_ABLUTION, false))
            .with_tag(of_armour_and_evasion()),
    ]
}

/// What "Skill Speed" is: the speed of attacks and casts, of Warcries and of
/// placing Totems.
pub(crate) const SKILL_SPEED: [&str; 3] = ["SkillSpeed", "WarcrySpeed", "TotemPlacementSpeed"];

/// "while you have a `charge`": at least one charge of that kind
/// (`PowerCharge`, `FrenzyCharge` or `EnduranceCharge`).
fn while_holding(charge: &str) -> ModTag {
    ModTag::MultiplierThreshold {
        var: charge.into(),
        threshold: 1.0,
        upper: false,
    }
}

/// The flags that let each element contribute to the ailments of the other
/// two, e.g. `ColdCanIgnite`.
fn every_element_contributes_to_every_elemental_ailment() -> impl Iterator<Item = Modifier> {
    ELEMENTAL_AILMENTS.iter().flat_map(|(_, ailment)| {
        ELEMENTAL_AILMENTS
            .iter()
            .filter(move |(_, own)| own != ailment)
            .map(move |(element, _)| Modifier::flag(format!("{element}Can{ailment}").as_str()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CalcConfig, ModDb};

    const PER_POWER_CHARGE: &str = "elemental_power_elemental_damage_+%_final_per_power_charge";

    /// The more Elemental Damage Pinnacle of Power grants `skill` after
    /// consuming four Power Charges.
    fn more_elemental_damage(skill: BuffSkill) -> ModDb {
        let mut db = ModDb::new();
        db.add_list(player_buff_modifiers(PER_POWER_CHARGE, 15.0, skill).expect("a rule"));
        db
    }

    fn four_consumed() -> CalcConfig {
        CalcConfig::new().with_multiplier("RemovablePowerCharge", 4.0)
    }

    /// A meta gem's triggered skills cost nothing, while the meta gem still
    /// reserves its Spirit.
    #[test]
    fn a_skill_with_no_cost_waives_its_cost_not_a_reservation() {
        let mods = skill_modifiers("no_cost", 1.0).expect("defined");
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name.as_str(), "Cost");
        assert_eq!(mods[0].mod_type, ModType::More);
        assert_eq!(mods[0].value.as_number(), Some(-100.0));
    }

    #[test]
    fn spells_have_culling_strike_only_grants_culling_to_spells() {
        let mut db = ModDb::new();
        db.add_list(skill_modifiers("spells_have_culling_strike", 1.0).expect("defined"));
        assert!(db.flag(&CalcConfig::new().with_flags(ModFlags::SPELL), "CanCull"));
        assert!(!db.flag(&CalcConfig::new().with_flags(ModFlags::ATTACK), "CanCull"));
    }

    /// The damage type is in the stat id, or, for Fiery Death's
    /// `corpse_explosion_monster_life_%`, in its text: "base Fire Damage".
    #[test]
    fn a_corpse_explosion_deals_the_type_its_stat_names() {
        let cases = [
            (
                "corpse_explosion_monster_life_permillage_physical",
                60.0,
                DamageType::Physical,
                0.06,
            ),
            (
                "corpse_explosion_monster_life_permillage_chaos",
                100.0,
                DamageType::Chaos,
                0.1,
            ),
            (
                "corpse_explosion_monster_life_%",
                8.0,
                DamageType::Fire,
                0.08,
            ),
            (
                "corpse_explosion_monster_life_%_lightning",
                5.0,
                DamageType::Lightning,
                0.05,
            ),
        ];
        for (stat, value, damage_type, share) in cases {
            assert_eq!(
                corpse_explosion_damage(stat, value),
                Some((damage_type, share)),
                "{stat}"
            );
        }
        assert_eq!(
            corpse_explosion_damage("corpse_explosion_monster_life_%_x", 5.0),
            None
        );
        // Decompose poisons with what it deals.
        let mut db = ModDb::new();
        db.add_list(
            skill_modifiers("corpse_explosion_monster_life_permillage_chaos", 100.0)
                .expect("a rule"),
        );
        assert_eq!(
            db.sum(
                ModType::Base,
                &CalcConfig::new(),
                &[ModName::from("PoisonChance")]
            ),
            100.0
        );
    }

    #[test]
    fn pinnacle_of_power_is_more_elemental_damage_per_consumed_power_charge() {
        let db = more_elemental_damage(BuffSkill::default());
        let elemental = [ModName::from("ElementalDamage")];
        assert!((db.more(&four_consumed(), &elemental) - 1.6).abs() < 1e-12);
        // No charge to consume, no damage. The buff is by damage type, so
        // nothing lands on damage in general.
        assert_eq!(db.more(&CalcConfig::new(), &elemental), 1.0);
        assert_eq!(db.more(&four_consumed(), &[ModName::from("Damage")]), 1.0);
    }

    /// Charge Regulation grants each of its three effects only while the
    /// character holds a charge of the matching kind.
    #[test]
    fn charge_regulation_needs_a_charge_of_the_matching_kind() {
        let buff = |stat: &str, value: f64| {
            let mut db = ModDb::new();
            db.add_list(player_buff_modifiers(stat, value, BuffSkill::default()).expect("a rule"));
            db
        };
        let holding = |charge: &str| CalcConfig::new().with_multiplier(charge, 1.0);
        let none = CalcConfig::new();

        let defences = buff(
            "charge_mastery_armour_evasion_energy_shield_+%_final_with_endurance_charges",
            19.0,
        );
        for name in ["Armour", "Evasion", "EnergyShield"] {
            let names = [ModName::from(name)];
            assert!((defences.more(&holding("EnduranceCharge"), &names) - 1.19).abs() < 1e-12);
            assert_eq!(
                defences.more(&holding("PowerCharge"), &names),
                1.0,
                "{name}"
            );
            assert_eq!(defences.more(&none, &names), 1.0, "{name}");
        }

        let crit = buff(
            "charge_mastery_crit_chance_+%_final_with_power_charges",
            25.0,
        );
        let names = [ModName::from("CriticalStrikeChance")];
        assert!((crit.more(&holding("PowerCharge"), &names) - 1.25).abs() < 1e-12);
        assert_eq!(crit.more(&holding("FrenzyCharge"), &names), 1.0);

        let speed = buff("charge_mastery_skill_speed_+%_with_frenzy_charges", 25.0);
        for name in SKILL_SPEED {
            let names = [ModName::from(name)];
            assert_eq!(
                speed.sum(ModType::Inc, &holding("FrenzyCharge"), &names),
                25.0
            );
            assert_eq!(speed.sum(ModType::Inc, &none, &names), 0.0, "{name}");
        }
    }

    /// Its quality counts every charge the character holds, of any kind.
    #[test]
    fn charge_regulation_quality_is_damage_per_charge_of_any_kind() {
        let mut db = ModDb::new();
        db.add_list(
            player_buff_modifiers(
                "charge_regulation_damage_per_charge_granted_+%",
                2.0,
                BuffSkill::default(),
            )
            .expect("a rule"),
        );
        let cfg = CalcConfig::new()
            .with_multiplier("PowerCharge", 3.0)
            .with_multiplier("EnduranceCharge", 4.0);
        assert_eq!(db.sum(ModType::Inc, &cfg, &[ModName::from("Damage")]), 14.0);
    }

    /// Heightened Charges: a 20% chance to double the benefit is, on
    /// average, 20% more of it. Past 100% there is nothing left to double.
    #[test]
    fn a_chance_to_double_the_benefit_of_consumed_charges_raises_it_on_average() {
        let more = |chance: f64| {
            more_elemental_damage(BuffSkill {
                consumed_charge_benefit_double_chance: chance,
            })
            .more(&four_consumed(), &[ModName::from("ElementalDamage")])
        };
        assert!((more(20.0) - 1.72).abs() < 1e-12);
        assert!((more(100.0) - 2.2).abs() < 1e-12);
        assert!((more(250.0) - 2.2).abs() < 1e-12);
    }

    /// Charged Staff at level 23, 2 to 30 per stack, after consuming six
    /// Power Charges: 12 to 180 Lightning damage for Quarterstaff attacks.
    #[test]
    fn charged_staff_adds_lightning_to_quarterstaff_attacks_per_consumed_power_charge() {
        let mut db = ModDb::new();
        for (stat, value) in [
            (
                "charged_staff_attack_minimum_added_lightning_damage_per_stack",
                2.0,
            ),
            (
                "charged_staff_attack_maximum_added_lightning_damage_per_stack",
                30.0,
            ),
        ] {
            db.add_list(player_buff_modifiers(stat, value, BuffSkill::default()).expect("a rule"));
        }
        let added = |cfg: &CalcConfig| {
            (
                db.sum(ModType::Base, cfg, &[ModName::from("LightningDamageMin")]),
                db.sum(ModType::Base, cfg, &[ModName::from("LightningDamageMax")]),
            )
        };
        let quarterstaff_attack = |consumed: f64| {
            CalcConfig::new()
                .with_flags(ModFlags::ATTACK | ModFlags::HIT | ModFlags::STAFF)
                .with_damage_type(DamageType::Lightning)
                .with_condition("UsingQuarterstaff", true)
                .with_multiplier("RemovablePowerCharge", consumed)
        };
        assert_eq!(added(&quarterstaff_attack(6.0)), (12.0, 180.0));
        // No charge consumed, nothing added.
        assert_eq!(added(&quarterstaff_attack(0.0)), (0.0, 0.0));
        // Only Quarterstaff attacks gain it.
        let other_weapon = quarterstaff_attack(6.0).with_condition("UsingQuarterstaff", false);
        assert_eq!(added(&other_weapon), (0.0, 0.0));
        // The buff's duration grants nothing while the buff is up.
        let duration = player_buff_modifiers(
            "charged_staff_buff_duration_per_stack_ms",
            6000.0,
            BuffSkill::default(),
        );
        assert_eq!(duration, Some(Vec::new()));
    }

    #[test]
    fn pinnacle_of_power_lets_each_element_contribute_to_the_other_ailments() {
        let mods =
            player_buff_modifiers(PER_POWER_CHARGE, 15.0, BuffSkill::default()).expect("a rule");
        let mut flags: Vec<&str> = mods
            .iter()
            .filter(|modifier| modifier.mod_type == ModType::Flag)
            .map(|modifier| modifier.name.as_str())
            .collect();
        flags.sort_unstable();
        assert_eq!(
            flags,
            [
                "ColdCanIgnite",
                "ColdCanShock",
                "FireCanFreeze",
                "FireCanShock",
                "LightningCanFreeze",
                "LightningCanIgnite",
            ]
        );
    }

    #[test]
    fn a_stat_without_a_rule_is_left_to_the_stat_map() {
        let skill = BuffSkill::default();
        assert!(player_buff_modifiers("base_skill_effect_duration", 5000.0, skill).is_none());
        assert!(skill_modifiers("base_skill_effect_duration", 5000.0).is_none());
        assert!(granted_skill_modifiers("base_skill_effect_duration", 5000.0, &[]).is_empty());
        assert!(curse_modifiers("base_skill_effect_duration", 5000.0, &[]).is_none());
    }

    /// Predator's Mark at 20 quality: 4% increased damage taken for each of
    /// the character's minions in its Presence, up to 60%.
    #[test]
    fn predators_mark_counts_the_minions_in_presence_up_to_its_maximum() {
        let stats = [
            stat(PREDATORS_MARK_PER_ENEMY, 4.0),
            stat(PREDATORS_MARK_MAXIMUM, 60.0),
        ];
        let mut db = ModDb::new();
        for ds in &stats {
            db.add_list(curse_modifiers(&ds.stat, ds.value, &stats).expect("a rule"));
        }
        let taken = |cfg: &CalcConfig| db.sum(ModType::Inc, cfg, &[ModName::from("DamageTaken")]);
        let minions = |count: f64| CalcConfig::new().with_multiplier("MinionPresenceCount", count);
        assert_eq!(taken(&minions(7.0)), 28.0);
        // Twenty minions reach the maximum.
        assert_eq!(taken(&minions(20.0)), 60.0);
        // The mark adds nothing when the character has no minions.
        assert_eq!(taken(&minions(0.0)), 0.0);
        assert_eq!(taken(&CalcConfig::new()), 0.0);
    }

    /// "Summons {0} Hyenas" is how many Hyenas Cackling Companions creates,
    /// counted where they are created. It grants the skill nothing.
    #[test]
    fn the_number_of_hyenas_is_read_where_they_are_created() {
        assert!(is_summoned_minion_count("skill_hyena_cackle_size"));
        assert_eq!(
            skill_modifiers("skill_hyena_cackle_size", 7.0),
            Some(Vec::new())
        );
    }

    /// The stats of supports that change what the skill they apply to costs.
    #[test]
    fn support_cost_stats_become_the_modifiers_the_cost_reads() {
        let only = |stat: &str, value: f64| {
            let mut mods = skill_modifiers(stat, value).expect("a rule");
            assert_eq!(mods.len(), 1, "{stat}");
            let modifier = mods.remove(0);
            (
                modifier.name.to_string(),
                modifier.mod_type,
                modifier.value.as_number(),
            )
        };
        assert_eq!(
            only("support_inspiration_cost_+%_final", -40.0),
            ("Cost".to_string(), ModType::More, Some(-40.0))
        );
        assert_eq!(
            only("base_mana_cost_efficiency_+%", 25.0),
            ("ManaCostEfficiency".to_string(), ModType::Inc, Some(25.0))
        );
        assert_eq!(
            only("base_skill_cost_life_instead_of_mana_%", 100.0),
            (
                "HybridManaAndLifeCost_Life".to_string(),
                ModType::Base,
                Some(100.0)
            )
        );
    }

    #[test]
    fn cast_on_critical_can_grant_increased_critical_hit_chance() {
        let stat = "cast_on_crit_global_critical_hit_chance_granted_+%";
        let mut db = ModDb::new();
        db.add_list(player_buff_modifiers(stat, 23.0, BuffSkill::default()).expect("a rule"));
        let names = [ModName::from("CriticalStrikeChance")];
        assert_eq!(db.sum(ModType::Inc, &CalcConfig::new(), &names), 23.0);
    }

    /// Archmage at 4.10% of 4000 maximum Mana: 164 more Mana for a spell,
    /// nothing for an attack or a channelled spell.
    #[test]
    fn archmage_adds_a_share_of_maximum_mana_to_the_cost_of_non_channelling_spells() {
        let stat = "archmage_max_mana_permyriad_to_add_to_non_channelled_spell_mana_cost";
        let mut db = ModDb::new();
        db.add_list(player_buff_modifiers(stat, 410.0, BuffSkill::default()).expect("a rule"));
        let added = |types: &[&str]| {
            let mut cfg = CalcConfig::new();
            cfg.stats.insert("Mana".into(), 4000.0);
            for name in types {
                cfg.skill_types |= SkillTypes::from_pob2_name(name).unwrap();
            }
            db.sum(ModType::Base, &cfg, &[ModName::from("ManaCostNoMult")])
        };
        assert_eq!(added(&["Spell"]), 164.0);
        assert_eq!(added(&["Attack"]), 0.0);
        assert_eq!(added(&["Spell", "Channel"]), 0.0);
    }

    /// Sorcery Ward at 31% of 18000 Armour and Evasion Rating: a 5580
    /// Barrier for Elemental Damage, or for every type of damage once
    /// "Sorcery Ward's Barrier can also take Physical and Chaos Damage from
    /// Hits".
    #[test]
    fn sorcery_ward_barrier_is_a_share_of_armour_and_evasion() {
        let mut db = ModDb::new();
        db.add_list(
            player_buff_modifiers(SORCERY_WARD_BARRIER, 31.0, BuffSkill::default())
                .expect("a rule"),
        );
        // (Barrier for Elemental Damage, Barrier for every type of damage).
        let barrier = |cfg: &CalcConfig| {
            let strongest = |name: &str| db.max_of(ModType::Base, cfg, &[ModName::from(name)]);
            (strongest("ElementalAegisValue"), strongest("AegisValue"))
        };
        let granted = || CalcConfig::new().with_condition(SORCERY_WARD, true);
        let armour = granted().with_stat("Armour", 18000.0);
        assert_eq!(barrier(&armour), (5580.0, 0.0));
        let split = granted()
            .with_stat("Armour", 9000.0)
            .with_stat("Evasion", 9000.0);
        assert_eq!(barrier(&split), (5580.0, 0.0));
        let ablution = armour.with_condition(CEREMONIAL_ABLUTION, true);
        assert_eq!(barrier(&ablution), (0.0, 5580.0));
        // No defences, no Barrier.
        assert_eq!(barrier(&granted()), (0.0, 0.0));
        // A skill group that lists the gem without the grant: no Barrier.
        let listed_only = CalcConfig::new().with_stat("Armour", 18000.0);
        assert_eq!(barrier(&listed_only), (0.0, 0.0));
    }

    /// Magma Barrier's buff is increased Block chance, which multiplies a
    /// shield's own.
    #[test]
    fn magma_barrier_grants_increased_block_chance() {
        let mut db = ModDb::new();
        db.add_list(
            player_buff_modifiers(MAGMA_BARRIER_BLOCK, 30.0, BuffSkill::default()).expect("a rule"),
        );
        let increased = db.sum(
            ModType::Inc,
            &CalcConfig::new(),
            &[ModName::from("BlockChance")],
        );
        assert_eq!(increased, 30.0);
    }

    fn stat(id: &str, value: f64) -> SkillDamageStat {
        SkillDamageStat {
            stat: id.into(),
            value,
        }
    }

    /// The use rate `stats` of Doedre's Undoing give the skill its gem grants.
    fn witchtoad_use_rate(stats: &[SkillDamageStat]) -> f64 {
        let mut db = ModDb::new();
        for ds in stats {
            db.add_list(granted_skill_modifiers(&ds.stat, ds.value, stats));
        }
        db.sum(
            ModType::Base,
            &CalcConfig::new(),
            &[ModName::from("SkillUseRate")],
        )
    }

    /// One Witchtoad every 0.25 seconds in each of two areas: eight a second.
    #[test]
    fn witchtoads_use_the_skill_their_gem_grants_at_the_rate_they_spawn() {
        let two_areas = [
            stat("maximum_curse_zones_allowed", 2.0),
            stat("max_toads_per_doedre_cursed_ground", 4.0),
            stat(WITCHTOAD_SPAWN_INTERVAL_MS, 250.0),
        ];
        assert_eq!(witchtoad_use_rate(&two_areas), 8.0);
        assert_eq!(
            witchtoad_use_rate(&[stat(WITCHTOAD_SPAWN_INTERVAL_MS, 500.0)]),
            2.0
        );
    }

    /// The interval says nothing about the curse the support applies to, and
    /// the Witchtoads are not a buff on the character.
    #[test]
    fn the_witchtoad_interval_grants_the_supported_curse_and_the_character_nothing() {
        assert_eq!(
            skill_modifiers(WITCHTOAD_SPAWN_INTERVAL_MS, 250.0),
            Some(Vec::new())
        );
        let buff = player_buff_modifiers(WITCHTOAD_SPAWN_INTERVAL_MS, 250.0, BuffSkill::default());
        assert_eq!(buff, Some(Vec::new()));
    }

    /// Muster's more damage is per different kind of Reviving Minion; the
    /// summoning skill's minion stats reach the minions, and the skill itself
    /// is left nothing to report.
    #[test]
    fn minion_stats_are_for_the_minions() {
        const MUSTER: &str =
            "support_varied_troops_damage_+%_final_per_different_persistent_ominion";
        let three = SummonedMinions {
            reviving_kinds: 3,
            ..Default::default()
        };
        let mut db = ModDb::new();
        db.add_list(minion_modifiers(MUSTER, 7.0, three).expect("a rule"));
        let damage = [ModName::from("Damage")];
        assert!((db.more(&CalcConfig::new(), &damage) - 1.21).abs() < 1e-12);
        let none = minion_modifiers(MUSTER, 7.0, SummonedMinions::default()).expect("a rule");
        assert_eq!(none[0].value.as_number(), Some(0.0));

        assert!(is_minion_stat("active_skill_minion_damage_+%_final"));
        assert_eq!(
            skill_modifiers("active_skill_minion_damage_+%_final", 50.0),
            Some(Vec::new())
        );
        assert!(!is_minion_stat("active_skill_damage_+%_final"));
    }

    #[test]
    fn minion_attribute_bonuses_count_complete_player_attribute_groups() {
        let facts = SummonedMinions {
            player_strength: 127.0,
            player_dexterity: 19.0,
            ..Default::default()
        };
        for (stat, divisor, name, expected) in [
            (
                "minion_1%_damage_+%_per_X_player_strength",
                10.0,
                "Damage",
                12.0,
            ),
            (
                "active_skill_minion_1%_attack_speed_+%_per_X_player_dexterity",
                3.0,
                "AttackSpeed",
                6.0,
            ),
            (
                "minion_1%_area_of_effect_+%_per_X_player_dexterity",
                3.0,
                "AreaOfEffect",
                6.0,
            ),
        ] {
            let mods = minion_modifiers(stat, divisor, facts).expect("minion stat");
            assert_eq!(mods[0].name.as_str(), name);
            assert_eq!(mods[0].value.as_number(), Some(expected));
            assert_eq!(
                skill_modifiers(stat, divisor),
                Some(Vec::new()),
                "the summoner gains nothing"
            );
        }
    }
}
