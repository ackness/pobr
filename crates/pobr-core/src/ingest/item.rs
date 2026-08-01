//! Item modifier source ingest.
//!
//! Parses an item's English modifier text into attributed `Modifier`s,
//! choosing the [`SourceKind`] based on section (implicit / explicit /
//! enchant), and records the equipment slot (`slot`) and raw modifier text
//! (`raw_text`) so the final output can be traced source-level back to a
//! specific slot and modifier type (PoBR's core value-add over PoB).
//!
//! The mapping from section to attribution:
//!
//! | section  | [`SourceKind`]              | `SourceId.id`             |
//! |----------|-----------------------------|---------------------------|
//! | implicit | [`SourceKind::ItemImplicit`]| `item.<slot>.implicit`    |
//! | explicit | [`SourceKind::ItemAffix`]   | `item.<slot>.explicit`    |
//! | enchant  | [`SourceKind::ItemEnchant`] | `item.<slot>.enchant`     |
//!
//! `slot` is always the slot's stable ID (e.g. `helmet`); lines that can't be
//! parsed are collected into [`ItemIngest::unsupported`] (no error), matching
//! `CalculationSession`'s semantics.
//!
//! ## Quality — not modeled here
//!
//! PoB2's item quality is **not** a global `more` modifier, it's a per-stat
//! **base scaling**:
//!
//! - Weapons: physical damage `min/max = base × (1 + physInc/100) × (1 + quality/100)`
//!   (`src/Classes/Item.lua`'s `BuildModListForSlotNum` 1751-1756, physical only).
//! - Armour: armour/evasion/ES **each** get `value = base × (1 + inc/100) × (1 + quality/100)`
//!   (same file, 1812-1819 — each stat scales independently, with no crosstalk).
//! - Jewellery / belts: quality scales affix strength by tag via a
//!   **catalyst** (`getCatalystScalar`), not an overall base scaling.
//!
//! So if quality were injected into ModDb as a global `LocalPhysicalDamageMore`
//! / `LocalDefencesMore` `More` modifier, it would incorrectly apply to
//! **global** damage / all defences (across slots, across damage types),
//! contradicting PoB2's "per-item, per-stat base scaling" semantics. The
//! actual quality scaling is handled directly by the orchestration layer
//! ([`pobr-build::calc_orchestrator`]) when computing per-item base values
//! (`item_rolled_defence` / weapon `physical_min/max` × `(1 + quality/100)`,
//! per stat, per item), matching PoB2. So this module **no longer** injects a
//! quality modifier.
//!
//! Catalysts (accessory quality → catalyst) aren't modeled yet: PoBR's parsed
//! modifiers don't carry GGG affix tags (life/mana/defences/physical/attack/
//! caster…), so `getCatalystScalar` has nothing to match against; and
//! [`Item`] currently has no `catalyst` / `catalystQuality` field. See the
//! deferral note in this module's tests for details.

use pobr_data::prelude::*;

use crate::mod_parser::{ParseCtx, ParseError, ParseStatus};
use crate::{ModTag, Modifier};

/// An item modifier's section, which determines the attributed [`SourceKind`] and `SourceId` suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemModSection {
    Implicit,
    Explicit,
    Enchant,
}

impl ItemModSection {
    /// This section's attribution source category.
    pub fn source_kind(self) -> SourceKind {
        match self {
            Self::Implicit => SourceKind::ItemImplicit,
            Self::Explicit => SourceKind::ItemAffix,
            Self::Enchant => SourceKind::ItemEnchant,
        }
    }

    /// This section's stable suffix in `SourceId.id` (`item.<slot>.<suffix>`).
    pub fn id_suffix(self) -> &'static str {
        match self {
            Self::Implicit => "implicit",
            Self::Explicit => "explicit",
            Self::Enchant => "enchant",
        }
    }
}

/// Result of ingesting an item: parsed modifiers + raw text that couldn't be parsed.
#[derive(Debug, Clone, Default)]
pub struct ItemIngest {
    pub modifiers: Vec<Modifier>,
    pub unsupported: Vec<String>,
}

/// Parses an item's modifier text into modifiers attributed with slot + section.
///
/// Parse failures (structural errors) propagate as [`ParseError`]; unrecognized
/// modifiers (e.g. `mirrored`) don't error, they're collected into
/// [`ItemIngest::unsupported`] instead, matching `CalculationSession`'s
/// semantics.
///
/// Ingest order is implicit → explicit → enchant, matching how PoB displays
/// an item's text block.
///
/// Quality is **not** turned into a modifier here — its per-stat base scaling
/// is handled by the orchestration layer; see the "Quality" section of the
/// module-level docs.
///
/// Modifier parsing goes through `ctx` (a `ctx` with no engine rules injected
/// collects every modifier as Unsupported, see [`ParseCtx::parse`]).
pub fn ingest_item_with_ctx(
    slot: EquipmentSlot,
    item: &Item,
    ctx: ParseCtx<'_>,
) -> Result<ItemIngest, ParseError> {
    let mut ingest = ItemIngest::default();

    ingest_section(
        slot,
        ItemModSection::Implicit,
        &item.implicit_texts,
        &mut ingest,
        ctx,
    )?;
    ingest_section(
        slot,
        ItemModSection::Explicit,
        &item.modifier_texts,
        &mut ingest,
        ctx,
    )?;
    ingest_section(
        slot,
        ItemModSection::Enchant,
        &item.enchant_texts,
        &mut ingest,
        ctx,
    )?;

    Ok(ingest)
}

/// Parses one section's modifier text and appends it to `ingest`.
fn ingest_section(
    slot: EquipmentSlot,
    section: ItemModSection,
    texts: &[String],
    ingest: &mut ItemIngest,
    ctx: ParseCtx<'_>,
) -> Result<(), ParseError> {
    if texts.is_empty() {
        return Ok(());
    }

    let source_id = SourceId::new(
        section.source_kind(),
        format!("item.{}.{}", slot.id(), section.id_suffix()),
    );

    for text in texts {
        let outcome = ctx.parse(text)?;
        match outcome.status {
            ParseStatus::Parsed => {
                for mut modifier in outcome.mods {
                    substitute_slot_placeholder(&mut modifier, slot.id());
                    let origin = ModifierSource::new(source_id.clone())
                        .with_slot(slot.id())
                        .with_raw_text(text.clone());
                    ingest.modifiers.push(modifier.with_origin(origin));
                }
            }
            ParseStatus::Unsupported => {
                if let Some(unparsed) = outcome.unparsed {
                    ingest.unsupported.push(unparsed);
                }
            }
        }
    }

    Ok(())
}

/// The "convert to local" modifier on weapons: an unflagged
/// `CriticalStrikeMultiplier` (vendor `CritMultiplier`) gets a
/// `Condition:{Main,Off}HandAttack` tag so it only applies to the hand pass
/// **attacking with that weapon** (non-weapon attacks like Shield Wall or
/// spells don't get it).
///
/// Mirrors vendor `Item.lua:1954-1961` ("Convert accuracy, crit damage bonus,
/// … to local"): 0.22.0 (0.5.4b) added `CritMultiplier and mod.flags == 0` to
/// the conversion list. The guard matches vendor's condition-by-condition:
/// `mod.flags == 0`, `keywordFlags == 0 or KeywordFlag.Attack`, `not mod[1]`
/// (no existing tag).
///
/// Only called by the orchestration layer on **weapon** items (Weapon2's
/// shields/quivers/foci aren't converted — vendor's conversion is inside the
/// `self.base.weapon` branch).
///
/// ponytail: vendor's list also converts Accuracy/ImpaleChance/OnHit/leech
/// (pre-0.22.0 entries that PoBR hasn't modeled yet); only the 0.5.4b
/// CritMultiplier addition lands here, the rest wait for their own oracle
/// pinning before being added through the same entry point.
pub fn apply_weapon_hand_conditions(modifiers: &mut [Modifier], slot: EquipmentSlot) {
    let var = match slot {
        EquipmentSlot::Weapon1 => "MainHandAttack",
        EquipmentSlot::Weapon2 => "OffHandAttack",
        _ => return,
    };
    for m in modifiers {
        let keyword_ok =
            m.keyword_flags == KeywordFlags::NONE || m.keyword_flags == KeywordFlags::ATTACK;
        if m.name == ModName::from("CriticalStrikeMultiplier")
            && m.flags == ModFlags::NONE
            && keyword_ok
            && m.tags.is_empty()
        {
            m.tags.push(ModTag::condition(var, false));
        }
    }
}

/// Replaces the `{SlotName}` placeholder in an item modifier's Multiplier tag with this item's slot ID.
///
/// PoB2 expands `{SlotName}` when merging item mods, based on the item's slot
/// (`calcLib.mod`). A typical source is `per Socket filled` / `per socketed
/// rune or soul core` → `Multiplier{var:"RunesSocketedIn{SlotName}"}`
/// (ModParser.lua:1477-1478). After substitution, the value is read from the
/// `RunesSocketedIn<slot>` multiplier pre-filled by the orchestration layer;
/// without substitution the var never matches and silently contributes 0
/// (the per-socket scaling would be broken).
fn substitute_slot_placeholder(modifier: &mut Modifier, slot_id: &str) {
    const PLACEHOLDER: &str = "{SlotName}";
    for tag in &mut modifier.tags {
        if let ModTag::Multiplier { var, limit_var, .. } = tag {
            if var.contains(PLACEHOLDER) {
                *var = var.replace(PLACEHOLDER, slot_id);
            }
            if let Some(lv) = limit_var.as_mut()
                && lv.contains(PLACEHOLDER)
            {
                *lv = lv.replace(PLACEHOLDER, slot_id);
            }
        }
    }
}

// Flask / charm modifier ingest
//
// Flask/charm modifiers **don't directly** enter aggregation: the parsed
// output is packed into a List-type "payload mod"
// ([`FLASK_BUFF_LIST_NAME`] / [`CHARM_BUFF_LIST_NAME`], `ModValue::NestedMods`),
// which `calc/env_finalize.rs` stage 3's `merge_flasks_charms` scales by the
// effect multiplier bucket and merges into the player db, gated by
// `mode_combat` (mirroring vendor CalcPerform.lua:1429-1663's
// mergeFlasks/mergeCharms two-stage "collect → ScaleAddList → AddList").
// A List mod doesn't participate in sum/more/flag aggregation → the payload
// has zero effect on output before merging (a migration invariant).
//
// Scope: only covers "modifiers enter calc + apply the effect multiplier
// bucket"; charge/duration/recovery modeling (vendor
// flaskData.duration/charges, calcFlaskRecovery) isn't built.

/// The List mod name for a flask's modifier payload (consumed by `merge_flasks_charms`).
pub const FLASK_BUFF_LIST_NAME: &str = "FlaskBuff";
/// The List mod name for a charm's modifier payload (consumed by `merge_flasks_charms`).
pub const CHARM_BUFF_LIST_NAME: &str = "CharmBuff";
/// The name of the "this item's local effect inc" modifier inside the
/// payload (equivalent to vendor `item.flaskData.effectInc`, sourced from
/// this item's `N% increased/reduced effect` line). Read out at merge time
/// but **not injected**.
pub const LOCAL_UTILITY_EFFECT_NAME: &str = "LocalUtilityEffect";

/// Flask / charm classification. PoE2 charms share the `.dat` item_class
/// `UtilityFlask` with flasks (base id `Metadata/Items/Flasks/FourCharm*`),
/// so classification goes by base name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UtilityItemKind {
    Flask,
    Charm,
}

/// Classifies flask/charm by base name (PoE2 charm base names always contain
/// "Charm": Thawing/Ruby/… Charm; magic names like "Sapphire Charm of
/// Lightning" also match via `contains`).
pub fn classify_utility_item(item: &Item) -> UtilityItemKind {
    if item.base.to_string().contains("Charm") {
        UtilityItemKind::Charm
    } else {
        UtilityItemKind::Flask
    }
}

/// Parses an **active** flask/charm's modifiers into a payload List mod (zero direct aggregation effect).
///
/// - Attribution: `SourceId(SourceKind::Flask, "flask.<slot_key>")`, where
///   `slot_key` = the slot name lowercased with whitespace stripped
///   (`"Charm 1"` → `charm1`); inner mods each carry their own origin (slot +
///   raw_text), so they're traceable modifier-by-modifier once merged.
/// - `N% increased/reduced effect` → the payload's [`LOCAL_UTILITY_EFFECT_NAME`] Inc.
/// - `Grants Onslaught [during effect]` → an `Onslaught` Flag (consumed after
///   merging by env_finalize stage 6's buff_definitions `OnslaughtFlask`).
/// - Other lines strip the `... during effect` suffix and reuse [`parse_mod`]
///   (the active-state semantics are already handled by the slot's `active`
///   gate); unparseable lines (including hard parse errors — flask text is
///   often trigger/recovery lines, following the orchestration layer's
///   skip-and-collect tolerance) are collected into [`ItemIngest::unsupported`].
/// - When every line fails to parse, an empty payload is **still produced**
///   (vendor's condition setting is independent of modList content,
///   CalcPerform.lua:1634-1643 — `UsingCharm`/`UsingFlask` is set true based
///   on the active slot).
pub fn ingest_flask_charm_with_ctx(slot_name: &str, item: &Item, ctx: ParseCtx<'_>) -> ItemIngest {
    let slot_key: String = slot_name
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let source_id = SourceId::new(SourceKind::Flask, format!("flask.{slot_key}"));
    let make_origin = |text: &str| {
        ModifierSource::new(source_id.clone())
            .with_slot(slot_key.clone())
            .with_raw_text(text.to_string())
    };

    let mut nested: Vec<Modifier> = Vec::new();
    let mut ingest = ItemIngest::default();
    for text in item
        .implicit_texts
        .iter()
        .chain(&item.modifier_texts)
        .chain(&item.enchant_texts)
    {
        if is_trigger_line(text) {
            continue;
        }
        if let Some(value) = parse_local_effect_inc(text) {
            nested.push(
                Modifier::number(LOCAL_UTILITY_EFFECT_NAME, ModType::Inc, value)
                    .with_source(text.clone())
                    .with_origin(make_origin(text)),
            );
            continue;
        }
        match ctx.parse(strip_during_effect(text)) {
            Ok(outcome) if outcome.status == ParseStatus::Parsed => {
                for modifier in outcome.mods {
                    nested.push(modifier.with_origin(make_origin(text)));
                }
            }
            // Unsupported or a hard error: report the original line (with
            // suffix), so it can be matched line-by-line against the item text.
            Ok(_) | Err(_) => ingest.unsupported.push(text.clone()),
        }
    }

    // An empty payload **still produces** a payload mod: vendor
    // unconditionally sets the `UsingFlask`/`UsingCharm` + `Using<BaseName>`
    // conditions for every active flask/charm in the loadout
    // (CalcPerform.lua:1634-1643 charmConditions / the flask counterpart),
    // independent of whether modList has any parseable modifiers — the
    // "while you have an active Charm" family of modifiers depends on this
    // condition. An empty NestedMods list is naturally a no-op in the merge
    // stage's scaling loop, and the condition is still set as usual.
    let list_name = match classify_utility_item(item) {
        UtilityItemKind::Flask => FLASK_BUFF_LIST_NAME,
        UtilityItemKind::Charm => CHARM_BUFF_LIST_NAME,
    };
    ingest.modifiers.push(
        Modifier::new(
            ModName::from(list_name),
            ModType::List,
            crate::ModValue::NestedMods(nested),
        )
        // source = the base name: the source for the merge stage's
        // `Using<BaseName>` condition (vendor CalcPerform.lua:1536/:1647
        // `Using..baseName:gsub("%s+","")`).
        .with_source(item.base.to_string())
        .with_origin(ModifierSource::new(source_id).with_slot(slot_key)),
    );
    ingest
}

/// A flask/charm's trigger condition is inherent base text, silently skipped rather than reported as unsupported.
/// Effect lines (e.g. `Also grants N Guard` and `Possessed by ...`) still go
/// through parsing, and stay in the unsupported report while unmodeled.
fn is_trigger_line(text: &str) -> bool {
    text.trim().to_lowercase().starts_with("used when ")
}

/// `N% increased effect` / `N% reduced effect` (case-insensitive) → this item's local effect inc.
fn parse_local_effect_inc(text: &str) -> Option<f64> {
    let lower = text.trim().to_lowercase();
    let (number, sign) = lower
        .strip_suffix("% increased effect")
        .map(|n| (n, 1.0))
        .or_else(|| lower.strip_suffix("% reduced effect").map(|n| (n, -1.0)))?;
    number.parse::<f64>().ok().map(|value| value * sign)
}

// Note: `Grants Onslaught during effect` (the sole modifier on Silver Charm
// The Fall of the Axe, etc.) is **deliberately no longer special-cased**.
// PoB2's ModParser returns `unsupported` for this line (no mod is produced,
// verified via `tools/pob2-oracle/run-parsemod.sh`) — its Onslaught never
// enters modDB, and golden doesn't include that Onslaught speed boost. An
// earlier `parse_granted_buff_flag` exceeded PoB2's parsing capability by
// unconditionally emitting `flag("Onslaught")`, inflating Speed relative to
// golden (detonate-dead 2.87 vs 2.62 = 1.09x, coiling 1.08x, flicker 1.15x;
// cooldown/trigger-rate-capped grenade/frost-bomb were unaffected, hence
// 1.00x). After removal, this line falls through to `ctx.parse` below →
// Unsupported, matching PoB2 line-for-line (also consistent with PoBR's
// design philosophy: unparseable text is Unsupported). Silver **Flask**'s
// Onslaught (CalcPerform.lua:618-648's `item.baseName:match("Silver Flask")`
// active form) is a separate channel unrelated to this text modifier; it
// should be wired up through its own source when that real mechanic lands,
// not via text special-casing.

/// Strips the `... during [flask] effect` suffix (case-insensitive), returning the remaining slice.
fn strip_during_effect(text: &str) -> &str {
    let trimmed = text.trim();
    let lower = trimmed.to_lowercase();
    for suffix in [" during flask effect", " during effect"] {
        if lower.ends_with(suffix) {
            return trimmed[..trimmed.len() - suffix.len()].trim_end();
        }
    }
    trimmed
}
