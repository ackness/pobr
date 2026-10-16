//! Non-damaging ailment application loop.
//!
//! Mirrors vendor `CalcPerform.lua:3076-3180` "Calculate maximum and apply
//! the strongest non-damaging ailments": folds Chill/Shock source mods into
//! `Current<X>` magnitude and writes it to the enemy db, so shock's damage
//! bonus automatically flows into effective DPS through the existing
//! `DamageTaken` consumption chain (offence.rs's `enemy_damage_multiplier`,
//! gated by `mode_effective`).
//!
//! ## Formulas (CalcPerform.lua line numbers verified against vendor 0.18.0)
//!
//! - **Source aggregation** (:3128-3151): enemy db `<X>Val` (a config enemy
//!   status item) union player db `<X>Base`/`<X>Override`/`<X>Minimum`
//!   (BASE-type mods, equivalent to vendor `modDB:Tabulate("BASE", …)`).
//!   Base/Minimum sources are multiplied by magnitude =
//!   `calcLib.mod(skill, Enemy<X>Magnitude, AilmentMagnitude) ×
//!    calcLib.mod(enemyDB, Self<X>Magnitude, AilmentMagnitude)` (:3144-3146;
//!   Override sources are not multiplied). `override = max(each source's effect, Σ Minimum)` (:3147-3150).
//! - **Maximum** (:3153-3163): `<X>Max` Override takes priority; otherwise
//!   `non_damaging_ailments.json`'s max + Σ `<X>Max BASE` (vendor takes the
//!   max per skill across activeSkillList; pobr has no per-skill modList, so
//!   it aggregates uniformly against the player db — see "Known differences").
//! - **Current** (:3164): `floor(min(max(override, Σ <X>Val), Maximum) × 10^prec) /
//!   10^prec`. `prec` comes from `non_damaging_ailments.json` (0 for both
//!   Chill and Shock), i.e. an integer floor — no new magic number introduced
//!   (see the data channel below).
//! - **Writing the enemy db** (:3078-3124 + :3165-3168):
//!   - Shock → `DamageTaken INC Current {Condition:Shocked}` (:3120);
//!   - Chill → `ActionSpeed INC -Current {Condition:Chilled}` (:3089) plus the
//!     Bonechill branch's `ColdDamageTaken INC Current {Condition:Chilled}` (:3092-3094);
//!   - An Override source sets `Condition:Shocked/Chilled` (:3136-3138);
//!   - After applying, sets `Condition:Already<cond>` (tagged with
//!     `{Condition:<cond>}`, :3168, to prevent a minion applying it twice).
//! - **`Multiplier:ChillEffect/ShockEffect` incremental update** (:3172-3180):
//!   tops up the difference when the existing Σ BASE is less than Current.
//!
//! ## Constant data channel (no new magic numbers allowed)
//!
//! - Chill max = `cfg.constants.game().chill_max_effect` (injected from
//!   `base/game_constants.json`, = `non_damaging_ailments.json` Chill.max = 50);
//! - Shock max = `pobr_data::monster::SHOCK_MAX_EFFECT` (an existing Rust
//!   canonical source, = `non_damaging_ailments.json` Shock.max = 100; this
//!   domain hasn't been folded into the `RuntimeConstants` injection pack
//!   yet — wiring it up belongs to a runtime.rs/RuleSet extension, tracked
//!   under T4 follow-up / the data-driven-conversion backlog);
//! - precision: `non_damaging_ailments.json` has both Chill and Shock at 0 →
//!   integer `floor`, same as above with no injection channel yet (would
//!   need a `RuntimeConstants` extension first if the data changes to non-zero).
//!
//! ## Division of responsibility with `fill_ailments` (calc/ailment.rs, runs after offence)
//!
//! This stage **only consumes `<X>Val/Base/Override/Minimum` mods and
//! config** (same as PoB2 — vendor's version of this section also doesn't
//! depend on this perform's DPS); its output is a debuff mod on the enemy
//! db. `fill_ailments` is the **panel-view** magnitude estimate (cold hit
//! damage / thresholds → `chill_effect`, `shock_effect` output fields), which
//! only writes `OutputTable` and never touches the enemy db — the two never overlap.
//!
//! ## Known differences (declared)
//!
//! - The `ChillCanStack`/`ShockCanStack` stacking branches (:3084-3088 /
//!   :3105-3112) are not implemented (to be added once a build hits it);
//! - `ChillEffectIncDamageTaken` (Asphyxia's Wrath, :3095-3097) is not implemented;
//! - Bonechill's `hasGuaranteedBonechill` (skill data `supportBonechill` +
//!   ChillingArea etc., :1089-1180) is approximated as "a `ChillOverride`
//!   source exists"; `HasBonechill` reads a player db flag (currently no
//!   producer, so this branch is dormant);
//! - Maximum's per-skill `baseSkillModList` max (:3155-3159) degenerates to a
//!   uniform aggregation against the player db (pobr has no per-skill modList channel);
//! - Vendor writes the folded Base/Minimum value back to
//!   `modDB:NewMod(<X>Override, …)` (:3147) — this write-back only serves
//!   PoB's UI/config linkage, which pobr has no consumer for, so it's not written;
//! - Enemy `ActionSpeed` has no consumption point (enemy action speed only
//!   affects the EHP estimate);
//! - Vendor's outer gate's second operand is always truthy because Lua's `0`
//!   is truthy (:3128-3129), so the effective behavior is "write a zero-value
//!   mod plus the Already flag even with no source"; pobr uses an explicit
//!   existence gate instead (the enemy `<X>Val` aggregate is > 0, or the
//!   player has a Base/Override/Minimum mod present), which keeps things
//!   no-op safe (env/db is bit-for-bit unchanged when there's no source).
//! - Condition bridging: vendor's `{Condition:<cond>}` tag looks up
//!   enemyDB.conditions; pobr routes all conditions through `cfg.conditions`
//!   (a single namespace), so when applying, if the enemy db holds a
//!   `Condition:<cond>` flag (or there's an Override source this time), it's
//!   copied back into `cfg.conditions`, so the debuff mod just written is
//!   picked up during offence aggregation.

use pobr_data::monster::SHOCK_MAX_EFFECT;
use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, ModTag, Modifier};

use super::Env;

/// Application spec for a single non-damaging ailment (a row of vendor's
/// `ailments` table, CalcPerform.lua:3078-3125).
struct AilmentSpec {
    /// Vendor's canonical ailment name (the mod name prefix): "Chill" / "Shock".
    name: &'static str,
    /// Enemy condition name (`Condition:<X>`): "Chilled" / "Shocked".
    condition: &'static str,
}

const CHILL: AilmentSpec = AilmentSpec {
    name: "Chill",
    condition: "Chilled",
};
const SHOCK: AilmentSpec = AilmentSpec {
    name: "Shock",
    condition: "Shocked",
};

impl AilmentSpec {
    /// `non_damaging_ailments.json`'s max (see the module doc's "Constant data channel" for the data path).
    fn data_max(&self, cfg: &CalcConfig) -> f64 {
        match self.name {
            "Chill" => cfg.constants.game().chill_max_effect,
            _ => SHOCK_MAX_EFFECT,
        }
    }
}

/// The terms of `Maximum<X>`: an override replaces the game maximum plus the
/// sum of the `<X>Max` BASE modifiers. Shared by the application stage and the
/// `MaximumChill` / `MaximumShock` Calcs views.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MaximumTerms {
    pub override_value: Option<f64>,
    pub game_maximum: f64,
    pub bonus: f64,
}

impl MaximumTerms {
    pub fn value(self) -> f64 {
        self.override_value
            .unwrap_or(self.game_maximum + self.bonus)
    }
}

/// `Maximum<X>` terms for `"Chill"`; any other name reads Shock.
pub fn maximum_terms(player: &ModDb, cfg: &CalcConfig, name: &str) -> MaximumTerms {
    let spec = if name == "Chill" { &CHILL } else { &SHOCK };
    let max_name = ModName::from(format!("{}Max", spec.name));
    MaximumTerms {
        override_value: player.override_(cfg, max_name.as_str()),
        game_maximum: spec.data_max(cfg),
        bonus: player.sum(ModType::Base, cfg, std::slice::from_ref(&max_name)),
    }
}

/// One non-damaging ailment's limit and applied magnitude. `current` is
/// `floor(min(max(source, enemy_value), maximum))`, or 0 when the ailment is
/// not applied (no source, or already applied by another actor); `source` and
/// `enemy_value` are the player-side strongest source and the enemy `<X>Val`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AppliedAilment {
    /// The terms of `maximum`.
    pub terms: MaximumTerms,
    pub maximum: f64,
    /// False when the stage returned early: no source, or already applied.
    pub applied: bool,
    pub source: f64,
    pub enemy_value: f64,
    pub current: f64,
}

/// The Chill and Shock results of the enemy-ailment stage, retained for the
/// `Maximum*` and `Current*` Calcs views.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AppliedAilments {
    pub chill: AppliedAilment,
    pub shock: AppliedAilment,
}

/// env_finalize stage 7 entry point: applies Chill/Shock, then does the
/// `Multiplier:ChillEffect/ShockEffect` incremental update. Bit-for-bit no-op
/// on the enemy and conditions when there's no source mod at all.
pub fn apply_nondamaging_ailments(env: &mut Env) {
    let chill = apply_ailment(env, &CHILL);
    let shock = apply_ailment(env, &SHOCK);
    env.applied_ailments = AppliedAilments { chill, shock };
    // vendor :3173-3180: tops up the difference when output.Current<X> is
    // higher than the existing multiplier (an incremental update, consumed
    // by mods like "per ChillEffect/ShockEffect"). Always a no-op when not applied (Current=0).
    update_effect_multiplier(env, &CHILL, chill.current);
    update_effect_multiplier(env, &SHOCK, shock.current);
}

/// The kind of a player db source mod (vendor Tabulate's three names).
#[derive(Clone, Copy)]
enum SourceKind3 {
    Base,
    Override,
    Minimum,
}

/// Applies a single ailment, returning its limit and `Current<X>` (0.0 when not applied).
fn apply_ailment(env: &mut Env, spec: &AilmentSpec) -> AppliedAilment {
    // cfg snapshot: keeps the read stage's semantics consistent; cfg.conditions
    // is only copied back at the end of the write stage.
    let cfg = env.cfg.clone();
    let player = &env.player.mod_db;
    let enemy = &env.enemy.mod_db;

    let val_name = ModName::from(format!("{}Val", spec.name));
    let enemy_val = enemy.sum(ModType::Base, &cfg, std::slice::from_ref(&val_name));

    // vendor Tabulate("BASE", nil, <X>Base, <X>Override, <X>Minimum) (:3131).
    let sources = collect_player_sources(player, &cfg, spec);
    let terms = maximum_terms(player, &cfg, spec.name);
    let maximum = terms.value();
    let not_applied = AppliedAilment {
        terms,
        maximum,
        applied: false,
        source: 0.0,
        enemy_value: enemy_val,
        current: 0.0,
    };

    // Existence gate (the semantic equivalent of vendor :3128-3129, see the
    // module doc's last two "Known differences" bullets).
    if enemy_val <= 0.0 && sources.is_empty() {
        return not_applied;
    }
    // `Condition:Already<cond>` prevents duplicate application (:3130).
    let already_name = ModName::from(format!("Condition:Already{}", spec.condition));
    if enemy.flag(&cfg, already_name.as_str()) {
        return not_applied;
    }

    // Magnitude (only Base/Minimum sources are multiplied, :3144-3146): skill
    // side × enemy side, calcLib.mod = (1 + Σinc/100) × Πmore.
    let magnitude = ailment_magnitude(player, enemy, &cfg, spec);

    let mut strongest = 0.0_f64;
    let mut minimum = 0.0_f64;
    let mut override_seen = false;
    for (kind, raw) in &sources {
        let effect = match kind {
            SourceKind3::Override => {
                override_seen = true;
                *raw
            }
            SourceKind3::Base => raw * magnitude,
            SourceKind3::Minimum => {
                let scaled = raw * magnitude;
                minimum += scaled;
                scaled
            }
        };
        strongest = strongest.max(effect);
    }
    // `override = m_max(m_max(override, effect), minimum)`, applying max
    // repeatedly is order-independent here (minimum is monotonically
    // non-decreasing, so the final value already covers the full total).
    strongest = strongest.max(minimum);

    // Current<X> = floor(min(max(override, Σ Val), Maximum) × 10^prec)/10^prec (:3164);
    // prec=0 (non_damaging_ailments.json Chill/Shock) → integer floor.
    let current = strongest.max(enemy_val).min(maximum).floor();

    // Condition-bridging decision must read before writing (override source / existing enemy db condition flag).
    let cond_flag_name = ModName::from(format!("Condition:{}", spec.condition));
    let condition_active = override_seen || enemy.flag(&cfg, cond_flag_name.as_str());

    // Bonechill branch decision (Chill-only, :3092-3094; see the module doc's "Known differences" for the approximation).
    let bonechill = spec.name == "Chill"
        && player.flag(&cfg, "HasBonechill")
        && (enemy_val > 0.0 || override_seen);

    // Write stage (all writes to the enemy db / cfg.conditions are applied together after reads are done)
    let origin_id = format!("ailment.{}", spec.name.to_lowercase());
    let origin = || ModifierSource::new(SourceId::new(SourceKind::Derived, origin_id.clone()));
    let cond_tag = || ModTag::condition(spec.condition, false);

    let mut new_mods: Vec<Modifier> = Vec::new();
    // An Override source sets enemy `Condition:<cond>` (:3136-3138; merged into a single flag).
    if override_seen {
        new_mods.push(
            Modifier::flag(cond_flag_name)
                .with_source(spec.name)
                .with_origin(origin()),
        );
    }
    match spec.name {
        // Shock → DamageTaken INC Current {Condition:Shocked} (:3120).
        "Shock" => new_mods.push(
            Modifier::number(ModName::from("DamageTaken"), ModType::Inc, current)
                .with_tag(cond_tag())
                .with_source(spec.name)
                .with_origin(origin()),
        ),
        // Chill → ActionSpeed INC -Current {Condition:Chilled} (:3089) + Bonechill.
        _ => {
            new_mods.push(
                Modifier::number(ModName::from("ActionSpeed"), ModType::Inc, -current)
                    .with_tag(cond_tag())
                    .with_source(spec.name)
                    .with_origin(origin()),
            );
            if bonechill {
                new_mods.push(
                    Modifier::number(ModName::from("ColdDamageTaken"), ModType::Inc, current)
                        .with_tag(cond_tag())
                        .with_source("Bonechill")
                        .with_origin(origin()),
                );
            }
        }
    }
    // `Condition:Already<cond>` {Condition:<cond>} (:3168, prevents a minion applying it twice).
    new_mods.push(
        Modifier::flag(already_name)
            .with_tag(cond_tag())
            .with_source(spec.name)
            .with_origin(origin()),
    );
    env.enemy.mod_db.add_list(new_mods);

    // Condition bridging (pobr's single condition namespace, see the module
    // doc): makes the {Condition:<cond>}-tagged debuff mods above match during offence/defence aggregation.
    if condition_active {
        env.cfg.conditions.insert(spec.condition.to_string(), true);
    }

    AppliedAilment {
        terms,
        maximum,
        applied: true,
        source: strongest,
        enemy_value: enemy_val,
        current,
    }
}

/// Collects the player db's `<X>Base/<X>Override/<X>Minimum` BASE-type source
/// mods (equivalent to vendor `modDB:Tabulate("BASE", …)`: evaluates each with `matches(cfg)` + the Multiplier tag).
fn collect_player_sources(
    player: &ModDb,
    cfg: &CalcConfig,
    spec: &AilmentSpec,
) -> Vec<(SourceKind3, f64)> {
    let base_name = ModName::from(format!("{}Base", spec.name));
    let override_name = ModName::from(format!("{}Override", spec.name));
    let minimum_name = ModName::from(format!("{}Minimum", spec.name));
    [
        (SourceKind3::Base, &base_name),
        (SourceKind3::Override, &override_name),
        (SourceKind3::Minimum, &minimum_name),
    ]
    .into_iter()
    .flat_map(|(kind, name)| {
        player
            .named(name.as_str())
            .filter(|modifier| modifier.mod_type == ModType::Base && modifier.matches(cfg))
            .filter_map(move |modifier| Some((kind, modifier.effective_number(cfg)?)))
    })
    .collect()
}

/// Magnitude multiplier (:3144-3146): skill side
/// `Enemy<X>Magnitude`/`AilmentMagnitude` (player db) × enemy side
/// `Self<X>Magnitude`/`AilmentMagnitude` (enemy db), each computed as
/// `(1 + Σinc/100) × Πmore` (calcLib.mod).
fn ailment_magnitude(player: &ModDb, enemy: &ModDb, cfg: &CalcConfig, spec: &AilmentSpec) -> f64 {
    let skill_names = [
        ModName::from(format!("Enemy{}Magnitude", spec.name)),
        ModName::from("AilmentMagnitude"),
    ];
    let enemy_names = [
        ModName::from(format!("Self{}Magnitude", spec.name)),
        ModName::from("AilmentMagnitude"),
    ];
    let skill_side = (1.0 + player.sum(ModType::Inc, cfg, &skill_names) / 100.0)
        * player.more(cfg, &skill_names);
    let enemy_side =
        (1.0 + enemy.sum(ModType::Inc, cfg, &enemy_names) / 100.0) * enemy.more(cfg, &enemy_names);
    skill_side * enemy_side
}

/// `Multiplier:<X>Effect` incremental update (:3173-3180): tops up the difference when the existing Σ BASE < Current.
fn update_effect_multiplier(env: &mut Env, spec: &AilmentSpec, current: f64) {
    let name = ModName::from(format!("Multiplier:{}Effect", spec.name));
    let existing = env
        .enemy
        .mod_db
        .sum(ModType::Base, &env.cfg, std::slice::from_ref(&name));
    if existing < current {
        env.enemy.mod_db.add_mod(
            Modifier::number(name, ModType::Base, current - existing)
                .with_source(spec.name)
                .with_origin(ModifierSource::new(SourceId::new(
                    SourceKind::Derived,
                    format!("ailment.{}", spec.name.to_lowercase()),
                ))),
        );
    }
}
