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

    /// Counts the minions in the character's Presence, for "for each Minion
    /// in your Presence" (`MinionPresenceCount`): the minions the calculation
    /// created ([`add_minion_from_def`](Self::add_minion_from_def)), or the
    /// configured "# of Minions in your Presence", a combat input read as in
    /// combat when the calculation is. Call once, after the last minion is
    /// created: the count is read from the context, never again from the
    /// modifiers it came from. No minions leave it unset, so a line that
    /// counts them still shows as unprovided.
    pub fn bridge_minion_presence(&mut self) {
        let mut cfg = self.env.cfg.clone();
        let in_combat = cfg.mode_combat;
        cfg.conditions.entry("Combat".into()).or_insert(in_combat);
        let db = &self.env.player.mod_db;
        let count = db.get_multiplier("MinionPresenceCount", &cfg);
        if count != 0.0 {
            self.set_multiplier("MinionPresenceCount", count);
        }
    }
}
