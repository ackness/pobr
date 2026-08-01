//! Placeholder template instantiation — turns tag templates and flag-name arrays that carry
//! `$n` / `:cap` placeholders in the rule tables into pobr [`ModTag`] / [`ModFlags`] /
//! [`KeywordFlags`].
//!
//! The placeholder dialect is **shared** with `rules::special_mod` (`$n` capture, `:cap`
//! capitalize-and-concat, `negate/div/mult/base` operators); the numeric operator chain reuses
//! the single evaluator `rules::value_expr` (no second dialect allowed). This module only adds
//! `:cap` string concatenation expansion (a bounded extension — the payoff of ~139 closed gaps
//! clears the bar of a 20-entry gate).

use pobr_data::catalog::parser_rules::TagTemplate;
use pobr_data::catalog::stat_map::StatMapValue;

use crate::{ActorRef, ModTag};
use pobr_data::modifier::{KeywordFlags, ModFlags};
use pobr_data::prelude::{DamageType, SkillTypes};

/// Instantiate a placeholder string value against the captures (`$n` substitutes directly,
/// `$n:cap` capitalizes it, `+` between segments concatenates literals; non-placeholder
/// segments pass through unchanged). Vendor's `firstToUpper(cap) .. "Effect"` becomes the
/// template `"$2:cap+Effect"`.
pub fn interpolate(template: &str, captures: &[String]) -> String {
    // Template shape: `segment1+segment2+...`, where each segment is a literal or `$n` / `$n:cap`.
    template
        .split('+')
        .map(|seg| interpolate_segment(seg, captures))
        .collect()
}

fn interpolate_segment(seg: &str, captures: &[String]) -> String {
    if let Some(rest) = seg.strip_prefix('$') {
        // `$n` or `$n:cap`
        let (idx_str, cap_op) = match rest.split_once(':') {
            Some((n, op)) => (n, Some(op)),
            None => (rest, None),
        };
        if let Ok(idx) = idx_str.parse::<usize>() {
            let raw = captures
                .get(idx.saturating_sub(1))
                .cloned()
                .unwrap_or_default();
            return match cap_op {
                Some("cap") => first_to_upper(&raw),
                _ => raw,
            };
        }
        // Not a number → treat as a literal (keep the `$` prefix).
        seg.to_string()
    } else {
        seg.to_string()
    }
}

/// Vendor's Lua `firstToUpper`: capitalizes the first letter, leaves the rest unchanged.
fn first_to_upper(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Resolve a template field value as a string (with placeholder interpolation). `$n` / `$n:cap`
/// / literal.
fn field_text(value: &StatMapValue, captures: &[String]) -> Option<String> {
    match value {
        StatMapValue::Text(s) => Some(interpolate(s, captures)),
        StatMapValue::Number(n) => Some(n.to_string()),
        StatMapValue::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Resolve a template field value as a number (either a `$n` capture or a literal).
fn field_number(value: &StatMapValue, captures: &[String]) -> Option<f64> {
    match value {
        StatMapValue::Number(n) => Some(*n),
        StatMapValue::Text(s) => {
            if let Some(rest) = s.strip_prefix('$') {
                let n: usize = rest.split(':').next()?.parse().ok()?;
                captures.get(n.saturating_sub(1))?.parse().ok()
            } else {
                s.parse().ok()
            }
        }
        _ => None,
    }
}

fn field_bool(value: &StatMapValue) -> Option<bool> {
    match value {
        StatMapValue::Bool(b) => Some(*b),
        _ => None,
    }
}

/// Evaluate a numeric field value that carries a **capture operator** (`$n:mult(N)` → ×N,
/// `$n:div(N)` → ÷N, `$n:base(N)` → +N; falls back to [`field_number`] when there's no
/// operator). The template/special dialect shares the same `:cap` mechanism as
/// `interpolate_segment`, just applied to numbers instead.
///
/// Only used by consumers like MultiplierThreshold's `threshold = "$1:mult(10)"` (metres →
/// internal units) — deliberately **not** folded into [`field_number`], to avoid changing the
/// semantics of existing fields like Multiplier's `limit="$1:base(6)"` as a side effect; those
/// are separate latent bugs, out of scope for this change.
fn field_number_capop(value: &StatMapValue, captures: &[String]) -> Option<f64> {
    let StatMapValue::Text(s) = value else {
        return field_number(value, captures);
    };
    let Some(rest) = s.strip_prefix('$') else {
        return field_number(value, captures);
    };
    let (idx_str, op) = match rest.split_once(':') {
        Some((i, o)) => (i, Some(o)),
        None => (rest, None),
    };
    let idx: usize = idx_str.parse().ok()?;
    let mut v: f64 = captures.get(idx.saturating_sub(1))?.parse().ok()?;
    if let Some(op) = op {
        let (name, arg) = op.split_once('(')?;
        let arg: f64 = arg.strip_suffix(')')?.parse().ok()?;
        v = match name {
            "mult" => v * arg,
            "div" if arg != 0.0 => v / arg,
            "div" => v,
            "base" => v + arg,
            _ => return None,
        };
    }
    Some(v)
}

/// Instantiate a [`TagTemplate`] into a pobr [`ModTag`].
///
/// **Mappable set** (matches special_mod::compile_tag's coverage, extended with `$n` fields for
/// Multiplier/PerStat/ActorCondition):
/// - `Multiplier` (var/div/limit/limitTotal/actor);
/// - `Condition` / `ActorCondition` (var/neg/actor);
/// - `SkillType` (skill_type name);
/// - `DamageType` (damageType name);
/// - `PerStat` / `PercentStat` (stat/div/limit).
///
/// **Not mappable** (no pobr landing point, returns `None`; the line can still produce other
/// mods, but the caller uses this to treat the whole line as a conservative mismatch — see
/// engine): `SkillName` / `GlobalEffect` / `ItemCondition` / `MultiplierThreshold` /
/// `StatThreshold` etc.
pub fn compile_tag(tag: &TagTemplate, captures: &[String]) -> Option<ModTag> {
    let f = &tag.fields;
    match tag.tag_type.as_str() {
        "Multiplier" => {
            let var = f.get("var").and_then(|v| field_text(v, captures))?;
            let var = normalize_perstat_slot_suffix(&var);
            let var = normalize_attribute_var(&var);
            let div = f
                .get("div")
                .and_then(|v| field_number(v, captures))
                .unwrap_or(1.0);
            let limit = f.get("limit").and_then(|v| field_number(v, captures));
            let actor = f.get("actor").and_then(|v| field_text(v, captures));
            // Dynamic-limit channel (vendor `tag.limitVar`/`limitActor`, e.g. "for every
            // different grenade fired" → `Multiplier{var=DifferentGrenadeFired,
            // limitVar=GrenadeTypes}`). Previously hardcoded to `None`, so the JSON's limitVar
            // was silently dropped and the multiplier wasn't capped by the number of equipped
            // types.
            let limit_var = f.get("limitVar").and_then(|v| field_text(v, captures));
            let limit_actor = f.get("limitActor").and_then(|v| field_text(v, captures));
            Some(ModTag::Multiplier {
                var,
                div,
                limit,
                actor: parse_actor(actor.as_deref()),
                limit_var,
                limit_actor: parse_actor(limit_actor.as_deref()),
                invert: false,
                limit_total: f.get("limitTotal").and_then(field_bool).unwrap_or(false),
            })
        }
        "Condition" => {
            let neg = f.get("neg").and_then(field_bool).unwrap_or(false);
            // Vendor's Condition can carry either `var` (a single condition) or `varList` (OR
            // semantics: true if any one holds, ModStore.lua:596-607). `var` takes priority; a
            // single-element `varList` degenerates to a single Condition (e.g. `while holding a
            // (%w+)` with gear=shield → `UsingShield`, matching legacy verbatim), a multi-element
            // one → `ConditionAnyOf` (OR).
            if let Some(var) = f.get("var").and_then(|v| field_text(v, captures)) {
                return Some(ModTag::condition(var, neg));
            }
            let Some(StatMapValue::List(items)) = f.get("varList") else {
                return None;
            };
            let vars: Vec<String> = items
                .iter()
                .map(|v| field_text(v, captures))
                .collect::<Option<_>>()?;
            match vars.len() {
                0 => None,
                1 => Some(ModTag::condition(vars.into_iter().next().unwrap(), neg)),
                _ => Some(ModTag::ConditionAnyOf { vars, negated: neg }),
            }
        }
        "ActorCondition" => {
            // .3 normalization: vendor's `ActorCondition{actor=enemy,var=X}` becomes PoBR's
            // flat condition `Condition{var=Enemy<X>}` (actor=None), matching legacy's and the
            // orchestrator's cfg key space (the orchestrator sets `Enemy<X>` true from the
            // build config's `conditionEnemy<X>`). Exceptions handled by
            // [`normalize_enemy_cond_var`]: don't add the prefix when var already carries
            // `Enemy` (EnemyInPresence) or is an enemy **rarity** name
            // (Rare/Unique/RareOrUnique/Normal/Magic, which legacy uses bare) — avoids a double
            // prefix / diverging from legacy.
            //
            // Fix (fork-a): unconditionally using the bare name used to make `against ignited
            // enemies` produce `Condition{Ignited}` (checking the player's own Ignited state,
            // always false) instead of legacy's `EnemyIgnited` (checking the enemy's ailment,
            // set true by the orchestrator) — every player-side "against <ailment> enemies"
            // damage bonus was completely inert.
            //
            // `varList` (OR semantics, ModStore.lua:631-640): each var is normalized the same
            // way and collected into `ConditionAnyOf` (e.g. "against enemies affected by
            // ailments" → true if any of the nine ailments' Enemy<X> holds; "while a rare or
            // unique enemy is in your presence" → {EnemyNearbyRareOrUniqueEnemy, RareOrUnique},
            // the latter set true by the orchestrator on boss-tier configs).
            let neg = f.get("neg").and_then(field_bool).unwrap_or(false);
            if let Some(var) = f.get("var").and_then(|v| field_text(v, captures)) {
                return Some(ModTag::condition(normalize_enemy_cond_var(&var), neg));
            }
            let Some(StatMapValue::List(items)) = f.get("varList") else {
                return None;
            };
            let vars: Vec<String> = items
                .iter()
                .map(|v| field_text(v, captures).map(|t| normalize_enemy_cond_var(&t)))
                .collect::<Option<_>>()?;
            match vars.len() {
                0 => None,
                1 => Some(ModTag::condition(vars.into_iter().next().unwrap(), neg)),
                _ => Some(ModTag::ConditionAnyOf { vars, negated: neg }),
            }
        }
        "SkillType" => {
            let name = f.get("skill_type").and_then(|v| field_text(v, captures))?;
            let bare = name.strip_prefix("SkillType:").unwrap_or(&name);
            // Full enum table (data-driven, A1): the rule JSON's names come from a reverse
            // lookup against the vendor enum, so a miss means corrupted data — debug builds
            // assert loudly (surfaced for A2), release builds conservatively drop the tag.
            let st = SkillTypes::from_pob2_name(bare);
            debug_assert!(st.is_some(), "unknown SkillType name: {bare}");
            st.map(ModTag::SkillTypes)
        }
        "SkillName" => {
            // Named-skill qualifier (vendor's single `skillName` / list `skillNameList`,
            // matched by lowercase equality; the [`ModTag::SkillName`] semantics are already
            // implemented in `matches`, matching special_mod DSL V2's coverage). First data
            // consumer: vendor's skillNameList catches text that was stripped early during
            // extraction (e.g. `increased Grenade Damage` → Damage + SkillName{"grenade"} — the
            // skill name is the literal "Grenade", which no real skill is named, so it never
            // matches = vendor's own zero-effect outcome, matched verbatim against the ModCache
            // golden). `includeTransfigured` is ignored (PoE2 has no skill variants).
            let names: Vec<String> = if let Some(v) = f.get("skillName") {
                vec![field_text(v, captures)?.to_ascii_lowercase()]
            } else if let Some(StatMapValue::List(items)) = f.get("skillNameList") {
                items
                    .iter()
                    .map(|v| field_text(v, captures).map(|s| s.to_ascii_lowercase()))
                    .collect::<Option<_>>()?
            } else {
                return None;
            };
            (!names.is_empty()).then_some(ModTag::SkillName { names })
        }
        "DamageType" => {
            let name = f.get("damageType").and_then(|v| field_text(v, captures))?;
            damage_type_bit(&name).map(ModTag::DamageType)
        }
        "SlotName" => {
            // Vendor slot names (`Body Armour`/`Weapon 2`/`Helmet`…) → legacy's stable slot ID
            // (lowercase + spaces stripped, matching EquipmentSlot::id). slotNameList (multiple
            // slots) is conservatively skipped this batch (not in the C1 diff set).
            let name = f.get("slotName").and_then(|v| field_text(v, captures))?;
            Some(ModTag::SlotName(slot_name_to_id(&name)))
        }
        "PerStat" | "PercentStat" => {
            // .3 normalization (C2): vendor's `PerStat{stat,div,limit}` maps field-for-field to
            // PoBR's `Multiplier{var=stat,div,limit}` (the calc side's effective_number only
            // recognizes Multiplier; legacy also always produces Multiplier). Normalize to
            // Multiplier.
            //
            // Vendor's `statList = {A, B, …}` (e.g. "per 75 armour and evasion on equipped
            // shield" → {ArmourOnWeapon 2, EvasionOnWeapon 2}, ModParser.lua:1631): mult =
            // floor(Σstats/div) — the stats are summed first, then divided. This is normalized
            // into a `|`-joined compound var; the consumer side (effective_number's Multiplier
            // branch) splits on `|` and sums, matching vendor ModStore.lua:445-452's semantics.
            let stat = if let Some(StatMapValue::List(items)) = f.get("statList") {
                let parts: Vec<String> = items
                    .iter()
                    .map(|v| field_text(v, captures))
                    .collect::<Option<_>>()?;
                if parts.is_empty() {
                    return None;
                }
                parts
                    .into_iter()
                    .map(|p| normalize_attribute_var(&normalize_perstat_slot_suffix(&p)))
                    .collect::<Vec<_>>()
                    .join("|")
            } else {
                let stat = f
                    .get("stat")
                    .or_else(|| f.get("var"))
                    .and_then(|v| field_text(v, captures))?;
                normalize_attribute_var(&normalize_perstat_slot_suffix(&stat))
            };
            let div = f
                .get("div")
                .and_then(|v| field_number(v, captures))
                .unwrap_or(1.0);
            let limit = f.get("limit").and_then(|v| field_number(v, captures));
            Some(ModTag::Multiplier {
                var: stat,
                div,
                limit,
                actor: None,
                limit_var: None,
                limit_actor: None,
                invert: false,
                limit_total: f.get("limitTotal").and_then(field_bool).unwrap_or(false),
            })
        }
        "MultiplierThreshold" => {
            // Vendor's `MultiplierThreshold{actor=enemy, var=<X>Stacks, threshold=1, upper}`
            // expresses a binary "enemy ailment present/absent" condition (e.g. "on targets
            // that are not Poisoned" → enemy poison stacks <1), mapped to PoBR's flat
            // `Condition{Enemy<X past>, negated=upper}`, mirroring how legacy handles that
            // phrase (`EnemyPoisoned` negated). Only **ailment-stack var + literal
            // threshold=1** is mapped; scaling forms (a `$n`-captured threshold, i.e. "per X up
            // to N") and non-ailment vars have no binary pobr landing point and still return
            // None (conservative mismatch, unchanged from before the fix).
            let var = f.get("var").and_then(|v| field_text(v, captures))?;
            let upper = f.get("upper").and_then(field_bool).unwrap_or(false);
            if let Some(cond) = ailment_stacks_condition(&var) {
                // Ailment stacks are only binarized for a **literal threshold=1**; anything
                // else (a captured scaling `$n`) has no landing point.
                return matches!(f.get("threshold"), Some(StatMapValue::Number(n)) if *n == 1.0)
                    .then(|| ModTag::condition(cond, upper));
            }
            // Distance threshold (vendor ModStore.lua:559-573): "against enemies within/further
            // than N metres" → `var=enemyDistance`, `threshold=N×10` (`"$1:mult(10)"` converts
            // metres to internal units, requiring field_number_capop to apply the `:mult(10)`
            // operator) → [`ModTag::MultiplierThreshold`].
            //
            // Directional pass-through for non-distance/non-ailment vars (A2 real gap #12,
            // "while you have an ally in your presence" → NearbyAlly≥1): **producing a tag for
            // a lower-bound threshold (upper=false) is safe to under-apply** — evaluation reads
            // a missing cfg.multiplier key as 0, so 0 < threshold fails and the mod stays
            // inactive; once the orchestrator later feeds that multiplier, the entry
            // auto-activates. **Upper-bound thresholds (upper=true) still conservatively return
            // None**: a missing key reads as 0, and 0 ≤ threshold is always true, which would
            // over-apply.
            if var != "enemyDistance" && upper {
                return None;
            }
            let threshold = f
                .get("threshold")
                .and_then(|v| field_number_capop(v, captures))?;
            Some(ModTag::MultiplierThreshold {
                var,
                threshold,
                upper,
            })
        }
        // Unmapped tag shape: conservatively skip (return None; engine decides how to treat
        // the whole line based on this).
        _ => None,
    }
}

/// `<Ailment>Stacks` threshold var → the enemy-ailment-present condition var
/// (`PoisonStacks`→`EnemyPoisoned`…), mirroring legacy's `Enemy<X>` condition landing point for
/// "on targets that are [not] <ailment>ed" (set true by the orchestrator from the build
/// config's `conditionEnemy<X>`). Covers only damage/common ailments; anything else returns
/// None (conservative mismatch).
fn ailment_stacks_condition(var: &str) -> Option<String> {
    let past = match var.strip_suffix("Stacks")? {
        "Poison" => "Poisoned",
        "Bleed" => "Bleeding",
        "Ignite" => "Ignited",
        "Shock" => "Shocked",
        "Chill" => "Chilled",
        "Freeze" => "Frozen",
        "Scorch" => "Scorched",
        "Sap" => "Sapped",
        "Brittle" => "Brittle",
        _ => return None,
    };
    Some(format!("Enemy{past}"))
}

/// Normalizes the `On<Slot>` slot-name suffix on a PerStat/Multiplier slot-scaling var into a
/// slot ID (lowercased, spaces stripped, via `slot_name_to_id`), matching the
/// `<Stat>On<slot.id()>` key format the orchestrator's `per_slot_defence_multipliers` builds.
/// Vendor data uses inconsistent slot-name casing (`OnBoots`/`OnBody Armour`/`Onhelmet` all
/// occur); without normalization, `+N to Armour per M ES on Equipped Boots` would produce
/// `EnergyShieldOnBoots`, which wouldn't match the consumer's `EnergyShieldOnboots`, so the
/// multiplier reads as 0 and the slot's defence base drops to zero (the root cause of fork-a's
/// observed Armour→0).
/// Only normalizes recognized equipment-slot suffixes; non-single-slot suffixes like
/// `OnAllArmourItems` pass through unchanged (handled by a separate consumer channel). Idempotent
/// on an already-lowercase `Onhelmet`.
pub(crate) fn normalize_perstat_slot_suffix(var: &str) -> String {
    let Some(idx) = var.rfind("On") else {
        return var.to_string();
    };
    let (head, slot) = (&var[..idx], &var[idx + 2..]);
    let is_known_slot = matches!(
        slot.to_ascii_lowercase().as_str(),
        "boots"
            | "helmet"
            | "gloves"
            | "body armour"
            | "weapon"
            | "weapon 1"
            | "weapon 2"
            | "shield"
            | "focus"
            | "quiver"
            | "off hand"
            | "main hand"
            | "ring"
            | "amulet"
            | "belt"
    );
    if is_known_slot {
        format!("{head}On{}", slot_name_to_id(slot))
    } else {
        var.to_string()
    }
}

/// Vendor's short attribute names (`Str`/`Dex`/`Int`) → PoBR's full names (`Strength`/
/// `Dexterity`/`Intelligence`). PerStat/Multiplier's attribute-scaling var must use the full
/// name — both the orchestrator's `set_multiplier("Strength"/"Dexterity"/"Intelligence", …)` and
/// legacy use full names, so a short-name var would miss the multiplier lookup and silently
/// contribute 0 (per-attribute scaling would be inert).
/// A closed set, attributes only; every other var (`AxeItem`/`SummonedMinion`/`Rage`/
/// `PowerCharge`/`Spirit`…) is returned unchanged (matches `stat_map_engine.rs` /
/// `vendor_name_aliases.json`'s coverage).
pub(crate) fn normalize_attribute_var(var: &str) -> String {
    match var {
        "Str" => "Strength",
        "Dex" => "Dexterity",
        "Int" => "Intelligence",
        other => other,
    }
    .to_string()
}

/// Normalizes the var of vendor's `ActorCondition{actor=enemy}` into a PoBR flat condition var
/// (matching legacy's and the orchestrator's cfg key space). Adds an `Enemy` prefix by default
/// (`Ignited`→`EnemyIgnited`, matching legacy's suffix table + the orchestrator's
/// `conditionEnemy<X>`→`Enemy<X>`).
/// Two exceptions pass through unchanged: a var already carrying the `Enemy` prefix
/// (`EnemyInPresence`, avoiding a double prefix); and enemy rarity names
/// (`Rare`/`Unique`/`RareOrUnique`/`Normal`/`Magic`), which legacy uses bare.
fn normalize_enemy_cond_var(var: &str) -> String {
    const BARE: &[&str] = &["Rare", "Unique", "RareOrUnique", "Normal", "Magic"];
    if var.starts_with("Enemy") || BARE.contains(&var) {
        var.to_string()
    } else {
        format!("Enemy{var}")
    }
}

/// Whether this is a tag type this module "knows but has no pobr landing point for" (as
/// distinct from a genuinely unknown type; lets engine decide whether to still emit partial
/// support). Currently conservative: any `compile_tag` returning None counts as a mismatch.
pub fn is_mappable_tag_type(tag_type: &str) -> bool {
    matches!(
        tag_type,
        "Multiplier"
            | "Condition"
            | "ActorCondition"
            | "SkillType"
            | "SkillName"
            | "DamageType"
            | "PerStat"
            | "PercentStat"
            | "SlotName"
    )
}

fn parse_actor(name: Option<&str>) -> Option<ActorRef> {
    match name {
        Some("player") => Some(ActorRef::Player),
        Some("parent") => Some(ActorRef::Parent),
        Some("minion") => Some(ActorRef::Minion),
        _ => None,
    }
}

/// ModFlag name → bit (matches special_mod::flag_bit's coverage). Unknown name → `None`.
pub fn flag_bit(name: &str) -> Option<ModFlags> {
    Some(match name {
        "Attack" => ModFlags::ATTACK,
        "Spell" => ModFlags::SPELL,
        "Hit" => ModFlags::HIT,
        "Dot" => ModFlags::DOT,
        "Cast" => ModFlags::CAST,
        "Melee" => ModFlags::MELEE,
        "Area" => ModFlags::AREA,
        "Projectile" => ModFlags::PROJECTILE,
        "Ailment" => ModFlags::AILMENT,
        "Weapon" => ModFlags::WEAPON,
        // Weapon **category** bits (vendor `ModFlag.Weapon1H`/`Weapon2H`/`WeaponMelee`/
        // `WeaponRanged`, Data/Global.lua). `weapon_type_bit` only recognizes weapon **type**
        // names (Axe/Bow/Staff…), not these category names — without this, phrases like "with
        // one handed (melee) weapons" would silently lose their Weapon1H/WeaponMelee bit
        // (leaving only Hit).
        "Weapon1H" => ModFlags::WEAPON_1H,
        "Weapon2H" => ModFlags::WEAPON_2H,
        "WeaponMelee" => ModFlags::WEAPON_MELEE,
        "WeaponRanged" => ModFlags::WEAPON_RANGED,
        "Thorns" => ModFlags::THORNS,
        other => return ModFlags::weapon_type_bit(other),
    })
}

/// Flag-name array → bit set.
pub fn compile_flags(names: &[String]) -> ModFlags {
    names
        .iter()
        .fold(ModFlags::NONE, |acc, n| match flag_bit(n) {
            Some(bit) => acc | bit,
            None => acc,
        })
}

/// KeywordFlag name → bit (matches special_mod::keyword_bit's coverage).
pub fn keyword_bit(name: &str) -> Option<KeywordFlags> {
    Some(match name {
        "Aura" => KeywordFlags::AURA,
        "Curse" => KeywordFlags::CURSE,
        "Totem" => KeywordFlags::TOTEM,
        "Attack" => KeywordFlags::ATTACK,
        "Spell" => KeywordFlags::SPELL,
        "Hit" => KeywordFlags::HIT,
        "Ailment" => KeywordFlags::AILMENT,
        "Poison" => KeywordFlags::POISON,
        "Bleed" => KeywordFlags::BLEED,
        "Ignite" => KeywordFlags::IGNITE,
        _ => return None,
    })
}

/// Keyword-name array → bit set.
pub fn compile_keyword_flags(names: &[String]) -> KeywordFlags {
    names
        .iter()
        .fold(KeywordFlags::NONE, |acc, n| match keyword_bit(n) {
            Some(bit) => acc | bit,
            None => acc,
        })
}

/// Vendor slot name → legacy's stable slot ID (lowercase + spaces stripped; off-hand-family
/// slots map to `weapon2`, main-hand-family to `weapon1`, matching legacy's
/// `slot_words_to_id` coverage).
fn slot_name_to_id(name: &str) -> String {
    match name.to_ascii_lowercase().as_str() {
        "body armour" => "bodyarmour".to_string(),
        "focus" | "shield" | "quiver" | "off hand" | "weapon 2" => "weapon2".to_string(),
        "weapon" | "weapons" | "main hand" | "weapon 1" => "weapon1".to_string(),
        other => other.replace(' ', ""),
    }
}

fn damage_type_bit(name: &str) -> Option<DamageType> {
    Some(match name {
        "Physical" => DamageType::Physical,
        "Fire" => DamageType::Fire,
        "Cold" => DamageType::Cold,
        "Lightning" => DamageType::Lightning,
        "Chaos" => DamageType::Chaos,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn tag(ty: &str, fields: &[(&str, StatMapValue)]) -> TagTemplate {
        TagTemplate {
            tag_type: ty.to_string(),
            fields: fields
                .iter()
                .cloned()
                .map(|(k, v)| (k.to_string(), v))
                .collect::<BTreeMap<_, _>>(),
        }
    }

    #[test]
    fn interpolate_capture_direct() {
        assert_eq!(interpolate("$1", &["5".into()]), "5");
        assert_eq!(interpolate("Rage", &[]), "Rage");
    }

    #[test]
    fn interpolate_cap_and_concat() {
        // "$2:cap+Effect" with cap "frenzy" → "FrenzyEffect"
        let caps = vec!["5".into(), "frenzy".into()];
        assert_eq!(interpolate("$2:cap+Effect", &caps), "FrenzyEffect");
    }

    #[test]
    fn multiplier_tag_with_capture_div() {
        let t = tag(
            "Multiplier",
            &[
                ("var", StatMapValue::Text("Rage".into())),
                ("div", StatMapValue::Text("$1".into())),
            ],
        );
        let got = compile_tag(&t, &["3".into()]).unwrap();
        match got {
            ModTag::Multiplier { var, div, .. } => {
                assert_eq!(var, "Rage");
                assert_eq!(div, 3.0);
            }
            _ => panic!("expected Multiplier"),
        }
    }

    #[test]
    fn multiplier_tag_cap_var() {
        let t = tag(
            "Multiplier",
            &[
                ("var", StatMapValue::Text("$2:cap+Effect".into())),
                ("div", StatMapValue::Text("$1".into())),
                ("actor", StatMapValue::Text("enemy".into())),
            ],
        );
        let got = compile_tag(&t, &["10".into(), "intimidate".into()]).unwrap();
        match got {
            ModTag::Multiplier {
                var, div, actor, ..
            } => {
                assert_eq!(var, "IntimidateEffect");
                assert_eq!(div, 10.0);
                assert_eq!(actor, None); // "enemy" isn't player/parent/minion → None (conservative)
            }
            _ => panic!("expected Multiplier"),
        }
    }

    #[test]
    fn condition_tag() {
        let t = tag(
            "Condition",
            &[("var", StatMapValue::Text("Onslaught".into()))],
        );
        assert_eq!(
            compile_tag(&t, &[]).unwrap(),
            ModTag::condition("Onslaught", false)
        );
    }

    #[test]
    fn condition_varlist_single_degenerates() {
        // The extracted shape of `while holding a (%w+)`: Condition has no `var`, only
        // varList=["Using+$1:cap"]. A single-element varList degenerates to a single Condition
        // (gear=shield → UsingShield, matching legacy's hardcoded "while holding a shield"
        // verbatim). Before the fix, only reading `var` dropped the whole entry (the root cause
        // of titan's UsingShield being inert).
        let t = tag(
            "Condition",
            &[(
                "varList",
                StatMapValue::List(vec![StatMapValue::Text("Using+$1:cap".into())]),
            )],
        );
        assert_eq!(
            compile_tag(&t, &["shield".into()]).unwrap(),
            ModTag::condition("UsingShield", false)
        );
    }

    #[test]
    fn condition_varlist_multi_maps_to_any_of() {
        // A multi-element varList (vendor OR semantics, ModStore.lua:596-607) → `ConditionAnyOf`
        // (matches if any one is true). Previously had no landing point and was conservatively
        // dropped as a whole.
        let t = tag(
            "Condition",
            &[(
                "varList",
                StatMapValue::List(vec![
                    StatMapValue::Text("Using+$1:cap".into()),
                    StatMapValue::Text("Using+$2:cap".into()),
                ]),
            )],
        );
        assert_eq!(
            compile_tag(&t, &["claw".into(), "shield".into()]).unwrap(),
            ModTag::ConditionAnyOf {
                vars: vec!["UsingClaw".into(), "UsingShield".into()],
                negated: false,
            }
        );
    }

    #[test]
    fn actor_condition_varlist_normalizes_each_var() {
        // Vendor's "while a rare or unique enemy is in your presence" →
        // ActorCondition{actor=enemy, varList={NearbyRareOrUniqueEnemy, RareOrUnique}}.
        // Each var goes through normalize_enemy_cond_var: non-rarity names get an Enemy prefix,
        // rarity names keep the bare form (legacy's/the orchestrator's key space). This is what
        // unblocks REAL gap #4.
        let t = tag(
            "ActorCondition",
            &[
                ("actor", StatMapValue::Text("enemy".into())),
                (
                    "varList",
                    StatMapValue::List(vec![
                        StatMapValue::Text("NearbyRareOrUniqueEnemy".into()),
                        StatMapValue::Text("RareOrUnique".into()),
                    ]),
                ),
            ],
        );
        assert_eq!(
            compile_tag(&t, &[]).unwrap(),
            ModTag::ConditionAnyOf {
                vars: vec!["EnemyNearbyRareOrUniqueEnemy".into(), "RareOrUnique".into()],
                negated: false,
            }
        );
    }

    #[test]
    fn skill_name_tag_maps_lowercased() {
        // Text stripped early by vendor's skillNameList (`increased Grenade Damage` →
        // Damage + SkillName{"Grenade"}) — the name is lowercased on ingest, and since no real
        // skill is literally named "grenade", it never matches = vendor's own zero-effect
        // outcome (same as the ModCache golden).
        let t = tag(
            "SkillName",
            &[("skillName", StatMapValue::Text("Grenade".into()))],
        );
        assert_eq!(
            compile_tag(&t, &[]).unwrap(),
            ModTag::SkillName {
                names: vec!["grenade".into()],
            }
        );
        assert!(is_mappable_tag_type("SkillName"));
    }

    #[test]
    fn unmappable_tag_returns_none() {
        let t = tag(
            "GlobalEffect",
            &[("effectName", StatMapValue::Text("Buff".into()))],
        );
        assert!(compile_tag(&t, &[]).is_none());
        assert!(!is_mappable_tag_type("GlobalEffect"));
    }

    #[test]
    fn multiplier_threshold_ailment_maps_to_enemy_condition() {
        // Vendor's `MultiplierThreshold{actor=enemy, var=PoisonStacks, threshold=1, upper=true}`
        // ("on targets that are not Poisoned", enemy poison stacks <1) → `Condition{EnemyPoisoned,
        // negated=true}`, mirroring legacy. Before the fix this returned None → engine treated
        // the whole line as a mismatch and dropped it (the root cause of Low Tolerance's +60%
        // poison magnitude being inert).
        let t = tag(
            "MultiplierThreshold",
            &[
                ("var", StatMapValue::Text("PoisonStacks".into())),
                ("threshold", StatMapValue::Number(1.0)),
                ("upper", StatMapValue::Bool(true)),
                ("actor", StatMapValue::Text("enemy".into())),
            ],
        );
        assert_eq!(
            compile_tag(&t, &[]).unwrap(),
            ModTag::condition("EnemyPoisoned", true)
        );
    }

    #[test]
    fn multiplier_threshold_scaling_limit_returns_none() {
        // A scaling form ("per Poison up to N", threshold=captured `$1`) on an ailment-stack
        // var that isn't a literal threshold=1 → still None (conservative, don't invent a
        // condition; avoids misreading a per-stack multiplier as a presence check).
        let t = tag(
            "MultiplierThreshold",
            &[
                ("var", StatMapValue::Text("PoisonStacks".into())),
                ("threshold", StatMapValue::Text("$1".into())),
            ],
        );
        assert!(compile_tag(&t, &["5".into()]).is_none());
        // A **lower-bound** threshold (upper defaults to false) on a non-ailment, non-distance
        // var → produced without further checks (A2 batch 2: a missing key reads as 0, and
        // 0 < threshold doesn't activate, so it's safe to under-apply; once the orchestrator
        // feeds the multiplier the entry auto-activates). This assertion used to be stuck on
        // the old "conservative None" semantics — PR#46 changed the semantics without updating
        // it (the lib unit tests weren't in the targeted gate at the time), fixed alongside
        // the grenade slice.
        let t2 = tag(
            "MultiplierThreshold",
            &[
                ("var", StatMapValue::Text("Rage".into())),
                ("threshold", StatMapValue::Number(1.0)),
            ],
        );
        assert_eq!(
            compile_tag(&t2, &[]).unwrap(),
            ModTag::MultiplierThreshold {
                var: "Rage".into(),
                threshold: 1.0,
                upper: false,
            }
        );
        // An upper-bound threshold (upper=true) still conservatively returns None (a missing
        // key reads as 0, and 0 ≤ threshold is always true, which would over-apply).
        let t3 = tag(
            "MultiplierThreshold",
            &[
                ("var", StatMapValue::Text("Rage".into())),
                ("threshold", StatMapValue::Number(1.0)),
                ("upper", StatMapValue::Bool(true)),
            ],
        );
        assert!(compile_tag(&t3, &[]).is_none());
    }

    /// `MultiplierThreshold{var=enemyDistance}` ("within/further than N metres") →
    /// [`ModTag::MultiplierThreshold`], with threshold converted from metres to internal units
    /// via the `:mult(10)` operator.
    #[test]
    fn multiplier_threshold_enemy_distance_translates() {
        let within = tag(
            "MultiplierThreshold",
            &[
                ("var", StatMapValue::Text("enemyDistance".into())),
                ("threshold", StatMapValue::Text("$1:mult(10)".into())),
                ("upper", StatMapValue::Bool(true)),
            ],
        );
        assert_eq!(
            compile_tag(&within, &["2".into()]).unwrap(),
            ModTag::MultiplierThreshold {
                var: "enemyDistance".into(),
                threshold: 20.0,
                upper: true,
            }
        );
    }

    #[test]
    fn flag_resolution() {
        let flags = compile_flags(&["Mace".into(), "Hit".into()]);
        assert!(flags.intersects(ModFlags::MACE));
        assert!(flags.intersects(ModFlags::HIT));
    }
}
