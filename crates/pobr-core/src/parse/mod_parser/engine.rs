//! Data-driven parseMod orchestration (vendor `ModParser.lua:6389-6755`).
//!
//! Entry point: [`parse_mod_engine`]`(text, &CompiledParserRules) -> ParseOutcome`.
//! The engine is already the main calc path: the orchestration layer always loads
//! `mod_parser_rules.json` via pobr-gamedata, compiles it into [`CompiledParserRules`], and
//! injects it into the session; legacy only serves as a fallback when engine rules aren't injected.
//!
//! Main sequence (mirrors vendor step by step):
//! 1. PoBR pre-pass (bracket stripping + whitespace normalization + a lowercase view, reusing
//!    legacy's semantics);
//! 2. unsupported table lookup;
//! 3. pre_flags scan (replicating the vendor quirk of appending a trailing space to the line);
//! 4. formList scan, no match → Unsupported;
//! 5. modTagList scan ×2;
//! 6. dispatch by form (forms.rs) → name/suffix/value;
//! 7. modFlagList scan;
//! 8. merge flags/keywordFlags/tagList → produce Vec<Modifier>;
//! 9. misc wrapping (addToAura/newAura/addToMinion/addToSkill/applyToEnemy → LIST mod);
//! 10. leftover non-whitespace text → unparsed.
//!
//! Wiring up the special channel / skillNameList / order=1/2 double pass tracks dual-run
//! coverage progress (steps 3/5; this batch implements the form main path first to reach C1
//! corpus diff=0 shape parity — special-channel wiring is deferred as a follow-up item, see
//! report §2.4).

use pobr_data::catalog::parser_rules::RuleEffectsDef;
use pobr_data::prelude::ModType;

use super::compiled::CompiledParserRules;
use super::forms::{FormReject, eval_form};
use super::outcome::{ParseOutcome, ParseStatus};
use super::template::{compile_flags, compile_keyword_flags, compile_tag};
use crate::{ModTag, ModValue, Modifier};
use pobr_data::modifier::{KeywordFlags, ModFlags};
use pobr_data::skill::SkillTypes;

/// Silent-degradation diagnostics from engine parsing (surfaced for A2): returned alongside
/// [`parse_mod_engine_diag`] and aggregated per line in the corpus report. Not carried on
/// [`ParseOutcome`] (would touch all 40 construction sites) and not part of canonical (the C1
/// dual-run gate doesn't compare it).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EngineDiag {
    /// Count of tags silently dropped when a pre_flag entry matched but the tag has no pobr
    /// landing type (dropping a tag broadens the modifier's scope into an approximate
    /// global-effect — the over-apply risk surface).
    pub dropped_pre_flag_tags: u16,
}

/// Parse one modifier line data-driven. Never errors (same closure as legacy's `Unsupported`;
/// empty input returns an empty Unsupported outcome).
///
/// Results are memoized via [`CompiledParserRules::memo`] — parsing is a pure function of
/// (text, rule set), so repeated lines (re-ingest / per-gem scans) hit the cache directly. The
/// diagnostic path [`parse_mod_engine_diag`] bypasses the cache (it needs a fresh diag count
/// each time).
pub fn parse_mod_engine(text: &str, rules: &CompiledParserRules) -> ParseOutcome {
    if let Some(cached) = rules.memo.get(text) {
        return cached;
    }
    let outcome = parse_mod_engine_diag(text, rules).0;
    rules.memo.insert(text, &outcome);
    outcome
}

/// Diagnostic variant of [`parse_mod_engine`]: also returns the silent-degradation count (for
/// the A2 report; both share the same implementation).
pub fn parse_mod_engine_diag(
    text: &str,
    rules: &CompiledParserRules,
) -> (ParseOutcome, EngineDiag) {
    let mut diag = EngineDiag::default();
    let outcome = parse_mod_engine_impl(text, rules, &mut diag);
    (outcome, diag)
}

fn parse_mod_engine_impl(
    text: &str,
    rules: &CompiledParserRules,
    diag: &mut EngineDiag,
) -> ParseOutcome {
    let original = text.trim();
    if original.is_empty() {
        return unsupported(text);
    }

    // 1. pre-pass: bracket stripping + whitespace normalization (keeps original case); the
    // lowercase view is only for matching.
    let cleaned = strip_pob_brackets(original);
    let normalized = normalize_spaces(&cleaned);
    let lower = normalized.to_ascii_lowercase();

    // 2. Whole-line unsupported lookup (lowercase).
    if rules.unsupported.contains(&lower) {
        return ParseOutcome {
            mods: Vec::new(),
            status: ParseStatus::Unsupported,
            unparsed: Some(normalized),
            special_meta: None,
        };
    }

    // 2b. specialModList channel (vendor `parseMod` looks up the whole-line specialModList
    //     table before formList, ModParser.lua:6151-6160). A match returns already-instantiated
    //     mods directly (with source text attached uniformly), matching vendor's
    //     specialModList anchor priority. When no data is injected, rules.special is always
    //     empty, so this branch never matches (behaviour = conv1 engine).
    if let Some(matched) = rules.special.try_match(&lower, &rules.special_handlers) {
        let mods: Vec<Modifier> = matched
            .mods
            .into_iter()
            .map(|mut inner| {
                inner.source = Some(original.to_string());
                inner
            })
            .collect();
        // A vendor specialModList match consumes the whole line — no leftover unparsed. An
        // empty mods result (pure recognition / unregistered handler) is still Parsed under
        // vendor semantics (recognized but produced nothing).
        return ParseOutcome {
            mods,
            status: ParseStatus::Parsed,
            unparsed: None,
            special_meta: None,
        };
    }

    // working line: vendor appends a trailing space before form scanning, `line = line .. " "`.
    let mut work = format!("{normalized} ");

    // 3. pre_flags scan (pattern, mostly ^-anchored).
    let mut effects_acc = EffectsAccumulator::default();
    {
        let lw = work.to_ascii_lowercase();
        if let Some((idx, _m, rest)) = rules.pre_flags.scan(&lw, &work) {
            let payload = rules.pre_flags.payload(idx);
            // Handler-backed entries: conservatively skip the effects (produce no mod, leave
            // it as unparsed) — this batch doesn't wire up the handler registry (Track A found
            // only 3 handler-backed entries in practice).
            if payload.handler_id.is_none() {
                // Note: when the absorb drop count is >0 (a tag has a type pobr has no landing
                // point for), we **keep the silent tag drop** — a large number of pre_flag
                // entries (buff/aura-domain prefixes) rely on the approximation of taking
                // global effect once the tag is dropped (most downstream cfg consumers lack a
                // skill_types bit). Tightening this to a conservative mismatch measured -4
                // def@5% / -9 @10% in practice. Scope tightening will land incrementally as
                // downstream per-skill cfg rolls out (same treatment as tag_phrase). The drop
                // count is recorded in diag (A2 report).
                diag.dropped_pre_flag_tags += effects_acc.absorb_pre_flag(payload, &_m.captures);
                work = rest;
            }
        }
    }

    // 4. formList scan.
    let (form_id, form_match, after_form) = {
        let lw = work.to_ascii_lowercase();
        match rules.forms.scan(&lw, &work) {
            Some((idx, m, rest)) => (rules.forms.payload(idx).clone(), m, rest),
            None => return unsupported_remaining(&work),
        }
    };
    work = after_form;

    // 5. modTagList scan ×2 (entries can carry two tags).
    for _ in 0..2 {
        let lw = work.to_ascii_lowercase();
        match rules.tag_phrases.scan(&lw, &work) {
            Some((idx, m, rest)) => {
                let payload = rules.tag_phrases.payload(idx);
                if payload.handler_id.is_some() {
                    // Handler-backed: not wired up this batch, stop the tag scan (conservative).
                    break;
                }
                let dropped = effects_acc.absorb_tag_phrase(&payload.effects, &m.captures);
                work = rest;
                if dropped > 0 {
                    // A tag has a type pobr has no landing point for → conservatively mismatch
                    // the whole line.
                    return unsupported_remaining(&work);
                }
            }
            None => break,
        }
    }

    // 6. form dispatch (name/suffix/value are resolved inside forms.rs, including any
    // form-internal scan).
    let lw = work.to_ascii_lowercase();
    let form_result = match eval_form(&form_id, &form_match, &lw, &work, rules) {
        Ok(r) => r,
        Err(FormReject::EmptyTable) => {
            // vendor `return {}, line`: recognized but produced nothing (Parsed, empty mods).
            return ParseOutcome {
                mods: Vec::new(),
                status: ParseStatus::Parsed,
                unparsed: tail_unparsed(&work),
                special_meta: None,
            };
        }
        Err(FormReject::Nil) => return unsupported_remaining(&work),
    };
    work = form_result.remaining.clone();

    // 6b. Inject the effects (keyword_flags / flags / tags) carried by a matched name_map entry
    //     into the accumulator (.3 normalization: keywordFlags/tags on vendor modNameList
    //     entries — e.g. the DamageType tag on each damage-type special name, or the Poison kw
    //     on `magnitude of poison you inflict`).
    if let Some(name_eff) = &form_result.name_effects {
        effects_acc.absorb_effects(name_eff, &[]);
    }

    // 7. modFlagList scan (plain).
    {
        let lw = work.to_ascii_lowercase();
        if let Some((idx, rest)) = rules.flag_phrases.scan(&lw, &work) {
            let eff = rules.flag_phrases.payload(idx);
            effects_acc.absorb_effects(eff, &[]);
            work = rest;
        }
    }

    // 8. Merge flags/kw/tags → Vec<Modifier>.
    let mut flags = effects_acc.flags | form_result.extra_flags;
    // Default keyword for the DMG family: only filled in when no explicit keyword was set
    // (vendor `modFlag or {kw=...}`).
    let mut keyword_flags = if effects_acc.keyword_flags.is_empty() {
        form_result.default_keyword
    } else {
        effects_acc.keyword_flags
    };
    // .3 normalization (C3): legacy has no Attack/Spell keyword bits, they're always folded
    // into ModFlags (legacy's parsing convention, see legacy.rs:1904 / :397). Fold keyword
    // ATTACK/SPELL into flags and clear the corresponding keyword bits — matching semantics
    // are equivalent under subset matching, and this keeps byte-for-byte parity with legacy.
    if keyword_flags.intersects(KeywordFlags::ATTACK) {
        flags |= ModFlags::ATTACK;
        keyword_flags = keyword_flags.without(KeywordFlags::ATTACK);
    }
    if keyword_flags.intersects(KeywordFlags::SPELL) {
        flags |= ModFlags::SPELL;
        keyword_flags = keyword_flags.without(KeywordFlags::SPELL);
    }

    let tags = effects_acc.tags.clone();

    // (bug #3 fix): the SPELL flag from a `Triggered Spells deal …` prefix and the SPELL bit
    // from `Spell Damage` scope are two independent sources in legacy — legacy's `SpellDamage`
    // special name still carries the prefix's SPELL bit (flags=0x2). Under the engine's single
    // flag channel, C3 normalization folding `Damage`+SPELL into a name would normally clear
    // SPELL; when a triggered SkillType tag is present (= the prefix source), keep the SPELL
    // bit instead, matching legacy byte-for-byte (downstream SPELL subset-matching semantics
    // are unaffected).
    let has_triggered = tags
        .iter()
        .any(|t| matches!(t, ModTag::SkillTypes(st) if st.intersects(SkillTypes::TRIGGERED)));

    let mut mods = Vec::with_capacity(form_result.names.len());
    for ((name, ty), value) in form_result
        .names
        .iter()
        .zip(form_result.types.iter())
        .zip(form_result.values.iter())
    {
        // The form's own suffix + the pre_flag `mod_suffix` (vendor `modSuffix`, e.g. `enemies
        // you curse take ` → `"Taken"`, so inner name `Damage` → `DamageTaken`,
        // ModParser.lua:6683 `name .. misc.modSuffix`).
        let full_name = format!("{}{}{}", name, form_result.suffix, effects_acc.mod_suffix);
        // .3 route-B engine normalization: fold vendor's "generic name + flag/kw" combinations
        // into PoBR special names (C3 damage flag→special name / Speed flag→AttackSpeed,
        // CastSpeed), and attach the corresponding DamageType tag by final name (C5) +
        // reabsorb the flag/kw bits consumed by the special name.
        let norm = normalize_pobr_name(&full_name, flags, keyword_flags, has_triggered);
        let modv = match ty {
            ModType::Flag => ModValue::Bool(*value != 0.0),
            _ => ModValue::Number(*value),
        };
        let mut m = Modifier::new(norm.name, *ty, modv).with_source(original);
        if !norm.flags.is_empty() {
            m = m.with_flags(norm.flags);
        }
        if !norm.keyword_flags.is_empty() {
            m = m.with_keyword_flags(norm.keyword_flags);
        }
        for t in &tags {
            m = m.with_tag(t.clone());
        }
        if let Some(dt) = norm.damage_type {
            m = m.with_tag(ModTag::DamageType(dt));
        }
        for cond in &norm.extra_conditions {
            m = m.with_tag(ModTag::condition(*cond, false));
        }
        if form_result.hand_attack_condition {
            // GRANTS/REMOVES local: the `{Hand}Attack` condition — pobr has no hand-placeholder
            // instantiation context here (hand is only known at item ingest time), so the
            // engine records a bare var for now (to be instantiated downstream). This batch
            // conservatively uses the fixed `MainHandAttack` (whether it matches legacy
            // behaviour is decided by dual-run; see report §2.4 D5).
            m = m.with_tag(ModTag::condition("MainHandAttack", false));
        }
        mods.push(m);
    }

    // C3 continued (splitting flagless `Speed`): vendor phrases like "attack and cast speed",
    // "use speed", "attack, cast and movement speed" map to the generic name `Speed` (no
    // attack/cast flag, so the C3 Speed-family branch above never fires). But PoBR's speed
    // calc aggregates **by name** over `[AttackSpeed, CastSpeed, SkillSpeed]`
    // (calc/skill_use_time.rs), and a bare `Speed` **isn't consumed by any speed calc** —
    // attack/cast speed is silently lost entirely (the root cause of low Speed on bow-shot/
    // ice-shot etc). legacy never produces a bare `Speed` (expand_compound turns "attack and
    // cast speed" → `["attack speed","cast speed"]` → AttackSpeed+CastSpeed, legacy.rs:735).
    // To match: split every bare `Speed` into AttackSpeed + CastSpeed.
    let mut split_mods = Vec::with_capacity(mods.len());
    for mut m in mods {
        if m.name.as_str() == "Speed" {
            let mut cast = m.clone();
            cast.name = "CastSpeed".into();
            m.name = "AttackSpeed".into();
            split_mods.push(m);
            split_mods.push(cast);
        } else {
            split_mods.push(m);
        }
    }
    let mods = split_mods;

    // 9. misc LIST wrapping (addToMinion / addToAura / newAura / addToSkill / applyToEnemy).
    let mods = effects_acc.wrap_list(mods);

    ParseOutcome {
        mods,
        status: ParseStatus::Parsed,
        unparsed: tail_unparsed(&work),
        special_meta: None,
    }
}

/// Accumulates the effects of pre_flag / flag_phrase / tag_phrase entries (flags / kw / tags +
/// minion / enemy wrapping).
///
/// LIST wrapping implemented so far: `addToMinion` (MinionModifier), `applyToEnemy`
/// (EnemyModifier), `newAura`+`newAuraOnlyAllies` (ExtraAura, not consumed by the player when
/// allies-only — vendor `ModParser.lua:6877` + `CalcPerform.lua:3104`). The remaining misc
/// wrappers (addToAura / addToSkill) aren't wired up yet (their lines are returned unchanged).
#[derive(Default)]
struct EffectsAccumulator {
    flags: ModFlags,
    keyword_flags: KeywordFlags,
    tags: Vec<ModTag>,
    add_to_minion: bool,
    add_to_minion_tags: Vec<ModTag>,
    /// `applyToEnemy` (vendor `applyToEnemy` / `actorEnemy`) — wraps the result as an
    /// `EnemyModifier LIST`, with the inner mod carrying enemy-side conditions plus
    /// `Condition(Effective)`.
    apply_to_enemy: bool,
    /// `newAura` (vendor) — wraps the result as an `ExtraAura LIST` (not consumed by the
    /// player's offence pipeline).
    new_aura: bool,
    /// `newAuraOnlyAllies` (vendor) — the aura only affects allies, so the player's own
    /// contribution is 0 (mirrors `CalcPerform.lua:3104`'s `if not onlyAllies` skipping the
    /// player).
    new_aura_only_allies: bool,
    /// `modSuffix` (vendor, e.g. `take ` → `"Taken"`) — appended to the end of the inner name.
    mod_suffix: String,
}

impl EffectsAccumulator {
    /// Absorb one RuleEffectsDef (flags/kw/tags + minion/enemy wrapping directives). Returns
    /// the **number of dropped tags** (>0 = a tag type pobr has no landing point for;
    /// surfaced in the A2 report).
    fn absorb_effects(&mut self, eff: &RuleEffectsDef, captures: &[String]) -> u16 {
        self.flags |= compile_flags(&eff.flags);
        self.keyword_flags = self.keyword_flags | compile_keyword_flags(&eff.keyword_flags);
        let mut dropped: u16 = 0;
        for tag in &eff.tags {
            match compile_tag(tag, captures) {
                Some(t) => self.tags.push(t),
                None => dropped += 1,
            }
        }
        // minion wrapping directives.
        self.add_to_minion |= eff.add_to_minion;
        for tag in &eff.add_to_minion_tags {
            if let Some(t) = compile_tag(tag, captures) {
                self.add_to_minion_tags.push(t);
            }
        }
        // enemy wrapping directives (applyToEnemy / actorEnemy both go through EnemyModifier
        // wrapping) + modSuffix.
        self.apply_to_enemy |= eff.apply_to_enemy || eff.actor_enemy;
        // aura wrapping directives (newAura + newAuraOnlyAllies).
        self.new_aura |= eff.new_aura;
        self.new_aura_only_allies |= eff.new_aura_only_allies;
        if let Some(suffix) = &eff.mod_suffix {
            self.mod_suffix = suffix.clone();
        }
        dropped
    }

    fn absorb_pre_flag(
        &mut self,
        payload: &super::compiled::PreFlagPayload,
        captures: &[String],
    ) -> u16 {
        self.absorb_effects(&payload.effects, captures)
    }

    fn absorb_tag_phrase(&mut self, eff: &RuleEffectsDef, captures: &[String]) -> u16 {
        self.absorb_effects(eff, captures)
    }

    /// misc wrapping: turn the produced mods into LIST-wrapped mods (vendor :6680-6750).
    /// Implemented so far: MinionModifier, EnemyModifier, ExtraAura (newAura+onlyAllies); the
    /// rest (addToAura / addToSkill) are conservatively skipped — those lines are returned
    /// unchanged.
    fn wrap_list(&self, mods: Vec<Modifier>) -> Vec<Modifier> {
        // `newAura` + `newAuraOnlyAllies` (vendor `ModParser.lua:6877`): wrap each inner mod as
        // an `ExtraAura LIST` — the player's offence pipeline only reads special names like
        // `Damage`/`SpellDamage`, never the `ExtraAura` wrapper, so the player's own
        // contribution is 0 (matching `CalcPerform.lua:3104`'s `if not onlyAllies` skipping the
        // player and only emitting to minions). Aura phrases that are **not** onlyAllies
        // ("You and Allies in your Presence …") are not wrapped — the player themself also
        // gets the inner mod (same as vendor: player is included in the `if not onlyAllies`
        // branch). Minion-side ExtraAura consumption is a follow-up (this build set doesn't
        // have minions contributing to player DPS, so wrapping alone fixes the player's
        // inflated numbers).
        if self.new_aura && self.new_aura_only_allies && !mods.is_empty() {
            return mods
                .into_iter()
                .map(|inner| {
                    let mut wrapper = Modifier::new(
                        "ExtraAura",
                        ModType::List,
                        ModValue::NestedMods(vec![inner.clone()]),
                    );
                    if let Some(src) = &inner.source {
                        wrapper = wrapper.with_source(src.clone());
                    }
                    wrapper
                })
                .collect();
        }
        if self.add_to_minion && !mods.is_empty() {
            return mods
                .into_iter()
                .map(|inner| {
                    let mut wrapper = Modifier::new(
                        "MinionModifier",
                        ModType::List,
                        ModValue::NestedMods(vec![inner.clone()]),
                    );
                    if let Some(src) = &inner.source {
                        wrapper = wrapper.with_source(src.clone());
                    }
                    for t in &self.add_to_minion_tags {
                        wrapper = wrapper.with_tag(t.clone());
                    }
                    wrapper
                })
                .collect();
        }
        // EnemyModifier wrapping (vendor `applyToEnemy`, ModParser.lua:6733-6748): a single
        // outer `EnemyModifier LIST NestedMods([inner...])`, with every inner mod uniformly
        // getting `Condition(Effective)` attached (pobr's enemy-side debuff convention,
        // legacy.rs:1236); enemy-side conditions use the `Enemy<X>` naming convention (vendor
        // enemy-actor conditions get an `Enemy` prefix in pobr; ones that already carry it,
        // like `EnemyInPresence`, aren't prefixed twice, legacy.rs:1198/:1207).
        if self.apply_to_enemy && !mods.is_empty() {
            let src = mods.first().and_then(|m| m.source.clone());
            let inner: Vec<Modifier> = mods
                .into_iter()
                .map(|mut m| {
                    m.tags = m.tags.into_iter().map(prefix_enemy_condition).collect();
                    m = m.with_tag(ModTag::condition("Effective", false));
                    m
                })
                .collect();
            let mut wrapper =
                Modifier::new("EnemyModifier", ModType::List, ModValue::NestedMods(inner));
            if let Some(src) = src {
                wrapper = wrapper.with_source(src);
            }
            return vec![wrapper];
        }
        mods
    }
}

/// Add an `Enemy` prefix to an enemy-side `Condition(var)` (vendor enemy-actor condition →
/// pobr's naming convention; ones that already carry the prefix, like
/// `EnemyInPresence`/`EnemyCursed`, aren't prefixed twice). Non-Condition tags pass through
/// unchanged.
fn prefix_enemy_condition(tag: ModTag) -> ModTag {
    match tag {
        ModTag::Condition {
            var,
            negated,
            actor,
        } if !var.starts_with("Enemy") => ModTag::Condition {
            var: format!("Enemy{var}"),
            negated,
            actor,
        },
        other => other,
    }
}

/// Result of engine name normalization: the PoBR special name + the reabsorbed flag/kw + any
/// DamageType tag to attach + extra conditions.
struct NormalizedName {
    name: String,
    flags: ModFlags,
    keyword_flags: KeywordFlags,
    damage_type: Option<pobr_data::prelude::DamageType>,
    /// Extra condition tags (C3 weapon scope on a non-damage name becomes `Condition(UsingX)`).
    extra_conditions: Vec<&'static str>,
}

/// .3 route-B engine normalization (C3 + C5): folds vendor's "generic name + flag" combinations
/// into PoBR special names, reabsorbing the flag bits the special name consumes; and attaches a
/// DamageType tag based on the final name.
///
/// - **C3 Damage family**: `Damage` + a weapon/scope flag → a special name (`SpellDamage` /
///   `ProjectileDamage` / `AreaDamage` / `{Weapon}Damage`), clearing the consumed bit (legacy's
///   special names never carry these flags).
/// - **C3 Speed family**: `Speed` + ATTACK → `AttackSpeed`, + CAST → `CastSpeed` (the
///   `Condition(UsingX)` implied by a weapon-attack combo is attached separately by
///   flag_phrases; the engine here conservatively only renames and clears the attack bit).
/// - **C5 DamageType**: if the final name is one of the five basic damage-type names, attach
///   the matching DamageType (same table as legacy's `damage_type_for_name`).
fn normalize_pobr_name(
    name: &str,
    flags: ModFlags,
    kw: KeywordFlags,
    keep_spell: bool,
) -> NormalizedName {
    use pobr_data::prelude::DamageType;

    let mut out_name = name.to_string();
    let mut out_flags = flags;
    let out_kw = kw;

    // (bug #4 fix) + fork-a: the resource-pool **prefix** used by suffix-concatenation families
    // (`GainAs<Dst>` / `ConvertTo<Dst>`) must use vendor's short pool name (`Life`/`Mana`), not
    // the extraction-time aliased `MaximumLife`/`MaximumMana`. Both vendor
    // `CalcDefence.lua:92,1316` and pobr's downstream `calc/defence.rs`
    // `format!("{src}ConvertTo{dst}")` (src=`Life`) rebuild the lookup name from the short
    // form — if the engine produced `MaximumLifeConvertToEnergyShield`, the conversion matrix
    // lookup would miss (silently dropping the conversion; the root cause of detonate-dead's
    // 0.38x overall defence / comet's low PhysMaxHit). `GainAs` was fixed earlier; `ConvertTo`
    // in the same family was missed — strip the `Maximum` prefix here too, matching legacy
    // byte-for-byte.
    if name.contains("GainAs") || name.contains("ConvertTo") {
        if let Some(rest) = name.strip_prefix("MaximumLife") {
            out_name = format!("Life{rest}");
        } else if let Some(rest) = name.strip_prefix("MaximumMana") {
            out_name = format!("Mana{rest}");
        }
    }

    // C3 Damage family: generic name Damage + a scope flag → a special name. Priority: spell >
    // projectile > area > weapon (matches legacy's special-name mapping; in practice items have
    // at most one scope flag per line).
    if name == "Damage" {
        // Weapon flags (with their accompanying HIT bit) → special name, clearing the weapon
        // bit and the HIT bit.
        const WEAPON_SPECIALS: &[(ModFlags, &str)] = &[
            (ModFlags::SPEAR, "SpearDamage"),
            (ModFlags::CROSSBOW, "CrossbowDamage"),
            (ModFlags::BOW, "BowDamage"),
            (ModFlags::MACE, "MaceDamage"),
            // PoE2's quarterstaff bit is `Staff` (0x200000), not `Warstaff` — same fix as
            // WEAPON_COND below (previously using WARSTAFF never matched Staff-flag
            // quarterstaff entries, so QuarterstaffDamage was unreachable; switching to STAFF
            // matches legacy's special-name mapping).
            (ModFlags::STAFF, "QuarterstaffDamage"),
        ];
        if flags.intersects(ModFlags::SPELL) {
            out_name = "SpellDamage".to_string();
            // By default clear the scope SPELL bit (legacy's special name doesn't carry it);
            // but the SPELL bit from a triggered prefix is an independent source that legacy
            // keeps (bug #3 fix) — don't clear it when keep_spell is set.
            if !keep_spell {
                out_flags = out_flags.without(ModFlags::SPELL);
            }
        } else if let Some((bit, special)) =
            WEAPON_SPECIALS.iter().find(|(b, _)| flags.intersects(*b))
        {
            out_name = (*special).to_string();
            out_flags = out_flags.without(*bit | ModFlags::HIT);
        } else if flags.intersects(ModFlags::PROJECTILE) {
            out_name = "ProjectileDamage".to_string();
            out_flags = out_flags.without(ModFlags::PROJECTILE);
        } else if flags.intersects(ModFlags::AREA) {
            out_name = "AreaDamage".to_string();
            out_flags = out_flags.without(ModFlags::AREA);
        }
    }

    // C3 Speed family: generic name Speed + ATTACK/CAST → AttackSpeed/CastSpeed, clearing the
    // matching bit.
    let mut extra_conditions: Vec<&'static str> = Vec::new();
    if name == "Speed" {
        if flags.intersects(ModFlags::ATTACK) {
            out_name = "AttackSpeed".to_string();
            out_flags = out_flags.without(ModFlags::ATTACK);
        } else if flags.intersects(ModFlags::CAST) {
            out_name = "CastSpeed".to_string();
            out_flags = out_flags.without(ModFlags::CAST);
        }
    }

    // C3 weapon scope on a non-damage name (Speed/Crit etc): legacy converts it to
    // `Condition(UsingX)` + clears the HIT bit (keeping the weapon bit). Damage names have
    // already been absorbed by the special-name branch above and don't enter this branch.
    if !out_name.ends_with("Damage") && !out_name.starts_with("Damage") {
        // Combined weapon categories (Weapon1H/2H + WeaponMelee) → `UsingOneHandedMelee` /
        // `UsingTwoHandedMelee` (vendor's "with one handed melee weapons" = Weapon1H|
        // WeaponMelee|Hit). Matched by **full-bitset subset** and checked **before** the
        // single-type table — otherwise a plain "with melee weapons" (WeaponMelee only) would
        // be misjudged as one/two-handed melee. `|` isn't const, hence `let` here.
        let weapon_combo_cond: [(ModFlags, &str); 2] = [
            (
                ModFlags::WEAPON_1H | ModFlags::WEAPON_MELEE,
                "UsingOneHandedMelee",
            ),
            (
                ModFlags::WEAPON_2H | ModFlags::WEAPON_MELEE,
                "UsingTwoHandedMelee",
            ),
        ];
        // Single weapon type → `Using<Type>`. PoE2's quarterstaff bit is `Staff` (0x200000);
        // the old table wrongly used `WARSTAFF` (0x20000000) → quarterstaff-scoped global
        // attack-speed/crit entries never got their condition attached and never matched.
        const WEAPON_COND: &[(ModFlags, &str)] = &[
            (ModFlags::SPEAR, "UsingSpear"),
            (ModFlags::CROSSBOW, "UsingCrossbow"),
            (ModFlags::BOW, "UsingBow"),
            (ModFlags::MACE, "UsingMace"),
            (ModFlags::STAFF, "UsingQuarterstaff"),
        ];
        if let Some((_, cond)) = weapon_combo_cond
            .iter()
            .find(|(b, _)| b.is_subset_of(out_flags))
        {
            extra_conditions.push(cond);
            out_flags = out_flags.without(ModFlags::HIT);
        } else if let Some((_, cond)) = WEAPON_COND.iter().find(|(b, _)| out_flags.intersects(*b)) {
            extra_conditions.push(cond);
            out_flags = out_flags.without(ModFlags::HIT);
        }
    }

    // C5 DamageType: if the final name is one of the five basic damage-type names, attach the
    // matching DamageType.
    let damage_type = match out_name.as_str() {
        "PhysicalDamage" => Some(DamageType::Physical),
        "FireDamage" => Some(DamageType::Fire),
        "ColdDamage" => Some(DamageType::Cold),
        "LightningDamage" => Some(DamageType::Lightning),
        "ChaosDamage" => Some(DamageType::Chaos),
        _ => None,
    };

    NormalizedName {
        name: out_name,
        flags: out_flags,
        keyword_flags: out_kw,
        damage_type,
        extra_conditions,
    }
}

fn unsupported(text: &str) -> ParseOutcome {
    ParseOutcome {
        mods: Vec::new(),
        status: ParseStatus::Unsupported,
        unparsed: Some(text.trim().to_string()),
        special_meta: None,
    }
}

fn unsupported_remaining(work: &str) -> ParseOutcome {
    ParseOutcome {
        mods: Vec::new(),
        status: ParseStatus::Unsupported,
        unparsed: Some(work.trim().to_string()),
        special_meta: None,
    }
}

/// vendor `line:match("%S") and line`: leftover text with any non-whitespace becomes unparsed,
/// otherwise None.
fn tail_unparsed(work: &str) -> Option<String> {
    if work.chars().any(|c| !c.is_whitespace()) {
        Some(work.trim().to_string())
    } else {
        None
    }
}

// ---- pre-pass reuses legacy's semantics (step 1, no maintaining two implementations — these
//      two functions are private to legacy, so the engine side replicates the same logic and
//      keeps it byte-for-byte identical; dual-run catches any drift) ----

fn strip_pob_brackets(text: &str) -> String {
    if !text.contains('[') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            let mut inner = String::new();
            for ic in chars.by_ref() {
                if ic == ']' {
                    break;
                }
                inner.push(ic);
            }
            let display = inner.rsplit('|').next().unwrap_or(&inner);
            out.push_str(display);
        } else {
            out.push(c);
        }
    }
    out
}

fn normalize_spaces(text: &str) -> String {
    // legacy's normalize_spaces also lowercases — but the engine needs to keep the original
    // case for faithful text slicing/source. So this only normalizes whitespace; the lowercase
    // view is computed separately.
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Test helper: path to the real rule table.
#[cfg(test)]
pub mod test_support {
    use std::path::PathBuf;

    /// Path to the repo's real mod_parser_rules.json.
    pub fn real_rules_path() -> PathBuf {
        // engine.rs lives in crates/pobr-core/src/mod_parser/, 4 levels up to the repo root.
        let manifest = env!("CARGO_MANIFEST_DIR"); // crates/pobr-core
        PathBuf::from(manifest)
            .join("../../data")
            .join(pobr_data::data_version())
            .join("overlay/mod_parser_rules.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pobr_data::catalog::parser_rules::ModParserRulesDoc;

    fn real_rules() -> CompiledParserRules {
        let json = std::fs::read_to_string(test_support::real_rules_path()).unwrap();
        let doc: ModParserRulesDoc = serde_json::from_str(&json).unwrap();
        CompiledParserRules::compile(&doc).unwrap()
    }

    #[test]
    fn inc_fire_damage() {
        let r = real_rules();
        let o = parse_mod_engine("50% increased Fire Damage", &r);
        assert_eq!(o.status, ParseStatus::Parsed, "unparsed={:?}", o.unparsed);
        assert_eq!(o.mods.len(), 1);
        assert_eq!(o.mods[0].name.as_str(), "FireDamage");
        assert_eq!(o.mods[0].mod_type, ModType::Inc);
        assert_eq!(o.mods[0].value, ModValue::Number(50.0));
    }

    #[test]
    fn flat_max_life() {
        let r = real_rules();
        let o = parse_mod_engine("+50 to maximum Life", &r);
        assert_eq!(o.status, ParseStatus::Parsed, "unparsed={:?}", o.unparsed);
        // .3 route B: after extraction-time normalization, the engine produces PoBR StatId
        // `MaximumLife` (not vendor's `Life`).
        assert!(o.mods.iter().any(|m| m.name.as_str() == "MaximumLife"
            && m.mod_type == ModType::Base
            && m.value == ModValue::Number(50.0)));
    }

    #[test]
    fn unsupported_garbage() {
        let r = real_rules();
        let o = parse_mod_engine("this is not a modifier xyzzy", &r);
        assert_eq!(o.status, ParseStatus::Unsupported);
    }

    #[test]
    fn mirrored_is_unsupported() {
        let r = real_rules();
        let o = parse_mod_engine("Mirrored", &r);
        assert_eq!(o.status, ParseStatus::Unsupported);
    }

    #[test]
    fn life_convert_to_es_strips_maximum_prefix() {
        // Vendor's "N% of Maximum Life Converted to Energy Shield" leading name gets aliased
        // by extraction-time normalization to `MaximumLife`, but the pool-conversion families
        // `ConvertTo<Dst>`/`GainAs<Dst>` need vendor's short pool name `Life`
        // (calc/defence.rs rebuilds the lookup name as `{src}ConvertTo{dst}` with src=`Life`;
        // vendor CalcDefence.lua:92). The engine must produce `LifeConvertToEnergyShield`, not
        // `MaximumLifeConvertToEnergyShield` (the latter misses the conversion matrix lookup →
        // the conversion is silently dropped).
        let r = real_rules();
        let o = parse_mod_engine("5% of Maximum Life Converted to Energy Shield", &r);
        assert_eq!(o.status, ParseStatus::Parsed, "unparsed={:?}", o.unparsed);
        assert!(
            o.mods
                .iter()
                .any(|m| m.name.as_str() == "LifeConvertToEnergyShield"),
            "应产 LifeConvertToEnergyShield: {:?}",
            o.mods
        );
        assert!(
            !o.mods
                .iter()
                .any(|m| m.name.as_str() == "MaximumLifeConvertToEnergyShield"),
            "不应残留 MaximumLifeConvertToEnergyShield: {:?}",
            o.mods
        );
    }

    #[test]
    fn attack_and_cast_speed_splits_to_attack_cast() {
        // Vendor's "attack and cast speed" maps to the generic name `Speed` (no attack/cast
        // flag); PoBR's speed bucket aggregates by name over [AttackSpeed,CastSpeed,SkillSpeed]
        // and doesn't consume a bare `Speed`, so it's split into AttackSpeed + CastSpeed
        // (matching legacy's expand_compound, legacy.rs:735).
        let r = real_rules();
        let o = parse_mod_engine("8% increased Attack and Cast Speed", &r);
        assert_eq!(o.status, ParseStatus::Parsed, "unparsed={:?}", o.unparsed);
        assert!(
            o.mods.iter().any(|m| m.name.as_str() == "AttackSpeed"
                && m.mod_type == ModType::Inc
                && m.value == ModValue::Number(8.0)),
            "应产 AttackSpeed Inc 8: {:?}",
            o.mods
        );
        assert!(
            o.mods
                .iter()
                .any(|m| m.name.as_str() == "CastSpeed" && m.value == ModValue::Number(8.0)),
            "应产 CastSpeed Inc 8: {:?}",
            o.mods
        );
        assert!(
            !o.mods.iter().any(|m| m.name.as_str() == "Speed"),
            "不应残留 bare Speed: {:?}",
            o.mods
        );
    }
}
