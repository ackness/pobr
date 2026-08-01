//! Skill gem / granted-effect domain schema (`base/skill_gems.json` /
//! `base/granted_effects.json` / `base/granted_effect_levels.json` /
//! `base/granted_effect_stat_sets.json` / `base/cost_types.json`, sourced from
//! `SkillGems.dat` / `GrantedEffects*` / `CostTypes.dat`).

use serde::{Deserialize, Serialize};

/// A skill gem definition (from `SkillGems.dat` plus `BaseItemTypes`
/// foreign-key resolution).
///
/// Gems have **no own Id column**; their identity comes from the base id their
/// `BaseItemType` points to (e.g. `Metadata/Items/Gems/SkillGemFireball`).
/// `name` lives in the base_items domain — this struct only holds fields
/// relevant to gem mechanics.
///
/// The gem → granted-effect edge (`granted_effect_id` /
/// `additional_granted_effect_ids`) is absent from the base artifact: the
/// `GemEffects` table's bundle isn't downloadable at the pinned patch (see
/// `pipeline/config.json`'s `_tablesUnavailableForPinnedPatch`).
/// `pobr-gamedata` merges it in at load time from `overlay/gem_effects.json`
/// ([`GemEffectDef`], extracted from vendor `Data/Gems.lua` via extract-lua);
/// the adapter always writes it empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillGemDef {
    /// Stable id, taken from the `Id` of the underlying `BaseItemType`.
    pub id: String,
    /// Gem type (raw GGG enum: 0 = active skill, 1 = support). Kept as the raw
    /// value for easier cross-checking.
    pub gem_type: Option<u32>,
    /// Gem colour (raw GGG enum: 1 = red/strength, 2 = green/dexterity,
    /// 3 = blue/intelligence, 4 = white, etc.).
    pub gem_colour: Option<u32>,
    /// Minimum character level required to use.
    pub min_level_req: u32,
    /// Strength requirement percentage (attribute-requirement weight).
    pub str_pct: u32,
    /// Dexterity requirement percentage.
    pub dex_pct: u32,
    /// Intelligence requirement percentage.
    pub int_pct: u32,
    /// Whether this is a support gem (determined by `GemType == 1`).
    pub is_support: bool,
    /// Id of the gem's primary granted effect (`GemEffects.GrantedEffect` →
    /// `GrantedEffects.Id`, e.g. `IceNovaPlayer`). Sourced by merging
    /// `overlay/gem_effects.json` at load time; always `None` in the base
    /// artifact / old data packs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub granted_effect_id: Option<String>,
    /// Ids of effects the gem **additionally** grants
    /// (`GemEffects.AdditionalGrantedEffects` column sequence, e.g.
    /// InfernalCry → `InfernalCryCorpseExplosionPlayer`). Corresponds to
    /// PoB2's `additionalGrantedEffectId1..N` (`Export/Scripts/skills.lua:919-923`),
    /// the foreign key for meta/compound gem expansion (T5.6). Sourced the
    /// same way as `granted_effect_id`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_granted_effect_ids: Vec<String>,
}

/// A granted-effect definition (from `GrantedEffects.dat` plus foreign-key
/// resolution).
///
/// Each gem/item ultimately grants one or more `GrantedEffect`s; an
/// active-skill effect links to an `ActiveSkills` record (display name /
/// skill types). This slice covers identity + active-skill link + cast time +
/// the support-eligibility columns (require/add/exclude type expressions plus
/// the cannot_be_supported/support_gems_only booleans) + the
/// StatSet/CostTypes foreign-key indices.
///
/// Per-level parameters (cost / cooldown / attack time) live in the separate
/// [`SkillLevelDef`] domain (`granted_effect_levels.json`), keyed by this
/// `id`.
///
/// Per-level **damage stat values** live in the separate [`SkillStatSetDef`]
/// domain (`granted_effect_stat_sets.json`), also keyed by this `id` (the
/// adapter has already resolved the `stat_set` foreign key via join).
/// `stat_set` itself is kept as the raw index for reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantedEffectDef {
    /// Stable id, i.e. `GrantedEffects.Id` (e.g. `FireballPlayer`).
    pub id: String,
    /// Whether this is a support effect.
    pub is_support: bool,
    /// Linked active-skill id (resolves `ActiveSkills.Id`; `None` for support
    /// effects).
    pub active_skill: Option<String>,
    /// Cast/channel time in milliseconds. A raw value of 0 (instant/support)
    /// is normalized to `None`.
    pub cast_time: Option<u32>,
    /// The support-eligibility **require** postfix expression token stream
    /// (`AllowedActiveSkillTypes` column, FK → `ActiveSkillType.Id` names;
    /// `"AND"/"OR"/"NOT"` are special rows in that table, order preserved).
    /// Empty means unrestricted (matches any active skill). Evaluation
    /// semantics: PoB2 `CalcTools.lua::doesTypeExpressionMatch` (postfix
    /// stack machine — any true value on the stack is a match).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub require_skill_types: Vec<String>,
    /// Type tokens a support **merges into** the active skill
    /// (`AddedActiveSkillTypes` column; a plain list, not an expression).
    /// Corresponds to PoB2's `addSkillTypes`, which participates in the
    /// support-eligibility fixed point (`CalcActiveSkill.lua:179-210`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add_skill_types: Vec<String>,
    /// The support-eligibility **exclude** postfix expression token stream
    /// (`ExcludedActiveSkillTypes` column, same token encoding as require).
    /// Matching the expression rejects support.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_skill_types: Vec<String>,
    /// Active effect **cannot be supported by any support**
    /// (`CannotBeSupported` column). Corresponds to PoB2's
    /// `grantedEffect.cannotBeSupported` (first stage of the eligibility
    /// check).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cannot_be_supported: bool,
    /// This support **can only support gem-granted** skills
    /// (`SupportsGemsOnly` column). Corresponds to PoB2's
    /// `grantedEffect.supportGemsOnly` (skills without gemData are rejected,
    /// second stage of the four-stage check).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub support_gems_only: bool,
    /// Foreign-key index into the `GrantedEffectStatSets` table (raw
    /// `StatSet` column). Per-level damage stat values resolve through this
    /// (pending that table's download); currently kept as an index for
    /// reference only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stat_set: Option<u32>,
    /// Stable ids of **additional** statSets (`AdditionalStatSets` column,
    /// FK → `GrantedEffectStatSets.Id` — corrected during W0 verification:
    /// the FK target is the statSet table, not another `GrantedEffects` row;
    /// column order preserved). These are skill-form variants (e.g. IceNova
    /// → `IceNovaPlayerOnFrostbolt` / `IceNovaColdInfusedPlayer`),
    /// corresponding 1:1 with `sets[1..]` of [`SkillStatSetDef`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_stat_set_ids: Vec<String>,
    /// Foreign-key index into cost types (raw `CostTypes` column, e.g. `[0]`
    /// = mana). Pairs positionally with [`SkillLevelDef::cost_amounts`] (the
    /// i-th type costs the i-th amount).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cost_types: Vec<u32>,
    /// Type names of the linked active skill
    /// (`ActiveSkills.ActiveSkillTypes` resolved through
    /// `ActiveSkillType.Id`, e.g. `["Attack","Projectile","Damage"]` /
    /// `["Spell","Area"]`). Used to determine attack/spell status (an attack
    /// skill's hit damage comes from the weapon base).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_types: Vec<String>,
    /// Ids of minions this skill summons (from PoB2 Export template's
    /// hand-written `#minionList` directive, not in the `.dat` files).
    /// **Merged in-memory shape**: the base `granted_effects.json` doesn't
    /// have this field; `pobr-gamedata` merges it in at load time from
    /// `overlay/granted_effect_minions.json`
    /// ([`crate::catalog::actors::GrantedEffectMinionDef`]) by `id`. Empty
    /// means not a summon skill (backward compatible). Source: vendor
    /// `Export/Scripts/skills.lua:771-776`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub minion_list: Vec<String>,
    /// Minion ids a support adds (`addMinionList`, merged the same way as
    /// `minion_list`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add_minion_list: Vec<String>,
    /// Player equipment slots the minion borrows from (keys with value
    /// `true` in `minionUses`, merged the same way).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub minion_uses: Vec<String>,
    /// Whether the minion uses its own separate item set
    /// (`minionHasItemSet`, merged the same way).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub minion_has_item_set: bool,
}

impl GrantedEffectDef {
    /// Whether this is an attack skill (type names include `Attack`) — an
    /// attack skill's hit damage comes from the weapon base.
    pub fn is_attack(&self) -> bool {
        self.skill_types.iter().any(|t| t == "Attack")
    }

    /// Whether this is a spell skill (type names include `Spell`) — spells
    /// don't use weapon damage.
    pub fn is_spell(&self) -> bool {
        self.skill_types.iter().any(|t| t == "Spell")
    }

    /// Whether this is a **non-weapon attack** (type names include
    /// `NonWeaponAttack`, e.g. Shield Wall) — the hit's base damage comes
    /// from the skill itself (an off-hand stat-set), not the main-hand
    /// weapon base. Corresponds to PoB2's `skillFlags.shieldAttack`/
    /// `NonWeaponAttack`: the source isn't `weaponData1` but is decided by a
    /// `setOffHand*` skill stat.
    pub fn is_non_weapon_attack(&self) -> bool {
        self.skill_types.iter().any(|t| t == "NonWeaponAttack")
    }
}

/// Per-level parameters for a granted effect (from
/// `GrantedEffectsPerLevel.dat`).
///
/// Kept as a domain separate from [`GrantedEffectDef`] (one effect can have
/// dozens of level rows, and folding them into the main table would bloat
/// it). Collected in `granted_effect_levels.json`, keyed by `GrantedEffect`
/// id into an ascending array.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillLevelDef {
    /// Gem/skill level (1-based).
    pub level: u32,
    /// Cooldown in milliseconds. A raw 0 (no cooldown) normalizes to `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_ms: Option<u32>,
    /// Attack time in milliseconds, for attack-type skills. A raw 0 (not an
    /// attack / determined by the weapon) normalizes to `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack_time_ms: Option<u32>,
    /// Cost amount for each cost type, paired positionally with
    /// [`GrantedEffectDef::cost_types`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cost_amounts: Vec<u32>,
    /// Attack speed multiplier (PoB's
    /// `GrantedEffectsPerLevel.attackSpeedMultiplier`, percentage points,
    /// can be negative). Applies to weapon attack rate:
    /// `AttackRate × (1 + attackSpeedMultiplier/100)` (e.g. Flicker is -50).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack_speed_multiplier: Option<f64>,
    /// Base skill damage multiplier (PoB's
    /// `GrantedEffectsPerLevel.baseMultiplier`, e.g. Flicker L13 = 1.99).
    /// Falls back to this when the stat-set's `BaseMultiplier` is missing,
    /// as [`SkillStatSetLevel::damage_multiplier`]'s fallback source (the
    /// two are synonymous — PoB stores both).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_multiplier: Option<f64>,
    /// Base crit chance for the skill (PoB's `critChance`, percentage
    /// points; e.g. Comet = 13.0 = 13%). Sourced from
    /// `GrantedEffectStatSetsPerLevel` (community schema column
    /// `SpellCritChance` = vendor `AttackCritChance` primary column `/100`;
    /// community `AttackCritChance` = vendor `OffhandCritChance`, overrides
    /// when nonzero; vendor `Export/Scripts/skills.lua:281-286`). The source
    /// of a spell/attack skill's inherent crit chance — an attack skill
    /// falls back to the weapon base crit when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crit_chance: Option<f64>,
    /// Support gem cost multiplier (PoB's `manaMultiplier` =
    /// `CostMultiplier - 100`, percentage points, can be negative; e.g.
    /// Heightened Curse is +30). `None` means the raw `CostMultiplier == 100`
    /// (no multiplier, matching the omission condition in vendor
    /// `Export/Scripts/skills.lua:262-264`). On the consumption side this
    /// injects a `SupportManaMultiplier` MORE into the supported skill
    /// (PoB2 `CalcActiveSkill.lua:689-691`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mana_multiplier: Option<f64>,
    /// Flat Spirit reservation (PoB's `spiritReservationFlat`, community
    /// `.dat` column name `Reservation`, raw value; vendor
    /// `skills.lua:244-246`). The Spirit reservation source for sustained
    /// effects (auras / always-on buffs). `None` means 0 (no reservation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spirit_reservation_flat: Option<f64>,
    /// Reservation multiplier (PoB's `reservationMultiplier` = raw value
    /// `- 100`, percentage points, can be negative; community `.dat` column
    /// name `EffectOnPlayer`; vendor `skills.lua:247-249`). `None` means the
    /// raw value == 100. On the consumption side this injects a
    /// `ReservationMultiplier` MORE (PoB2 `CalcActiveSkill.lua:692-694` on
    /// the support side / `:754-756` on the active side).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reservation_multiplier: Option<f64>,
    /// Number of stored uses (PoB's `storedUses`, raw value; vendor
    /// `skills.lua:277-279`). `None` means 0 (no storage).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stored_uses: Option<u32>,
    /// Level requirement (PoB's `levelRequirement`). **PoE2's `.dat` has no
    /// `PlayerLevelReq` column** — the real source is
    /// `SkillGems.ItemExperienceType → ItemExperiencePerLevel.PlayerLevel`
    /// (vendor `skills.lua:239-240`), and that table's bundle isn't
    /// downloadable at pinned patch 4.5.0.3.4 (see `pipeline/config.json`'s
    /// `_tablesUnavailableForPinnedPatch`). This is only a schema
    /// placeholder for now (the adapter always writes `None` and nothing
    /// consumes it); data reaches storage through the extract-lua fallback
    /// channel (from createMinionSkills' level selection).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level_requirement: Option<u32>,
}

/// **All the statSets** for a given granted effect (skill).
///
/// The primary set is the `GrantedEffectStatSets` row pointed to by
/// `GrantedEffects.StatSet`; additional sets are the rows pointed to, in
/// column order, by `GrantedEffects.AdditionalStatSets` (e.g. IceNova →
/// `IceNovaPlayerOnFrostbolt` / `IceNovaColdInfusedPlayer`, skill-form
/// variants). Additional sets are already **merged** with the primary set at
/// ingest time per vendor export semantics (constant/per-level stat
/// concatenation, effectiveness fallback — see [`StatSetDef`]), so consumers
/// just pick one and use it, no further merging needed.
///
/// Collected in `granted_effect_stat_sets.json`, keyed by
/// [`GrantedEffectDef::id`] (the adapter has already performed the join at
/// export time). This is the last leg of the "gem → skill damage → DPS" data
/// pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillStatSetDef {
    /// Granted effect id (e.g. `FireballPlayer`), aligned with
    /// [`GrantedEffectDef::id`].
    pub effect_id: String,
    /// The statSet list: `sets[0]` is always the primary set, followed by
    /// additional sets (in `AdditionalStatSets` column order). Consumers
    /// default to the primary set (PoB2's `<Gem statSetIndex>` defaults to
    /// 1, the first set).
    pub sets: Vec<StatSetDef>,
}

/// A single statSet (from `GrantedEffectStatSets` +
/// `GrantedEffectStatSetsPerLevel`).
///
/// The content of additional sets (`sets[1..]`) is already stored per PoB2's
/// export-script base-merge semantics (vendor
/// `Export/Scripts/skills.lua:498-553`):
/// - `constant_stats` = primary set's constants ++ this set's constants
///   (`:502-504`, tableConcat);
/// - `base_effectiveness`: falls back to the primary set when this set's raw
///   value is the default 1 (`:506-508`);
/// - per-level rows are paired with the primary set's rows **by array
///   position**, stat = primary row ++ this row (`:541-549`);
/// - `damage_multiplier`: takes this row when `BaseMultiplier ≠ 0`,
///   otherwise falls back to the paired primary row (`:533-541`; the
///   `UseSetAttackMulti` column isn't downloaded, but both branches end up
///   computing this row's `/10000+1`, so falling back to the primary row is
///   a conservative approximation when it's missing).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatSetDef {
    /// Stable statSet id (`GrantedEffectStatSets.Id`, e.g.
    /// `IceNovaColdInfusedPlayer`; the primary set's id is usually the same
    /// as the effect id).
    pub set_id: String,
    /// Form label text (e.g. `Cold-Infused`). Sourced from
    /// `overlay/stat_set_labels.json` (vendor-extracted, since the `.dat`
    /// `Label` column's FK target table `GrantedEffectLabels` isn't
    /// downloadable), merged at load time; `None` in the base artifact or
    /// when there's no vendor counterpart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// 1-based index in PoB2's exported statSets list (vendor template's
    /// `#set` ordering, the same index semantics as `<Gem statSetIndex>`).
    /// The vendor template curates and skips some sets (e.g.
    /// IceNovaPlayerOnFrostbolt), so this doesn't necessarily line up with
    /// the `sets` array's index. Sourced the same way as `label` (overlay
    /// merge); `None` means vendor didn't export this set (it can't be
    /// selected by statSetIndex).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor_set_index: Option<u32>,
    /// The stat-set's base effectiveness (`BaseEffectiveness`, kept for
    /// reference — per-level values are already the resolved final
    /// quantities).
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub base_effectiveness: f64,
    /// Level-independent constant stats (`ConstantStats` plus values; e.g. a
    /// support gem's `damage_+%_final` multiplier).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constant_stats: Vec<SkillDamageStat>,
    /// Inherent **attack speed MORE** from the statSet's `baseMods` (PoB2's
    /// `mod("Speed", "MORE", N, ModFlag.Attack)`, percentage points; e.g.
    /// Flicker Strike = 285). This is a constant baseMod built into PoB2,
    /// not present in the GGG `.dat` tables — extracted and merged from
    /// vendor Lua. Injected as an `AttackSpeed` MORE (only consumed on the
    /// attack-skill path). `None` means none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_attack_speed_more: Option<f64>,
    /// Skill DoT config booleans, from vendor skillData's `dotIs*` (statSet
    /// `baseMods` entries like `skill("dotIsArea", true)`; PoB2 strips the
    /// corresponding ModFlag bits from dotCfg based on these,
    /// `CalcOffence.lua:5831-5860`). No GGG `.dat` column for this — merged
    /// at load time via the extract-lua skill_overrides channel. Default
    /// all-false means conservatively don't strip any flag.
    #[serde(default, skip_serializing_if = "DotFlags::is_default")]
    pub dot_flags: DotFlags,
    /// Corpse-explosion gate, from vendor statSet `baseMods`'
    /// `skill("explodeCorpse", true)` (e.g. DetonateDeadPlayer,
    /// act_int.lua:5287). PoB2 uses this to inject
    /// `monsterLife × skillData.corpseExplosionLifeMultiplier` into physical
    /// base damage (`CalcOffence.lua:2211-2217`). No GGG `.dat` column for
    /// this — merged at load time via the extract-lua skill_overrides
    /// channel. Default false means nothing is injected.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub explode_corpse: bool,
    /// Implicit (no per-level values) stats: entries from vendor statSet's
    /// `stats` list where **no level row has a value** (= GGG `.dat`'s
    /// `GrantedEffectStatSets.ImplicitStats` column, not downloaded by the
    /// adapter). Vendor always consumes value 1 for these
    /// (`CalcTools.lua:152`, `statSetLevel[index] or 1`); mostly
    /// behavior-flag stats (e.g. Garukhan's Resolve's
    /// `attacks_roll_crits_twice` → statmap flag BifurcateCrit).
    ///
    /// Sourced from the extract-lua skill_overrides channel (`implicit_stat`
    /// entries, a **curated whitelist** — the vendor's full 4394 entries are
    /// mostly display/behavior noise, so only stats PoBR's calc actually
    /// consumes are extracted), merged at load time. Consumed by folding
    /// into the base segment with value 1 via `effect_stats`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implicit_stats: Vec<String>,
    /// Per-level stats (ascending by gem level; includes base damage values
    /// plus `damage_+%[_final]` scaling).
    pub levels: Vec<SkillStatSetLevel>,
}

/// Skill DoT config booleans (vendor skillData `dotIs*`) plus an extraction
/// verification marker.
///
/// Note that vendor's `dotIsSpell`/`dotIsProjectile`/
/// `doubleHitsWhenDualWielding` are **stat-driven** (`skill(...)` entries in
/// `SkillStatMap.lua`, already stored in `overlay/skill_stat_map.json`'s
/// `skill_data` kind) and don't go through this struct; this struct only
/// carries booleans attached directly to statSet `baseMods` (in the full
/// 4.5.0.3.4 vendor data there's only one: `dotIsArea` on TornadoShotPlayer's
/// "Tornado" statSet).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DotFlags {
    /// dotIsArea: DoT keeps the Area flag (false means dotCfg strips the
    /// Area bit).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub area: bool,
    /// dotIsProjectile.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub projectile: bool,
    /// dotIsSpell.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub spell: bool,
    /// dotIsAttack.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub attack: bool,
    /// dotIsHit.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hit: bool,
    /// Whether this went through vendor-extraction verification (true means
    /// the overlay has a `dotIs*` entry for this statSet; false means an
    /// unverified conservative default — flagged as `verified:false`
    /// metadata and listed separately in the parity report).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub verified: bool,
}

impl DotFlags {
    /// Whether all fields are at their default (a serde skip predicate: an
    /// all-false struct isn't written out at all, keeping diffs clean).
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Damage stats for a granted effect at a given gem level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillStatSetLevel {
    /// Gem level (1-based, aligned with [`SkillLevelDef::level`]).
    pub gem_level: u32,
    /// Skill damage multiplier (PoB's `baseMultiplier` =
    /// `1 + GrantedEffectStatSetsPerLevel.BaseMultiplier/10000`). Attack
    /// skills use this to scale up weapon + added damage (e.g. grenade L18
    /// = 7.57 → 757% weapon damage); `1.0` means no multiplier (most
    /// spells).
    #[serde(default = "one_f64", skip_serializing_if = "is_one_f64")]
    pub damage_multiplier: f64,
    /// Resolved damage stats at this level (stat id → value).
    pub stats: Vec<SkillDamageStat>,
}

fn one_f64() -> f64 {
    1.0
}

fn is_one_f64(v: &f64) -> bool {
    *v == 1.0
}

/// A single resolved damage stat (stable stat id + resolved numeric value).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillDamageStat {
    /// Stable stat id (e.g. `spell_minimum_base_fire_damage`).
    pub stat: String,
    /// The resolved value at this gem level (`BaseResolvedValues` /
    /// `AdditionalStatsValues`).
    pub value: f64,
}

/// serde predicate to skip a zero f64 (keeps diffs clean).
fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

// Gem quality stat domain (`overlay/gem_quality_stats.json`)

/// A single gem-quality stat slope.
///
/// Sourced from PoB2's exported `Data/Skills/*.lua` `qualityStats` field (raw
/// `.dat` is `GrantedEffectQualityStats.StatValues / 1000`, see vendor
/// `Export/Scripts/skills.lua:304-313`; that table's bundle isn't
/// downloadable at the currently pinned patch, so it goes through the
/// extract-lua channel — see `pipeline/config.json`'s
/// `_tablesUnavailableForPinnedPatch`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityStat {
    /// Stable stat id (e.g. `base_spell_%_chance_to_echo`).
    pub stat: String,
    /// Slope per 1 point of quality. Consumers add `trunc(rate × quality)`
    /// into the skill's stat set — **truncated toward zero**, matching
    /// PoB2's `CalcTools.lua:142`, `math.modf(stat[2] * skillInstance.quality)`.
    pub per_quality_rate: f64,
    /// Alt-quality stat (vendor `altQualityStats`): only added when the
    /// build has the GemlingQuality flag (the Gemling ascendancy's "Gem
    /// Quality grants Socketed Skills an additional effect") set (PoB2
    /// `CalcTools.lua:147-152`, `includeAltQualityStats`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub alt: bool,
}

/// serde predicate to skip a false bool (keeps diffs clean; old data without
/// the `alt` key means false).
fn is_false(v: &bool) -> bool {
    !*v
}

/// Quality-stat table for a granted effect (a single entry of
/// `overlay/gem_quality_stats.json`).
///
/// Support-gem effects aren't in this table (PoB2's export condition
/// `not (skillGem and granted.IsSupport)` already applies on the vendor data
/// side; extraction is a faithful transcription).
///
/// TODO (pending restoration of the `.dat` table channel):
/// `GrantedEffectQualityStats`'s Alt columns
/// (`AltStats`/`AltStatValuesPermille`/`AltApplyToStatSets`/`ApplyToStatSets`)
/// are stored as-is but not consumed — PoB2's export also only reads the
/// primary columns, so behavior is aligned; the semantics are deferred.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GemQualityStatDef {
    /// Granted effect id (aligned with [`GrantedEffectDef::id`], e.g.
    /// `CometPlayer`).
    pub effect_id: String,
    /// Quality-stat slopes (keeps vendor export order; multiple entries for
    /// the same stat add together).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stats: Vec<QualityStat>,
}

/// Top level of `overlay/gem_quality_stats.json` (from the consumer's
/// perspective: the `_meta` provenance header is ignored by default serde;
/// consumers just take the `effects` list, ascending by `effect_id`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GemQualityStatsDef {
    /// Quality-stat tables, ascending by `effect_id`.
    pub effects: Vec<GemQualityStatDef>,
}

// Gem ↔ effect foreign-key domain (`overlay/gem_effects.json`)

/// A single gem variant's link to its granted effect (a single entry of
/// `overlay/gem_effects.json`).
///
/// Sourced from vendor PoB2 `Data/Gems.lua` (exported by
/// `Export/Scripts/skills.lua:898-925` from `.dat`
/// `SkillGems.GemEffects` → the `GemEffects` table; that table's bundle
/// isn't downloadable at pinned patch 4.5.0.3.4, so per the owner's decision
/// on layering production tooling, it goes through extract-lua → overlay/
/// for now, and will migrate to base/ once the `.dat` channel is restored —
/// a byte-equivalent migration commit). Deterministically extracted by
/// `sync-pob-catalog extract-lua --what gem-effects`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GemEffectDef {
    /// Gem base id (vendor `gameId` = `BaseItemTypes.Id`, aligned with
    /// [`SkillGemDef::id`]).
    pub gem_id: String,
    /// Gem effect variant id (vendor `variantId` = `GemEffects.Id`, e.g.
    /// `IceNova`). Current data is 1 gem ↔ 1 variant (vendor Gems.lua has no
    /// duplicate gameId, verified at extraction time).
    pub variant_id: String,
    /// Id of the primary effect granted (`GemEffects.GrantedEffect` →
    /// `GrantedEffects.Id`).
    pub granted_effect_id: String,
    /// Ids of additionally granted effects
    /// (`GemEffects.AdditionalGrantedEffects` column sequence = vendor
    /// `additionalGrantedEffectId1..N`). Foreign key for meta/compound gem
    /// expansion (18-G5).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_granted_effect_ids: Vec<String>,
    /// Additional statSet ids for the primary effect
    /// (`GrantedEffects.AdditionalStatSets` → `GrantedEffectStatSets.Id`,
    /// vendor `additionalStatSet1..N`). A cross-check sidecar for
    /// [`GrantedEffectDef::additional_stat_set_ids`] (read directly from the
    /// `.dat`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_stat_set_ids: Vec<String>,
}

/// Top level of `overlay/gem_effects.json` (from the consumer's perspective:
/// the `_meta` provenance header is ignored by default serde; consumers just
/// take the `gems` list, ascending by `gem_id`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GemEffectsDef {
    /// Gem → effect edge table, ascending by `gem_id`.
    pub gems: Vec<GemEffectDef>,
}

// statSet label sidecar (`overlay/stat_set_labels.json`)

/// A single statSet's vendor export index plus label text (a single entry of
/// `overlay/stat_set_labels.json`).
///
/// Sourced by joining the vendor `Export/Skills/*.txt` templates
/// (`#skill`/`#set` lines → export order and set id) with vendor
/// `Data/Skills/*.lua` (`statSets[i].label` text; the raw `.dat`
/// `GrantedEffectStatSets.Label` FK target table `GrantedEffectLabels` isn't
/// downloadable). Deterministically extracted by
/// `sync-pob-catalog extract-lua --what stat-set-labels`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatSetLabelDef {
    /// Owning granted-effect id (the template's `#skill` line).
    pub skill: String,
    /// Stable statSet id (the template's `#set` line).
    pub set_id: String,
    /// 1-based index in PoB2's exported statSets list (the same index
    /// semantics as `<Gem statSetIndex>`).
    pub set_index: u32,
    /// Label text (vendor `LabelType.Label`, falls back to the skill's
    /// display name by default — see `Export/Scripts/skills.lua:478`;
    /// extraction is a faithful transcription of the export artifact).
    pub label: String,
}

/// Top level of `overlay/stat_set_labels.json` (from the consumer's
/// perspective: `_meta` ignored, ascending by `(skill, set_index)`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StatSetLabelsDef {
    /// statSet label table.
    pub labels: Vec<StatSetLabelDef>,
}

#[cfg(test)]
mod m4_t4_dot_flags_tests {
    use super::{DotFlags, StatSetDef};

    fn bare_set() -> StatSetDef {
        StatSetDef {
            set_id: "S".into(),
            label: None,
            vendor_set_index: None,
            base_effectiveness: 0.0,
            constant_stats: Vec::new(),
            skill_attack_speed_more: None,
            dot_flags: DotFlags::default(),
            explode_corpse: false,
            implicit_stats: Vec::new(),
            levels: Vec::new(),
        }
    }

    /// An all-default dot_flags isn't written out (zero diff against
    /// existing base JSON), and old JSON (missing the `dot_flags` key)
    /// deserializes to the conservative default.
    #[test]
    fn default_dot_flags_are_skipped_and_backward_compatible() {
        let json = serde_json::to_string(&bare_set()).unwrap();
        assert!(!json.contains("dot_flags"), "全 false 不得落盘：{json}");
        let parsed: StatSetDef = serde_json::from_str(&json).unwrap();
        assert!(parsed.dot_flags.is_default(), "缺键 = 保守默认（全 false）");
        assert!(!parsed.dot_flags.verified, "缺键 = 未核验");
    }

    /// A non-default dot_flags round-trips through serde losslessly, and
    /// only the true bits get serialized.
    #[test]
    fn dot_flags_round_trip() {
        let mut set = bare_set();
        set.dot_flags = DotFlags {
            area: true,
            verified: true,
            ..Default::default()
        };
        let json = serde_json::to_string(&set).unwrap();
        assert!(json.contains(r#""area":true"#));
        assert!(json.contains(r#""verified":true"#));
        assert!(!json.contains(r#""spell""#), "false 位不落盘：{json}");
        let parsed: StatSetDef = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, set, "serde 往返必须无损");
    }
}

/// A skill resource-cost type definition (from `CostTypes.dat`).
///
/// [`GrantedEffectDef::cost_types`] is an integer foreign-key index into
/// this table; [`SkillLevelDef::cost_amounts`] gives the cost amount for
/// each resource by position. For `per_minute` resources (e.g.
/// `ManaPerMinute`, consumed continuously over time), `divisor` is 60 (the
/// raw value is "per minute"; divide by 60 to get per second).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostTypeDef {
    /// Stable resource id (e.g. `Mana` / `Life` / `ES` / `LifePercent` /
    /// `ManaPerMinute`).
    pub id: String,
    /// Value divisor (1 for instantaneous costs; 60 for per-minute
    /// resources, dividing down to a per-second amount).
    pub divisor: u32,
    /// Whether this is a resource consumed continuously over time
    /// (per-second/per-minute).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub per_minute: bool,
}
