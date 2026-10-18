//! Actor-derived snapshots and conditions shared by full-build calculation.
//!
//! The initial snapshot precedes buff expansion and resource conversion, preserving
//! PoB-compatible evaluation order. `perform` refreshes Life/Mana after conversion.
//! Both snapshots use the same core pool calculation; build only supplies source facts.

use super::*;
use pobr_data::constants::BASE_REQUIRED_ENEMIES_TO_BE_SURROUNDED;
use pobr_data::source::{ModifierSource, SourceId, SourceKind};

pub(in crate::calc) fn life_mana_pools(env: &Env) -> (f64, f64) {
    let pool =
        |base, name| crate::calc::offence::scaled_pool(&env.player.mod_db, &env.cfg, base, name);
    (
        pool(env.player.base.life, "MaximumLife"),
        pool(env.player.base.mana, "MaximumMana"),
    )
}

impl CalculationSession {
    /// Settles a dual-wield equipment pair that requires Giant's Blood.
    /// Called after all sources arrive, before attribute/reservation reads.
    /// The candidate weapons keep their two-handed type and local modifiers;
    /// without the source, only the candidate off-hand attack is retired.
    pub fn prepare_giants_blood_dual_wielding(&mut self) {
        crate::calc::keystone_merge::merge_keystones(&mut self.env);
        let enabled = self.has_flag("GiantsBlood");
        self.set_condition("DualWielding", enabled);
        if !enabled && self.env.hand_sources.len() == 2 {
            self.env
                .hand_sources
                .retain(|source| source.label != crate::HandTag::OffHand);
        }
    }

    /// Derives attribute bonuses and captures the initial actor stat/multiplier values.
    /// Call once after source injection and the build's equipment facts (attributes can
    /// scale on them, as "per Socket filled" does), and before condition bridging.
    /// `class` contains the starting attributes already baked into CharacterBase mods;
    /// `None` disables attribute derivation for callers supplying their own base input.
    pub fn prepare_player_stats(&mut self, level: u32, class: Option<crate::CharacterBase>) {
        // A keystone an item grants (a jewel's "Giant's Blood") can change
        // what the attributes grant ("Inherent Life granted by Strength is
        // halved"), so it joins before they are derived; the merges of
        // `perform` skip the keystones already added.
        crate::calc::keystone_merge::merge_keystones(&mut self.env);
        let bases = class.map_or([0.0; 3], |c| [c.strength, c.dexterity, c.intelligence]);
        let attributes = [
            self.attribute_total("Strength", bases[0]),
            self.attribute_total("Dexterity", bases[1]),
            self.attribute_total("Intelligence", bases[2]),
        ];
        if let Some(class) = class {
            self.derive_attribute_bonuses(class, attributes);
        }
        // Derived resources and per-attribute modifiers must read the same final
        // attributes, including class bases and INC/MORE scaling.
        let [str_total, dex_total, int_total] = attributes;
        self.set_multiplier("Strength", str_total);
        self.set_multiplier("Dexterity", dex_total);
        self.set_multiplier("Intelligence", int_total);
        self.set_stat("Strength", str_total);
        self.set_stat("Dexterity", dex_total);
        self.set_stat("Intelligence", int_total);
        // "per 5 of your lowest Attribute" (Jack of all Trades): the least of
        // the three final attributes.
        let lowest = str_total.min(dex_total).min(int_total);
        self.set_multiplier("LowestAttribute", lowest);
        self.set_stat("LowestAttribute", lowest);
        // (Pre-existing #7-4) The Spirit denominator = **the final pool value**
        // (calc_spirit_pool, including INC/MORE and conversion deductions) — vendor's
        // PerStat reads output.Spirit; BASE-only would under-count wolf-pack's Perfidy
        // "+2 Armour per 1 Spirit" by 72 base (Spirit 336 vs base 300).
        let spirit_total = self.spirit_total();
        let (life_total, mana_total) = life_mana_pools(&self.env);
        self.set_multiplier("Spirit", spirit_total);
        self.set_multiplier("Mana", mana_total);
        self.set_multiplier("Life", life_total);
        self.set_multiplier("Level", f64::from(level));
        // cfg.stats snapshot backfill (a value-mirroring copy): the fetch channel for
        // PerStat/PercentStat (EvalContext::stat falls back to cfg.stats) and
        // StatThreshold (the matches gate), sharing the same key space as the multiplier
        // side (aligned after special_mod::normalize_stat_name normalization). Only
        // backfills the subset computable before perform; globals only computable inside
        // perform (Armour/ES etc.) stay 0 (see CalcConfig::stats's doc).
        let tribute = self.base_sum("Tribute");
        self.set_stat("Tribute", tribute);
        self.set_multiplier("Tribute", tribute);
        self.set_stat("Spirit", spirit_total);
        self.set_stat("Mana", mana_total);
        self.set_stat("Life", life_total);
        // The main skill's Life cost snapshot (matching vendor's output.LifeCost): the
        // fetch source for per-life-cost mods (PerStat stat=LifeCost, e.g. Atalui's
        // Bloodletting's gain-as-physical). Cost is resolved before damage, matching
        // vendor's CalcOffence ordering.
        let life_cost = self.life_cost_snapshot();
        if life_cost > 0.0 {
            self.set_stat("LifeCost", life_cost);
            self.set_multiplier("LifeCost", life_cost);
        }
    }

    fn derive_attribute_bonuses(&mut self, class: crate::CharacterBase, attributes: [f64; 3]) {
        let cc = &self.env.cfg.constants.character_constants;
        let (cls_str, cls_dex, cls_int) = (class.strength, class.dexterity, class.intelligence);
        let [str_total, dex_total, int_total] = attributes;
        // (Pre-existing #7-4) The Giant's Blood keystone's "Inherent Life granted by
        // Strength is halved" (matching vendor CalcPerform.lua:500-505: the
        // HalvesLifeFromStrength flag → `Life BASE = Str × 1` instead of ×2).
        // CharacterBase already bakes in the class-starting segment
        // `cls_str × life_per_strength`; the delta here is injected as
        // "target total − baked-in segment", making the Str-derived life total =
        // str_total × the halved coefficient (confirmed against oracle's per-source Life values, wolf-pack: 802→401).
        let no_attributes = self.has_flag("NoAttributeBonuses");
        let life_per_str = if no_attributes
            || self.has_flag("NoStrBonusToLife")
            || self.has_flag("NoStrengthAttributeBonuses")
        {
            0.0
        } else if self.has_flag("HalvesLifeFromStrength") {
            cc.life_per_strength / 2.0
        } else {
            cc.life_per_strength
        };
        let mana_per_int = if no_attributes
            || self.has_flag("NoIntBonusToMana")
            || self.has_flag("NoIntelligenceAttributeBonuses")
        {
            0.0
        } else {
            cc.mana_per_intelligence
        };
        let accuracy_per_dex = if no_attributes
            || self.has_flag("NoDexBonusToAccuracy")
            || self.has_flag("NoDexterityAttributeBonuses")
        {
            0.0
        } else {
            cc.accuracy_per_dexterity
        };
        let mk = |stat: &str, value: f64| {
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::CharacterBase,
                "base.attr_derived",
            ))
            .with_raw_text(format!("{stat} from attributes"));
            Modifier::number(stat, ModType::Base, value).with_origin(origin)
        };
        self.add_modifiers([
            mk(
                "MaximumLife",
                str_total * life_per_str - cls_str * cc.life_per_strength,
            ),
            mk(
                "MaximumMana",
                mana_per_int * int_total - cc.mana_per_intelligence * cls_int,
            ),
            mk(
                "Accuracy",
                accuracy_per_dex * dex_total - cc.accuracy_per_dexterity * cls_dex,
            ),
        ]);
    }

    /// Bridges source-granted flags, reservation-derived LowLife and Surrounded after
    /// all counters are present. Explicit LowLife or Surrounded configuration retains
    /// its existing precedence.
    pub fn bridge_player_conditions(&mut self) {
        for (flag, condition) in [
            ("Condition:CanBeElusive", "CanBeElusive"),
            ("Condition:CanUseBondedModifiers", "CanUseBondedModifiers"),
            ("Condition:ArcaneSurge", "AffectedByArcaneSurge"),
            ("ChaosInoculation", "FullLife"),
            // "Grants Skill: Sorcery Ward" and "Sorcery Ward's Barrier can
            // also take Physical and Chaos Damage from Hits": read by the
            // Barrier's modifiers.
            (
                "Condition:SorceryWard",
                crate::rules::skill_stats::SORCERY_WARD,
            ),
            (
                "Condition:CeremonialAblution",
                crate::rules::skill_stats::CEREMONIAL_ABLUTION,
            ),
            // Wind-ground "counts as boosted" (Berek's Grip/Pass/Respite, The
            // Taming): read by Twister's SkillDamageGainAs tags, like the
            // Twister config checkboxes.
            ("Condition:TwisterFire", "TwisterFire"),
            ("Condition:TwisterLightning", "TwisterLightning"),
            ("Condition:TwisterCold", "TwisterCold"),
        ] {
            if self.has_flag(flag) {
                self.set_condition(condition, true);
            }
        }
        self.bridge_low_pool_conditions();
        self.bridge_surrounded();
    }

    /// Surrounded: as many enemies near the character as the game requires,
    /// [`BASE_REQUIRED_ENEMIES_TO_BE_SURROUNDED`] changed by the stat
    /// `required_enemies_to_be_considered_surrounded_offset` (`SurroundedMinimum`,
    /// "Require 4 fewer enemies to be Surrounded"). The calculation is against one
    /// enemy, taken to be near the character as it is for `EnemyInPresence`, so the
    /// character is Surrounded once a single enemy is enough. There is no distance
    /// model: the Surrounded radius and "increased Surrounded Area of Effect" are
    /// not compared with anything.
    fn bridge_surrounded(&mut self) {
        if self.env.cfg.conditions.contains_key("Surrounded") {
            return;
        }
        let required = BASE_REQUIRED_ENEMIES_TO_BE_SURROUNDED + self.base_sum("SurroundedMinimum");
        if required <= 1.0 {
            self.set_condition("Surrounded", true);
        }
    }

    /// Counts the empty Charm slots, for "for each of your empty Charm slots"
    /// (`EmptyCharms`): the character's Charm slots less the `charms` in
    /// them. Call after the sources are finished and before
    /// [`prepare_player_stats`](Self::prepare_player_stats): Spirit can scale
    /// on the count, as the Shaman's "+40 to Spirit for each of your empty
    /// Charm slots" does.
    pub fn set_empty_charm_slots(&mut self, charms: usize) {
        let slots = crate::calc::env_finalize::charm_slots(&self.env.player.mod_db, &self.env.cfg);
        let empty = (slots - charms as f64).max(0.0);
        self.set_multiplier("EmptyCharms", empty);
        self.set_stat("EmptyCharms", empty);
    }

    /// Resolve "per Minion" and "in your Presence" from the created minions or
    /// explicit configuration. Call after the last minion is created; player and
    /// minion calculations share the snapshot without adding its source again.
    pub fn bridge_minion_presence(&mut self) {
        super::super::minion::bridge_count_multipliers(&self.env.player.mod_db, &mut self.env.cfg);
    }
}

/// Resolve one attack's Ancestral Boost after enabled buffs have been merged.
/// A support is a source, not uptime. Explicit elapsed seconds describe the
/// time since that support last boosted this skill; an explicit snapshot (or
/// the existing Max Hit choice) can instead select an available boosted use.
/// No attack rotation, Cry expiry, or repeat-use frequency is inferred.
pub(in crate::calc) fn bridge_ancestral_boost(env: &mut Env) {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    let sum = |name| db.sum(ModType::Base, cfg, &[ModName::from(name)]);
    let source = db.flag(cfg, "Condition:AncestrallyBoosted");
    let effect = 1.0 + sum("Multiplier:AncestralBoostEffect").max(0.0);
    let (active, targets) = if cfg.ancestral_boost_assume_supported {
        // Preserve the previously exported support snapshot, including its
        // Strike target count. New Slam/Cry effects are game-only below.
        (source, 2.0)
    } else {
        let has_type =
            |name| SkillTypes::from_pob2_name(name).is_some_and(|ty| cfg.skill_types.contains(ty));
        let strike = has_type("MeleeSingleTarget");
        let slam = has_type("Slam");
        let special = has_type("SpecialAncestralBoost");
        // Native callers can select an attack through CalcConfig::attack()
        // while supplying only its Strike/Slam subtype.
        let eligible = (cfg.flags.intersects(ModFlags::ATTACK) || has_type("Attack"))
            && (strike || slam || special)
            && ![
                "Triggered",
                "UsedByProxy",
                "OtherThingUsesSkill",
                "UsedByTotem",
                "SummonsTotem",
                "Trapped",
                "RemoteMined",
            ]
            .into_iter()
            .any(has_type);
        // Use the tracked accessor even when the input is absent: config
        // discovery needs to know this condition can select the snapshot.
        let requested_value = cfg.condition("AncestrallyBoosted");
        let requested = cfg
            .conditions
            .contains_key("AncestrallyBoosted")
            .then_some(requested_value);
        let snapshot = requested.unwrap_or_else(|| db.flag(cfg, "Condition:WarcryMaxHit"));
        let interval = if strike || special {
            sum("AncestralCallCooldown")
        } else if slam {
            sum("FistOfWarCooldown")
        } else {
            0.0
        };
        // The optional input must stay optional: an absent timer does not
        // mean that the support is ready, nor that an attack just consumed it.
        let timed_ready = cfg
            .multiplier_if_set("AncestralBoostElapsedSeconds")
            .map(|seconds| {
                seconds.is_finite() && seconds >= 0.0 && interval > 0.0 && seconds >= interval
            });
        let cry_targets = sum("AncestralCryStrikeTargets");
        let cry_active = (strike || slam) && cry_targets > 0.0;
        (
            eligible
                && requested != Some(false)
                && (cry_active || (source && timed_ready.unwrap_or(snapshot))),
            if cry_active { cry_targets } else { 2.0 },
        )
    };
    env.cfg
        .conditions
        .insert("AncestrallyBoosted".into(), active);
    env.cfg.conditions.insert(
        "AncestralBoostSnapshot".into(),
        active && !env.cfg.ancestral_boost_assume_supported,
    );
    env.cfg.multipliers.insert(
        "AncestralBoostEffect".into(),
        if active { effect } else { 0.0 },
    );
    env.cfg
        .multipliers
        .insert("AncestralBoostStrikeTargets".into(), targets);
}
