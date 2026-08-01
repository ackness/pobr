//! Per-type hit damage components (DamageComponent) + damage conversion chain + gain-as-extra.
//!
//! Expands a single physical bucket into a component vector split by damage
//! type (physical / fire / cold / lightning / chaos): each component
//! independently aggregates `base × (1 + Σinc/100) × Πmore`, and summing
//! them gives the total (non-crit) hit damage. This is the foundation for
//! subsequent damage conversion / per-type hit damage / ailments.
//!
//! ## Damage conversion chain (mirrors PoB2 `CalcOffence.lua`'s conversion section verbatim)
//!
//! - Fixed order `Physical → Lightning → Cold → Fire → Chaos` (`dmgTypeList` / [`DAMAGE_TYPES`]).
//! - Two stages: **skill conversion** first
//!   (`Skill<From>DamageConvertTo<To>` / `SkillDamageConvertTo<To>`), then
//!   **global conversion** (`<From>DamageConvertTo<To>` /
//!   `DamageConvertTo<To>` / `ElementalDamageConvertTo<To>` / `NonChaosDamageConvertTo<To>`).
//! - When the total conversion from a single source exceeds 100%, it's
//!   proportionally normalized to 100% (`factor = 100 / total`).
//! - **inc/more only by final type** (PoE2, no conversion-source double-dip):
//!   a converted/gained component only picks up increased/more for **its own
//!   final damage type** (plus the shared Elemental group), and never
//!   accumulates the conversion-source type. Primary source: PoB2
//!   `calcDamage(...,damageType,0)` (CalcOffence.lua:3990, typeFlags only
//!   contains the final type) plus headless oracle verification. PoE1's
//!   "conversion-source double-dip" has been removed in PoE2. See
//!   damage-scaling.md §Converted component semantics.
//! - **gain-as-extra** (`<From>DamageGainAs<To>` / `DamageGainAs<To>` etc.
//!   BASE%): an extra damage packet that is **not deducted from the source
//!   and doesn't participate in normalization**; inc/more likewise only apply to the target (final) type.
//!
//! ## Backward compatibility
//!
//! With no conversion / gain-as-extra modifier at all, [`calculate_components`]
//! takes the fast path that's completely identical to the historical
//! "per-type base × scale" implementation ([`scale_components_no_conversion`]), with output unchanged byte for byte.

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::round;

/// A single damage component: the aggregated min/max, including the hit/DoT
/// distinction, source bucket, and the set of types traversed by conversion.
///
/// **`type_path`** (08-mechanics §2.3, damage-scaling.md §Converted component
/// semantics): every damage type this component passed through on the
/// conversion chain (deduplicated, in [`DAMAGE_TYPES`] order). E.g. the fire
/// component from 50% physical converted to fire has
/// `type_path = [Physical, Fire]`; **this is for attribution/display only**
/// -- inc/more aggregation only picks up the final type's `FireDamage`
/// (+`ElementalDamage`), never the conversion source's `PhysicalDamage`.
/// An unconverted component's `type_path` only contains its own type, equivalent to the historical single-type aggregation.
#[derive(Debug, Clone, PartialEq)]
pub struct DamageComponent {
    pub damage_type: DamageType,
    pub min: f64,
    pub max: f64,
    /// Hit vs DoT (a DoT never crits, never rolls). Defaults to [`DamageKind::Hit`].
    pub kind: DamageKind,
    /// Source bucket (Attack / Spell / Secondary / ...). Defaults to [`DamageSource::Attack`].
    pub source: DamageSource,
    /// The set of damage types traversed by the conversion chain
    /// (deduplicated, ordered); for attribution/display only, inc/more only apply to the final type.
    pub type_path: Vec<DamageType>,
}

impl DamageComponent {
    /// Backward-compatible constructor: Hit / Attack, `type_path` only contains its own type.
    pub fn new(damage_type: DamageType, min: f64, max: f64) -> Self {
        Self {
            damage_type,
            min,
            max,
            kind: DamageKind::Hit,
            source: DamageSource::Attack,
            type_path: vec![damage_type],
        }
    }

    /// Enriched constructor: explicitly specifies kind / source
    /// (`type_path` still only contains its own type).
    pub fn with_kind_source(
        damage_type: DamageType,
        min: f64,
        max: f64,
        kind: DamageKind,
        source: DamageSource,
    ) -> Self {
        Self {
            damage_type,
            min,
            max,
            kind,
            source,
            type_path: vec![damage_type],
        }
    }

    /// Overrides `type_path` (used by conversion chain output). Automatically
    /// ensures the target type is in the path, deduplicated, sorted by chain order.
    pub fn with_type_path(mut self, path: impl IntoIterator<Item = DamageType>) -> Self {
        let mut set: Vec<DamageType> = Vec::new();
        for ty in path {
            if !set.contains(&ty) {
                set.push(ty);
            }
        }
        if !set.contains(&self.damage_type) {
            set.push(self.damage_type);
        }
        set.sort_by_key(|ty| type_order_index(*ty));
        self.type_path = set;
        self
    }

    /// This component's average hit damage `(min + max) / 2`.
    pub fn avg(&self) -> f64 {
        (self.min + self.max) / 2.0
    }

    /// Average hit damage with the lucky roll folded in (PoB2 `CalcOffence.lua:4043-4046`).
    ///
    /// `avg = (min/2 + max/2) × (1 − p) + (min/3 + 2×max/3) × p`, where `p`
    /// is the lucky chance (fraction 0..=1, clamped if out of range). Lucky
    /// = rolling twice and taking the higher, so the average skews toward
    /// max. Bit-for-bit identical to [`avg`](Self::avg) when `p = 0` (the
    /// old path is preserved). See [`lucky_hit_chance`] for how the chance
    /// is resolved (consumed by pass × damage_type inside crit_pass).
    pub fn avg_with_lucky(&self, lucky_chance: f64) -> f64 {
        let p = lucky_chance.clamp(0.0, 1.0);
        let not_lucky = self.min / 2.0 + self.max / 2.0;
        let lucky = self.min / 3.0 + 2.0 * self.max / 3.0;
        not_lucky * (1.0 - p) + lucky * p
    }
}

/// A single component's lucky chance (fraction 0..=1, PoB2 `CalcOffence.lua:4036-4042`).
///
/// Sources of full lucky (=1) (any match gives 1):
/// - `LuckyHits`: always lucky;
/// - `CritLucky`: crit pass only (vendor `pass == 1`) -- note this is a
///   different mechanic from `crit.rs`'s `CritChanceLucky` (rolling the
///   crit **chance** itself), don't conflate them;
/// - `LightningNoCritLucky`: non-crit pass only and the Lightning component (vendor `pass == 2`);
/// - `ElementalLuckHits`: the three elemental components (Lightning/Cold/Fire).
///
/// Otherwise `p = min(Sum(BASE, "<Type>LuckyHitsChance", "LuckyHitsChance"), 100) / 100`.
pub fn lucky_hit_chance(
    db: &ModDb,
    cfg: &CalcConfig,
    damage_type: DamageType,
    is_crit_pass: bool,
) -> f64 {
    if db.flag(cfg, ModName::from("LuckyHits"))
        || (!is_crit_pass
            && damage_type == DamageType::Lightning
            && db.flag(cfg, ModName::from("LightningNoCritLucky")))
        || (is_crit_pass && db.flag(cfg, ModName::from("CritLucky")))
        || (damage_type.is_elemental() && db.flag(cfg, ModName::from("ElementalLuckHits")))
    {
        return 1.0;
    }
    let prefix = type_prefix(damage_type);
    let names = [
        ModName::from(format!("{prefix}LuckyHitsChance")),
        ModName::from("LuckyHitsChance"),
    ];
    db.sum(ModType::Base, cfg, &names).min(100.0) / 100.0
}

/// canDeal / `DealNo<Type>` gating (contract 3's frozen signature; PoB2
/// `CalcOffence.lua:2226-2230`, consumed at `:3989/:4793/:5451` -- shared by
/// hit / ailment / DoT).
///
/// `canDeal[type] = not Flag("DealNo"..type, "DealNoDamage")`; a type
/// component that can't deal damage is **zeroed in place** (min/max → 0, the
/// component itself is kept -- semantically equivalent to vendor skipping it in downstream sums/buckets).
///
/// **Order matters**: conversion happens first, and what gets zeroed is
/// **what remains after conversion** -- this function must be called after
/// the conversion chain ([`calculate_components`] / [`convert_damage`]). For
/// example, in an Avatar of Fire build: after physical converts to fire, the
/// remaining physical is zeroed by `DealNoPhysical`, while the already-converted fire is kept.
///
/// Signature note: contract 3 specifies `&mut Vec<DamageComponent>`; this
/// implementation instead takes the more general `&mut [DamageComponent]`
/// (clippy's `ptr_arg`), which callers passing `&mut vec` are directly
/// compatible with via deref -- the call shape is unchanged.
pub fn apply_can_deal(components: &mut [DamageComponent], db: &ModDb, cfg: &CalcConfig) {
    let deal_no_damage = db.flag(cfg, ModName::from("DealNoDamage"));
    for component in components.iter_mut() {
        let gated = deal_no_damage
            || db.flag(
                cfg,
                ModName::from(format!("DealNo{}", type_prefix(component.damage_type))),
            );
        if gated {
            component.min = 0.0;
            component.max = 0.0;
        }
    }
}

/// The full set of damage types in a fixed calculation order, guaranteeing
/// deterministic ordering of the component vector.
///
/// **Bug#7 fix (damage-conversion-chain-order-wrong)**:
/// must match PoB2's conversion chain order: `Physical → Lightning → Cold → Fire → Chaos`
/// (PoB2 `CalcOffence.lua`'s `dmgTypeList`; damage-scaling.md §Conversion order and chaining).
pub const DAMAGE_TYPES: [DamageType; 5] = [
    DamageType::Physical,
    DamageType::Lightning,
    DamageType::Cold,
    DamageType::Fire,
    DamageType::Chaos,
];

/// `DamageType`'s index within [`DAMAGE_TYPES`] (used for sorting `type_path` in chain order).
fn type_order_index(damage_type: DamageType) -> usize {
    DAMAGE_TYPES
        .iter()
        .position(|t| *t == damage_type)
        .unwrap_or(usize::MAX)
}

/// Maps a `DamageType` to its stable modifier name prefix (matching PoB's naming).
fn type_prefix(damage_type: DamageType) -> &'static str {
    match damage_type {
        DamageType::Physical => "Physical",
        DamageType::Fire => "Fire",
        DamageType::Cold => "Cold",
        DamageType::Lightning => "Lightning",
        DamageType::Chaos => "Chaos",
    }
}

/// A single damage type's non-crit hit component base value (flat), excluding inc/more / conversion.
///
/// - Physical: the base comes from the weapon hit's `base_hit_min/max`
///   (from the skill/weapon itself, unaffected by AddedDamage MORE), plus
///   `PhysicalDamageMin/Max` Base additions (affected by AddedDamage MORE effectiveness).
/// - Other types: come from `<Type>DamageMin/Max` Base additions (flat
///   added damage, affected by AddedDamage MORE effectiveness).
///
/// **Bug#8 fix (added-damage-effectiveness-missing)**:
/// added damage effectiveness (the `AddedDamage` effectiveness multiplier)
/// only multiplies external flat added damage, not the skill/weapon's own base.
/// Source: damage-scaling.md §Added Damage Effectiveness;
///       PoB2 CalcOffence.lua:3909's `addedMult = calcLib.mod(skillModList, cfg,
///       "Added"..damageType.."Damage", "AddedDamage")`
///       only multiplies `addedMin * addedMult`, never `source[...]` (the weapon/skill's own damage).
///
/// **Completion (addedMult's INC leg)**: vendor's `calcLib.mod`
/// (CalcTools.lua:16-18) = `(1 + Sum(INC, cfg, names...)/100) × More(cfg, names...)`
/// -- both the INC and MORE legs share the same name set
/// (`Added<Type>Damage` + `AddedDamage`). The old implementation only had
/// the MORE leg; this now adds the INC leg via a single multi-name query in
/// vendor's name order (multi-name `more`'s per-name rounding semantics match vendor's single call).
fn base_flat(
    db: &ModDb,
    cfg: &CalcConfig,
    damage_type: DamageType,
    base_hit_min: f64,
    base_hit_max: f64,
) -> (f64, f64) {
    let prefix = type_prefix(damage_type);
    let min_name = ModName::from(format!("{prefix}DamageMin"));
    let max_name = ModName::from(format!("{prefix}DamageMax"));
    let added_min = db.sum(ModType::Base, cfg, &[min_name]);
    let added_max = db.sum(ModType::Base, cfg, &[max_name]);

    // Added damage effectiveness (addedMult = (1 + ΣINC/100) × ΠMORE): only
    // applies to external flat added damage, never the skill/weapon's own
    // base. Name set = `Added<Type>Damage` + `AddedDamage` (vendor's order).
    let eff_names = [
        ModName::from(format!("Added{prefix}Damage")),
        ModName::from("AddedDamage"),
    ];
    let eff = (1.0 + db.sum(ModType::Inc, cfg, &eff_names) / 100.0) * db.more(cfg, &eff_names);

    match damage_type {
        DamageType::Physical => {
            // The weapon/skill's own base is unaffected by effectiveness; flat added is affected
            (
                base_hit_min + added_min * eff,
                base_hit_max + added_max * eff,
            )
        }
        _ => (added_min * eff, added_max * eff),
    }
}

/// Calculates the hit component vector across every damage type (including
/// the conversion chain + gain-as-extra; inc/more only by final type).
///
/// Pipeline:
/// 1. Computes each type's flat base in [`DAMAGE_TYPES`] order
///    ([`base_flat`], including added effectiveness).
/// 2. Reads [`ConversionRules`] (skill + global convert + gain-as-extra).
///    **When entirely empty**, takes the [`scale_components_no_conversion`]
///    fast path, with output identical to history byte for byte.
/// 3. Otherwise runs [`apply_conversion_chain`]: produces "post-conversion
///    base components" carrying `type_path`, each then aggregating inc/more
///    by its own **final damage type** (PoE2 has no conversion-source double-dip).
///
/// **Bug#6 fix (missing-elemental-damage-modname-group)**: fire/cold/
/// lightning components' inc/more must include the `ElementalDamage` shared
/// group (expanded uniformly inside [`aggregate_inc_more`]).
///
/// A component is only included in the vector when its base (min or max) is
/// non-zero; the physical component is always included (the weapon hit
/// baseline), so a pure-physical path matches the old implementation exactly.
pub(crate) fn calculate_components(
    db: &ModDb,
    cfg: &CalcConfig,
    base_hit_min: f64,
    base_hit_max: f64,
) -> Vec<DamageComponent> {
    // Step 1: each type's flat base.
    let mut base: Vec<(DamageType, f64, f64)> = Vec::with_capacity(DAMAGE_TYPES.len());
    for &damage_type in DAMAGE_TYPES.iter() {
        let type_cfg = cfg.clone().with_damage_type(damage_type);
        let (min, max) = base_flat(db, &type_cfg, damage_type, base_hit_min, base_hit_max);
        base.push((damage_type, min, max));
    }

    // Step 2: conversion rules. No conversion/gain at all → the fast path (backward compatible, byte for byte).
    let rules = ConversionRules::from_mod_db(db, cfg);
    if rules.is_empty() {
        return scale_components_no_conversion(db, cfg, &base);
    }

    // Step 3: run the conversion chain, producing "post-conversion base components" carrying type_path.
    let converted = apply_conversion_chain(&base, &rules);
    if dbg_env!("POBR_DBG_BASES").is_some() {
        for (t, mn, mx) in &base {
            eprintln!("[POBR_BASE] {t:?} {mn:.2}/{mx:.2}");
        }
        for c in &converted {
            eprintln!(
                "[POBR_SUMMED] {:?} {:.2}/{:.2}",
                c.damage_type, c.min, c.max
            );
        }
    }

    // Step 4: each aggregates inc/more by final damage type (PoE2 has no conversion-source double-dip).
    converted
        .into_iter()
        .filter(|comp| {
            comp.damage_type == DamageType::Physical || comp.min != 0.0 || comp.max != 0.0
        })
        .map(|comp| scale_with_path(db, cfg, comp))
        .collect()
}

/// No-conversion fast path: per-type `base × (1 + Σinc/100) × Πmore`, equivalent to the historical implementation.
///
/// Kept separate from the conversion path, guaranteeing that pure physical /
/// flat-added-only output is **unchanged byte for byte** (regression-safe).
fn scale_components_no_conversion(
    db: &ModDb,
    cfg: &CalcConfig,
    base: &[(DamageType, f64, f64)],
) -> Vec<DamageComponent> {
    base.iter()
        .filter_map(|&(damage_type, base_min, base_max)| {
            let is_physical = damage_type == DamageType::Physical;
            if !is_physical && base_min == 0.0 && base_max == 0.0 {
                return None;
            }
            let comp = DamageComponent::new(damage_type, base_min, base_max);
            Some(scale_with_path(db, cfg, comp))
        })
        .collect()
}

/// Expands every relevant ModName per the component's `type_path`,
/// aggregating inc/more and scaling min/max.
///
/// A component with `type_path = [Physical, Fire]` picks up inc/more from
/// both `PhysicalDamage` and `FireDamage` (plus the elemental
/// `ElementalDamage`) and the generic `AttackDamage`/`Damage` -- a double-dip.
/// Degenerates to historical per-type aggregation when `type_path` only contains a single type.
fn scale_with_path(db: &ModDb, cfg: &CalcConfig, comp: DamageComponent) -> DamageComponent {
    let (inc, more) = aggregate_inc_more(db, cfg, comp.damage_type);
    let scale = (1.0 + inc / 100.0) * more;
    if dbg_env!("POBR_DBG_BASES").is_some() {
        eprintln!(
            "[POBR_POOL] {:?} inc={inc:.2} more={more:.4} scale={scale:.4}",
            comp.damage_type
        );
    }
    // PoB2's `calcDamage` (CalcOffence.lua:138-139,153-154): min and max
    // each additionally multiply by their own independent MORE factor,
    // `Min<Type>Damage` / `Max<Type>Damage` (e.g. "more maximum Lightning
    // Damage", "less minimum Physical Attack Damage"), scaling only one end
    // of the range. These ModNames encode the type in the name itself and
    // carry no DamageType tag, matched by name against cfg (which carries
    // skill flags); with no such mod, more returns 1.0 and output is unchanged.
    let prefix = type_prefix(comp.damage_type);
    let more_min = db.more(cfg, &[ModName::from(format!("Min{prefix}Damage"))]);
    let more_max = db.more(cfg, &[ModName::from(format!("Max{prefix}Damage"))]);
    DamageComponent {
        min: round(comp.min * scale * more_min),
        max: round(comp.max * scale * more_max),
        ..comp
    }
}

/// Aggregates inc (summed) and more (product) across a set of type paths.
///
/// Each path type contributes `<Type>Damage` (a type-scoped cfg matches its
/// `DamageType` tag); elemental types additionally contribute the shared
/// `ElementalDamage` (deduplicated, counted once even with multiple
/// elements). The generic `AttackDamage`/`Damage` is only counted once (independent of type-scoping).
pub(crate) fn aggregate_inc_more(
    db: &ModDb,
    cfg: &CalcConfig,
    final_type: DamageType,
) -> (f64, f64) {
    // The generic bucket (not restricted to a damage type): `Damage` always;
    // attack/spell/skill categories (projectile/area/melee) per the cfg flag
    // -- so `increased <Attack|Spell|Projectile|Area|Melee> Damage` applies to this skill.
    let mut generic_names = vec![ModName::from("Damage")];
    for (flag, name) in [
        (ModFlags::ATTACK, "AttackDamage"),
        (ModFlags::SPELL, "SpellDamage"),
        (ModFlags::PROJECTILE, "ProjectileDamage"),
        (ModFlags::AREA, "AreaDamage"),
        (ModFlags::MELEE, "MeleeDamage"),
    ] {
        if cfg.flags.intersects(flag) {
            generic_names.push(ModName::from(name));
        }
    }
    // Damage-scaling names derived from keywords / weapon categories (GrenadeDamage / CrossbowDamage, etc.).
    for kw in &cfg.damage_keywords {
        generic_names.push(ModName::from(kw.clone()));
    }
    let elemental_name = ModName::from("ElementalDamage");

    // The generic bucket is only counted once (not restricted to a damage type).
    let mut inc = db.sum(ModType::Inc, cfg, &generic_names);
    let mut more = db.more(cfg, &generic_names);
    if dbg_env!("POBR_DBG_BASES").is_some() && final_type == DamageType::Physical {
        eprintln!("[POBR_POOL_NAMES] {generic_names:?}");
        for c in db.contributions(ModType::Inc, cfg, &generic_names) {
            eprintln!(
                "[POBR_POOL_SRC] {:?} {} src={:?}",
                c.name, c.value, c.raw_text
            );
        }
        for c in db.contributions(ModType::More, cfg, &generic_names) {
            eprintln!(
                "[POBR_POOL_MORE] {:?} {} src={:?}",
                c.name, c.value, c.raw_text
            );
        }
    }

    // PoB2-PoE2: typed inc/more only aggregates by the component's **final
    // damage type**, never stacking the conversion-source type (PoE1's
    // "double-dip via conversion-source increased" has been removed in PoE2
    // -- verified per-component against a PoB2 headless oracle: `calcDamage`'s
    // typeFlags only contains the final damageType). Converted/gain-as-extra
    // output follows the same semantics. Must use the component's final
    // `damage_type` (not the last entry of type_path) -- type_path is
    // sorted in chain order, so the last entry may not be the component's
    // own type (e.g. a Cold component with path=[Physical,Lightning,Cold,Fire]
    // has Fire as its last entry).
    let type_cfg = cfg.clone().with_damage_type(final_type);
    let type_name = [ModName::from(format!("{}Damage", type_prefix(final_type)))];
    inc += db.sum(ModType::Inc, &type_cfg, &type_name);
    more *= db.more(&type_cfg, &type_name);

    if final_type.is_elemental() {
        let elem = [elemental_name];
        inc += db.sum(ModType::Inc, &type_cfg, &elem);
        more *= db.more(&type_cfg, &elem);
    }
    (inc, more)
}

/// A set of damage conversion / gain-as-extra rules (read from the
/// [`ModDb`], mirroring PoB2's `processDamageConversion` / `buildGainTable`).
///
/// - `convert[from]`: normalized `to → fraction` (the sum for a single source is ≤ 1).
/// - `gain[from]`: gain-as-extra's `to → fraction` (not normalized, doesn't deduct from the source).
#[derive(Debug, Clone)]
pub struct ConversionRules {
    /// `convert[from][to] = fraction` (already normalized per-source to ≤ 1).
    pub convert: [[f64; 5]; 5],
    /// `convert_path[from][to]` = the set of intermediate types this
    /// conversion flowed through (excluding from / to themselves).
    /// Used to accumulate every type along a chained conversion into `type_path` (double-dip).
    pub convert_path: Vec<Vec<Vec<DamageType>>>,
    /// `gain[from][to] = fraction` (gain-as-extra, doesn't participate in normalization).
    pub gain: [[f64; 5]; 5],
}

impl Default for ConversionRules {
    fn default() -> Self {
        let n = DAMAGE_TYPES.len();
        Self {
            convert: [[0.0; 5]; 5],
            convert_path: vec![vec![Vec::new(); n]; n],
            gain: [[0.0; 5]; 5],
        }
    }
}

impl ConversionRules {
    /// Reads skill + global conversion + gain-as-extra rules from the [`ModDb`].
    ///
    /// Conversion has two stages (PoB2's `processDamageConversion`, skill
    /// before global, each normalized within its own source): after skill
    /// conversion deducts from the source, global conversion applies to both
    /// the "unconverted remainder" **and** "what skill conversion already
    /// converted out". To stay a pure function + deterministic, this folds
    /// both stages into a single `convert[from][to]` matrix (equivalent to
    /// PoB2's folded `conversionTable`), recording the chained types along the way in `convert_path`.
    pub fn from_mod_db(db: &ModDb, cfg: &CalcConfig) -> Self {
        let mut rules = Self::default();
        // First computes skill/global conversion ratios independently per type (normalized within each source).
        let skill = build_conversion_matrix(db, cfg, true);
        let global = build_conversion_matrix(db, cfg, false);
        let (convert, convert_path) = fold_conversion_stages(&skill, &global);
        rules.convert = convert;
        rules.convert_path = convert_path;
        rules.gain = build_gain_matrix(db, cfg);
        rules
    }

    fn is_empty(&self) -> bool {
        let any = |m: &[[f64; 5]; 5]| m.iter().flatten().any(|v| *v != 0.0);
        !any(&self.convert) && !any(&self.gain)
    }
}

/// Reads a single-stage (skill or global) conversion ratio matrix
/// `m[from][to]`, normalizing to 100% when a source type's total exceeds 100%.
///
/// Source: PoB2 `processDamageConversion` (`SkillDamageConvertTo<To>` /
/// `<From>DamageConvertTo<To>` / `ElementalDamageConvertTo<To>` / `NonChaosDamageConvertTo<To>`).
fn build_conversion_matrix(db: &ModDb, cfg: &CalcConfig, skill: bool) -> [[f64; 5]; 5] {
    let mut matrix = [[0.0_f64; 5]; 5];
    for (fi, &from) in DAMAGE_TYPES.iter().enumerate() {
        let from_prefix = type_prefix(from);
        let mut total = 0.0;
        for (ti, &to) in DAMAGE_TYPES.iter().enumerate() {
            let to_prefix = type_prefix(to);
            let mut names = Vec::new();
            if skill {
                names.push(ModName::from(format!("SkillDamageConvertTo{to_prefix}")));
                names.push(ModName::from(format!(
                    "Skill{from_prefix}DamageConvertTo{to_prefix}"
                )));
            } else {
                names.push(ModName::from(format!("DamageConvertTo{to_prefix}")));
                names.push(ModName::from(format!(
                    "{from_prefix}DamageConvertTo{to_prefix}"
                )));
                if from.is_elemental() {
                    names.push(ModName::from(format!(
                        "ElementalDamageConvertTo{to_prefix}"
                    )));
                }
                if from != DamageType::Chaos {
                    names.push(ModName::from(format!("NonChaosDamageConvertTo{to_prefix}")));
                }
            }
            let pct = db.sum(ModType::Base, cfg, &names).max(0.0);
            matrix[fi][ti] = pct / 100.0;
            total += pct;
        }
        // Normalized within the source when >100% (PoB2: factor = 100 / total).
        if total > 100.0 {
            let factor = 100.0 / total;
            for cell in matrix[fi].iter_mut() {
                *cell *= factor;
            }
        }
    }
    matrix
}

/// Reads the gain-as-extra ratio matrix `g[from][to]` (not normalized, doesn't deduct from the source).
///
/// Source: PoB2 `buildGainTable` (`DamageGainAs<To>` / `<From>DamageGainAs<To>` /
/// `ElementalDamageGainAs<To>` / `NonChaosDamageGainAs<To>` / `Skill*DamageGainAs<To>`).
fn build_gain_matrix(db: &ModDb, cfg: &CalcConfig) -> [[f64; 5]; 5] {
    let mut matrix = [[0.0_f64; 5]; 5];
    for (fi, &from) in DAMAGE_TYPES.iter().enumerate() {
        let from_prefix = type_prefix(from);
        for (ti, &to) in DAMAGE_TYPES.iter().enumerate() {
            let to_prefix = type_prefix(to);
            let mut names = vec![
                ModName::from(format!("DamageGainAs{to_prefix}")),
                ModName::from(format!("{from_prefix}DamageGainAs{to_prefix}")),
                ModName::from(format!("SkillDamageGainAs{to_prefix}")),
                ModName::from(format!("Skill{from_prefix}DamageGainAs{to_prefix}")),
            ];
            if from.is_elemental() {
                names.push(ModName::from(format!("ElementalDamageGainAs{to_prefix}")));
                names.push(ModName::from(format!(
                    "SkillElementalDamageGainAs{to_prefix}"
                )));
            }
            if from != DamageType::Chaos {
                names.push(ModName::from(format!("NonChaosDamageGainAs{to_prefix}")));
                names.push(ModName::from(format!(
                    "SkillNonChaosDamageGainAs{to_prefix}"
                )));
            }
            let pct = db.sum(ModType::Base, cfg, &names).max(0.0);
            matrix[fi][ti] = pct / 100.0;
        }
    }
    // The random-element tier (vendor CalcOffence.lua:1175-1200):
    // `DamageGainAsRandom` expands into the three elements per physMode --
    // PoBR folds this into the AVERAGE tier (vendor's configInput.physMode
    // defaults to "AVERAGE"): n/3 per element. `DamageGainAsRandom` applies
    // to every source type (vendor expands it into `DamageGainAs<Elem>`,
    // summed by buildGainTable for every from row); `PhysicalDamageGainAsRandom`
    // only applies to the physical source row (expanded into `PhysicalDamageGainAs<Elem>`).
    let generic_random = db
        .sum(ModType::Base, cfg, &[ModName::from("DamageGainAsRandom")])
        .max(0.0);
    let phys_random = db
        .sum(
            ModType::Base,
            cfg,
            &[ModName::from("PhysicalDamageGainAsRandom")],
        )
        .max(0.0);
    if generic_random > 0.0 || phys_random > 0.0 {
        for (fi, &from) in DAMAGE_TYPES.iter().enumerate() {
            let pct = generic_random
                + if from == DamageType::Physical {
                    phys_random
                } else {
                    0.0
                };
            if pct == 0.0 {
                continue;
            }
            for (ti, &to) in DAMAGE_TYPES.iter().enumerate() {
                if to.is_elemental() {
                    matrix[fi][ti] += pct / 3.0 / 100.0;
                }
            }
        }
    }
    matrix
}

/// Folds the skill conversion and global conversion stages into a single
/// `convert[from][to]` matrix + the intermediate types `convert_path`.
///
/// Mirrors PoB2's `conversionTable` two-step semantics:
/// 1. Skill conversion: source `from` retains `skill_mult = 1 - Σskill[from][*]`; converts out `skill[from][to]`.
/// 2. Global conversion: applies to both "the retained portion of `from`"
///    and "each intermediate type skill conversion produced". Each type
///    that's acted on retains `1 - Σglobal[mid][*]`, and the rest is
///    redistributed to targets per `global[mid][to]`.
///
/// Returns `(convert, convert_path)`:
/// - `convert[from][to]` = "the total ratio of `from`'s base that ends up at `to`" (including retention to itself).
/// - `convert_path[from][to]` = the set of intermediate types this from→to
///   flow passed through (excluding from / to), used to accumulate
///   double-dip increased along the way (e.g. the fire from Phys→Cold→Fire carries a Cold tag).
#[allow(clippy::type_complexity)]
fn fold_conversion_stages(
    skill: &[[f64; 5]; 5],
    global: &[[f64; 5]; 5],
) -> ([[f64; 5]; 5], Vec<Vec<Vec<DamageType>>>) {
    let n = DAMAGE_TYPES.len();
    let mut result = [[0.0_f64; 5]; 5];
    let mut path: Vec<Vec<Vec<DamageType>>> = vec![vec![Vec::new(); n]; n];

    for from in 0..n {
        // After skill conversion, the ratio each "intermediate type" carries from `from`.
        let mut intermediate = [0.0_f64; 5];
        let skill_total: f64 = skill[from].iter().sum();
        let skill_mult = (1.0 - skill_total).max(0.0);
        intermediate[from] += skill_mult; // retained to itself
        for to in 0..n {
            if skill[from][to] > 0.0 {
                intermediate[to] += skill[from][to];
            }
        }

        // Global conversion applies to every intermediate type.
        for (mid, &carried) in intermediate.iter().enumerate() {
            if carried == 0.0 {
                continue;
            }
            let global_total: f64 = global[mid].iter().sum();
            let global_mult = (1.0 - global_total).max(0.0);
            // The intermediate type's retained portion still lands at mid (mid is the target, not a passthrough intermediate type here).
            if global_mult > 0.0 {
                result[from][mid] += carried * global_mult;
            }
            // The globally converted-out portion lands at each target (mid is an intermediate type along the way).
            for to in 0..n {
                if global[mid][to] > 0.0 {
                    result[from][to] += carried * global[mid][to];
                    record_mid(&mut path[from][to], mid, from, to);
                }
            }
        }
    }
    (result, path)
}

/// Records intermediate type `mid` into the `from→to` pass-through set (excluding from and to themselves).
fn record_mid(slot: &mut Vec<DamageType>, mid: usize, from: usize, to: usize) {
    if mid != from && mid != to {
        push_unique(slot, DAMAGE_TYPES[mid]);
    }
}

/// Runs the conversion chain: reorganizes each type's flat base per
/// [`ConversionRules`] into post-conversion components carrying `type_path`.
///
/// For each target type `to`:
/// - Accumulates `base[from] * convert[from][to]` from every source `from`
///   (including the from==to retention), and accumulates every contributing
///   `from` into that component's `type_path` (key to double-dip).
/// - gain-as-extra: based on each intermediate type's **post-conversion**
///   amount, adds `* gain[mid][to]`, likewise accumulated into `type_path`
///   (a double dip of source + target), but without deducting from the source.
///
/// Source: PoB2 `calcConvertedDamage` + `calcGainedDamage` (gain is based on post-conversion damage).
fn apply_conversion_chain(
    base: &[(DamageType, f64, f64)],
    rules: &ConversionRules,
) -> Vec<DamageComponent> {
    let n = DAMAGE_TYPES.len();
    // Indexes base.
    let base_min: Vec<f64> = base.iter().map(|b| b.1).collect();
    let base_max: Vec<f64> = base.iter().map(|b| b.2).collect();

    // Each type's post-conversion min/max, plus its type_path source set.
    let mut conv_min = vec![0.0_f64; n];
    let mut conv_max = vec![0.0_f64; n];
    let mut paths: Vec<Vec<DamageType>> = vec![Vec::new(); n];

    for to in 0..n {
        for from in 0..n {
            let frac = rules.convert[from][to];
            if frac <= 0.0 {
                continue;
            }
            let add_min = base_min[from] * frac;
            let add_max = base_max[from] * frac;
            if add_min == 0.0 && add_max == 0.0 {
                continue;
            }
            conv_min[to] += add_min;
            conv_max[to] += add_max;
            push_unique(&mut paths[to], DAMAGE_TYPES[from]);
            // Intermediate types along a chained conversion are also accumulated into type_path (double-dip).
            for ty in &rules.convert_path[from][to] {
                push_unique(&mut paths[to], *ty);
            }
        }
    }

    // gain-as-extra: adds based on the post-conversion intermediate type amount.
    let mut gain_min = vec![0.0_f64; n];
    let mut gain_max = vec![0.0_f64; n];
    let mut gain_paths: Vec<Vec<DamageType>> = vec![Vec::new(); n];
    for to in 0..n {
        for mid in 0..n {
            let frac = rules.gain[mid][to];
            if frac <= 0.0 {
                continue;
            }
            // The gain source is always the post-conversion intermediate
            // type amount (diagonal retention + converted-in amount),
            // mirroring PoB2's calcGainedDamage: source amount =
            // MinBase*conversionTable[mid].mult + convertedMin.
            // convert[mid][mid] already includes the retention mult, so
            // conv_min[mid] == base_min[mid] when there's pure gain with no
            // conversion; when mid is 100% converted away with nothing
            // converted in, the source amount is 0 -- **the original base is not fallen back to**.
            // (The old fallback would conjure a gain out of the raw fire
            // base in a fire 100%→lightning scenario with nothing to draw
            // from -- one root cause of deadeye's hit_avg being overestimated
            // by +13%; verified against vendor's numbers that
            // FireSummedBase=basis×gain's basis excludes the converted-away
            // fire. Landed per prescription 04-offence-core 04-02.)
            let add_min = conv_min[mid] * frac;
            let add_max = conv_max[mid] * frac;
            if add_min == 0.0 && add_max == 0.0 {
                continue;
            }
            gain_min[to] += add_min;
            gain_max[to] += add_max;
            // A gain component's type_path includes the intermediate type's pass-through set plus the target type.
            for ty in &paths[mid] {
                push_unique(&mut gain_paths[to], *ty);
            }
            push_unique(&mut gain_paths[to], DAMAGE_TYPES[mid]);
        }
    }

    // Assembles the final components.
    let mut result = Vec::with_capacity(n);
    for to in 0..n {
        let damage_type = DAMAGE_TYPES[to];
        let min = round(conv_min[to] + gain_min[to]);
        let max = round(conv_max[to] + gain_max[to]);
        // Merges the conversion + gain type_path.
        let mut path = paths[to].clone();
        for ty in &gain_paths[to] {
            push_unique(&mut path, *ty);
        }
        push_unique(&mut path, damage_type);
        path.sort_by_key(|ty| type_order_index(*ty));

        result.push(DamageComponent {
            damage_type,
            min,
            max,
            kind: DamageKind::Hit,
            source: DamageSource::Attack,
            type_path: path,
        });
    }
    result
}

/// Adds `ty` to `path` (deduplicated).
fn push_unique(path: &mut Vec<DamageType>, ty: DamageType) {
    if !path.contains(&ty) {
        path.push(ty);
    }
}

/// Damage conversion / extra gain / double-dip helpers (08-mechanics §2.2, damage-defence-order §2.2).
///
/// These are a **pure function** layer operating on an already-aggregated
/// [`DamageComponent`] vector, reused by tests / simple upper-layer
/// conversion scenarios. See [`calculate_components`] → [`apply_conversion_chain`] for the full pipeline.
///
/// **Converts** a fraction of the `from` type into the `to` type (source decreases, target increases).
pub fn convert_damage(
    components: &[DamageComponent],
    from: DamageType,
    to: DamageType,
    fraction: f64,
) -> Vec<DamageComponent> {
    apply_shift(components, from, to, fraction, true)
}

/// Adds a fraction of the `from` type as **extra** damage to the `to` type (source doesn't decrease).
pub fn gain_as_extra(
    components: &[DamageComponent],
    from: DamageType,
    to: DamageType,
    fraction: f64,
) -> Vec<DamageComponent> {
    apply_shift(components, from, to, fraction, false)
}

/// Normalizes the sum of multiple conversion fractions to <= 1.0 (PoB / PoE2: proportionally scaled when total conversion exceeds 100%).
pub fn normalize_conversion(fractions: &[f64]) -> Vec<f64> {
    let total: f64 = fractions.iter().filter(|f| **f > 0.0).sum();
    if total <= 1.0 {
        return fractions.to_vec();
    }
    fractions.iter().map(|f| f / total).collect()
}

/// Sums damage components by damage type (avg); used as the source for ailment magnitude double-dip.
pub fn sum_avg(components: &[DamageComponent]) -> f64 {
    components.iter().map(DamageComponent::avg).sum()
}

/// Gets a type's component (min, max), or (0, 0) if absent.
fn type_range(components: &[DamageComponent], damage_type: DamageType) -> (f64, f64) {
    components
        .iter()
        .find(|component| component.damage_type == damage_type)
        .map_or((0.0, 0.0), |component| (component.min, component.max))
}

/// Shared conversion / extra gain implementation. `remove_from_source` distinguishes convert (true) from gain (false).
///
/// The target component accumulates `from` into its `type_path` (double-dip), preserving its existing kind/source.
fn apply_shift(
    components: &[DamageComponent],
    from: DamageType,
    to: DamageType,
    fraction: f64,
    remove_from_source: bool,
) -> Vec<DamageComponent> {
    let fraction = fraction.clamp(0.0, 1.0);
    if fraction == 0.0 || from == to {
        return components.to_vec();
    }
    let (from_min, from_max) = type_range(components, from);
    let shift_min = from_min * fraction;
    let shift_max = from_max * fraction;

    let mut result: Vec<DamageComponent> = components.to_vec();
    let mut target_idx: Option<usize> = None;
    for (idx, component) in result.iter_mut().enumerate() {
        if component.damage_type == from && remove_from_source {
            component.min = round(component.min - shift_min);
            component.max = round(component.max - shift_max);
        }
        if component.damage_type == to {
            component.min = round(component.min + shift_min);
            component.max = round(component.max + shift_max);
            push_unique(&mut component.type_path, from);
            component.type_path.sort_by_key(|ty| type_order_index(*ty));
            target_idx = Some(idx);
        }
    }
    if target_idx.is_none() && (shift_min != 0.0 || shift_max != 0.0) {
        let comp =
            DamageComponent::new(to, round(shift_min), round(shift_max)).with_type_path([from, to]);
        result.push(comp);
    }
    result
}
