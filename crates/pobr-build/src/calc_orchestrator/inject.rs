//! The injection stages of `calculate_with_data` (a split of the main flow: behavior
//! unchanged, purely grouped).
//!
//! The `inject_*` free functions below are extracted stage by stage from
//! `calculate_with_data`'s main flow; each corresponds to a self-contained injection
//! stage from the original flow (depends only on session + build/data/options, no
//! cross-stage intermediate state), called in the exact same order as the original
//! inline code → zero behavior change (backed by the parity gate's value-for-value check).
//!
//! Division of labor with `mod.rs`: `mod.rs` keeps the orchestration backbone (the
//! `calculate*` entry points + the non-injection `stage_*` functions); this module
//! carries the isomorphic work of "pouring mods into the session", including the
//! `stage_inject_*` functions.

// The orchestration backbone's (`mod.rs`) types and sibling-module helpers are brought
// in via glob import — matching the style of the rest of the `calc_orchestrator::*` submodules.
use pobr_core::calc::CalculationSession;
use pobr_core::{ItemModSection, Modifier};
use pobr_data::item::{EquipmentSlot, Item};
use pobr_data::modifier::ModType;
use pobr_data::monster::EnemyTier;
use pobr_data::source::{ModifierSource, SourceId, SourceKind};

use super::EXPOSURE_MAGNITUDE;
use super::collect::{
    append_granted_passives, character_base, engine_ctx, filter_item_parseable, gate_parses,
    keystone_mod_map, parse_grant_line, passive_effect_copies, radius_jewel_grant_modifiers,
    resolve_gems, resolve_passive_nodes, vendor_scale_mod_value,
};
use super::conditions::grenade_type_count;
use super::context::CalculationContext;
use super::item::defence::{
    defence_base_modifiers, is_local_spirit_mod, item_per_level_defence, item_spirit_modifiers,
    item_ward_modifiers, parse_has_per_level_defence, per_slot_defence_multipliers,
    per_slot_socket_multipliers, shield_base_block, shield_block_modifiers,
};
use super::item::mirror::{kalandra_reflected_ring, slot_bonus_effect_scales};
use super::item::weapon::{
    clean_item_text, is_weapon_local_mod, parse_adds_with_suffix, parse_local_defence_flat,
    parse_local_defence_inc, parse_weapon_local_crit,
};
use super::skill::buffs::{
    buff_skill_specs, herald_skill_names, self_buff_offensive_modifiers,
    spirit_reservation_modifiers, support_buff_specs, warcry_skill_specs,
};
use super::skill::mods::{
    corpse_explosion_modifiers, crossbow_reload_modifiers, dot_flag_modifiers,
    main_skill_quality_modifiers, resolved_enemy_level, skill_base_modifiers, support_modifiers,
    unselected_set_global_modifiers,
};
use super::skill::resolve::parse_gem_property_bonus;
use super::skill::triggers::trigger_modifiers;
use super::sources::{GEM_PROPERTY_READER, LineFate, LineHolder, SourceWriter, item_sections_mut};
use super::stat_map::exposure_support_modifiers;
use super::{DataOrchestratorOptions, StageCtx, scale_trunc_2dp};
use crate::build::{Build, SocketGroup};
use crate::build_data::BuildData;
use crate::error::BuildError;
use pobr_core::CampaignProgress;

/// Stage 1d: item base defence (armour/evasion/ES) + shield base block + per-item Spirit/Ward → BASE mods.
pub(super) fn inject_defence_base(session: &mut SourceWriter, build: &Build, data: &BuildData) {
    // 1d. Item base defence (armour/evasion/ES) → Item-attributed BASE mods (× quality).
    //     Item `increased Armour/Evasion/EnergyShield` mods are injected as INC via
    //     add_item, scaling this base.
    session.add_modifiers(defence_base_modifiers(build, data));
    // 1d'. Shield base block → `ShieldBlockChance` BASE (13-G8).
    //      PoB2 CalcDefence.lua:975-980 reads Weapon 2/3's `armourData.BlockChance` as
    //      the shield base; the catalog value is injected via overlay/base_item_overrides merge.
    session.add_modifiers(shield_block_modifiers(build, data));
    // 1d''. Per-item Spirit (a weapon's rolled `Spirit:` line / catalog base spirit) →
    //       `Spirit` BASE (13-G11; matching PoB2 CalcSetup.lua:1275-1277's
    //       `item.spiritValue → NewMod("Spirit","BASE")`).
    session.add_modifiers(item_spirit_modifiers(build, data));
    // 1d'''. Per-item Ward (rolled `Ward:` line / catalog base ward) → `Ward` BASE
    //        (13-G14; matching PoB2 CalcDefence.lua:1158-1186's armourData.Ward
    //        per-slot aggregation).
    session.add_modifiers(item_ward_modifiers(build, data));
}

/// Stage 2b'': active flask/charm payload injection (consumed by env_finalize stage 3's merge).
pub(super) fn inject_flasks_charms(session: &mut SourceWriter, build: &Build, data: &BuildData) {
    // 2b''. Active flasks/charms (PoB's `<Slot name="Flask N|Charm N" active="true">`,
    //       already gated by `active` in xml_build — matching vendor
    //       CalcSetup.lua:1014-1028's `slot.active` deciding env.flasks/charms):
    //       packaged via `ingest_flask_charm` into FlaskBuff/CharmBuff payloads and
    //       injected into the session (a channel switch, replacing the old "inject the
    //       raw value directly" path), merged by env_finalize stage 3's
    //       merge_flasks_charms under the mode_combat gate, applying the effect
    //       multiplier zone + setting UsingFlask/UsingCharm conditions (matching vendor
    //       CalcPerform.lua:1429-1663). A charm needs a CharmLimit source (a belt
    //       implicit etc.) to enter the budget (:1589); unparseable lines (trigger/
    //       recovery lines) are skip-and-collected.
    for (active, (slot_name, item)) in build.utility_slots.iter().map(|item| (true, item)).chain(
        build
            .inactive_utility_slots
            .iter()
            .map(|item| (false, item)),
    ) {
        // A charm base's inherent buff (e.g. Ruby Charm's `+25% to Fire Resistance`) is
        // **not in the item text** — it's a base attribute (vendor's
        // `Item.lua:838-844` folds `base.charm.buff` line by line into
        // `buffModList`). `charm_buff` is fetched from base_items and merged into the
        // item's implicit text stream, so `ingest_flask_charm` packages it into the
        // CharmBuff payload alongside everything else (same charm-slot attribution,
        // effect-scaled together during merge). No buff (non-charm / an immunity-type
        // item not modeled) → the raw item is injected directly.
        //
        // Name matching: a magic charm's `item.base` is the item's full name (a single
        // line of prefix+base+suffix; `parse_base` takes the sole name line), so an
        // exact-name lookup against base_items misses. The 13 charm base names
        // ("Ruby Charm" etc.) aren't substrings of each other, so "full name contains
        // base name" reliably locates it (a normal/rare item whose full name equals
        // the base name matches too).
        let item_name = item.base.to_string();
        let base = data
            .base_items
            .values()
            .filter(|def| def.utility.is_some() || !def.charm_buff.is_empty())
            .filter(|def| item_name.contains(def.name.as_str()))
            .max_by_key(|def| def.name.len());
        if let Some(utility) = base.and_then(|base| base.utility.as_ref()) {
            session.add_utility_base(slot_name, item, utility);
        }
        if !active {
            session.add_inactive_utility_properties(slot_name, item);
            continue;
        }
        let base_buff = base
            .map(|base| base.charm_buff.as_slice())
            .unwrap_or_default();
        if base_buff.is_empty() {
            session.add_flask_charm(slot_name, item);
        } else {
            let mut augmented = item.clone();
            augmented.implicit_texts.extend(base_buff.iter().cloned());
            session.add_flask_charm(slot_name, &augmented);
        }
    }
}

/// Stage 4: skill gems classified as active/support, each injected via its own attribution entry point.
pub(super) fn inject_skill_gems(
    session: &mut SourceWriter,
    build: &Build,
    data: &BuildData,
) -> Result<(), BuildError> {
    // 4. Skill gems: classified as active/support, each injected via its own attribution entry point.
    for gem in resolve_gems(build, data) {
        if gem.is_support {
            session
                .add_support_gem(&gem)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
        } else {
            session
                .add_skill_gem(&gem)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
        }
    }
    Ok(())
}

/// Stages 4b/4b'/4b'': aura/curse BuffSpec + support-granted buffs + herald presence count/condition injection.
pub(super) fn inject_buffs_and_heralds(
    context: &mut CalculationContext,
    session: &mut SourceWriter,
    build: &Build,
    data: &BuildData,
) {
    for (index, member) in build
        .party
        .members
        .iter()
        .enumerate()
        .filter(|(_, m)| m.enabled)
    {
        let effects = crate::party::member_effects(member, index);
        for (source, text, parsed) in effects.lines {
            session.note_line(
                &source,
                &text,
                if parsed && source.id.ends_with(".EnemyConditions") {
                    super::sources::LineFate::ReadBy("party enemy condition")
                } else if parsed {
                    super::sources::LineFate::LeftOut(
                        "a stronger copy or the curse limit excluded this party effect",
                    )
                } else {
                    super::sources::LineFate::Unparsed
                },
            );
            if !parsed {
                context
                    .support_limits
                    .push(format!("Party {}: {}", member.name, text));
            }
        }
        for spec in effects.buffs {
            session.add_party_buff_skill(spec);
        }
        session.add_enemy_modifiers(effects.enemy);
        for condition in effects.conditions {
            session.set_condition(condition, true);
        }
    }
    // 4b. Aura/curse skills → BuffSpec injected via `session.add_buff_skill` (per the
    //     §2.4 contract), consumed by pobr-core's buff_pass (env_finalize stage 4;
    //     cfg.mode_buffs is already set above): an aura defensive buff (Discipline→
    //     EnergyShield, Purity of Fire→FireResistance…) has the AuraEffect-family
    //     multiplier zone applied (CalcPerform.lua:2102-2105) before merging into the
    //     player db; a curse goes through priority/limit/slot assignment (:2829-2896).
    //     The static direct injection `aura_buff_modifiers` used before the C5-2 switch
    //     is now off.
    for spec in buff_skill_specs(context, build, data) {
        // A `Multiplier:<X>` BASE in the buff payload → bridged to cfg.multipliers
        // (matching vendor's GetMultiplier, which sums modDB's `Multiplier:<X>`
        // globally, ModStore.lua:369; PoBR's ModTag::Multiplier reads from a
        // pre-populated cfg.multipliers table, so this must be explicitly backfilled
        // here). The first consumer = Sigil of Power's
        // `Multiplier:SigilOfPowerMaxStages` BASE 4 (the limitVar denominator for the
        // per-stage MORE).
        for m in &spec.mods {
            if let Some(var) = m.name.as_str().strip_prefix("Multiplier:")
                && m.mod_type == ModType::Base
                && let Some(v) = m.value.as_number()
            {
                session.set_multiplier(var, v);
            }
        }
        session.add_buff_skill(spec);
    }
    // 4b'. Player-side buffs granted by supports (Precision I/II → Accuracy INC,
    //     sup_dex.lua:4181-4250) → BuffSpec(kind=Buff); buff_pass's Buff branch
    //     (CalcPerform.lua:1949-1962) applies the BuffEffect multiplier zone before merging into the player db.
    for spec in support_buff_specs(context, build, data) {
        session.add_buff_skill(spec);
    }

    // 4b'''. (Pre-existing #9) Warcry skills → WarcrySpec injected via
    //     `session.add_warcry_skill`, consumed by pobr-core's `calc::warcry` (before
    //     perform's hand pass): uptime is computed as
    //     `min((exert count/main skill speed)/(cooldown+cast time), 1)`, then the
    //     warcry's offensive effect (Infernal Cry's `DamageGainAsFire`) is scaled and
    //     injected accordingly (CalcOffence.lua:3203-3256).
    for spec in warcry_skill_specs(context, build, data) {
        session.add_warcry_skill(spec);
    }

    // 4b''. Herald presence count/condition (matching vendor CalcPerform.lua:1792-1805's
    //     mode_buffs section — this orchestration path always sets mode_buffs=true):
    //     active skills with `Herald` in `skill_types` among the enabled groups are
    //     deduplicated by display name → `Multiplier:Herald` = the count +
    //     `Condition:AffectedByHerald`; each herald also sets
    //     `AffectedBy<name with spaces stripped>` (matching vendor's buff-branch
    //     naming `buff.name:gsub(" ","")`; "Herald of Plague" →
    //     AffectedByHeraldofPlague — "of" stays lowercase). Consumed by mod_parser's
    //     herald condition suffix family (ModParser.lua:1826/:6326-6328).
    let heralds = herald_skill_names(build, data);
    if !heralds.is_empty() {
        session.set_multiplier("Herald", heralds.len() as f64);
        session.set_condition("AffectedByHerald", true);
        for name in &heralds {
            session.set_condition(format!("AffectedBy{}", name.replace(' ', "")), true);
        }
    }
}

/// Socketed supports in enabled groups, using the game's SkillGems.GemColour:
/// 1 = strength/red, 2 = dexterity/green, 3 = intelligence/blue.
/// Crystallised Immunities requires a strict majority (more than half of all
/// socketed supports), distinct from Gem Studded's most numerous colour.
pub(super) fn inject_support_gem_counts(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
) {
    let (mut r, mut g, mut b) = (0.0_f64, 0.0_f64, 0.0_f64);
    let mut total = 0.0;
    for group in build.enabled_socket_groups() {
        for gem_id in &group.gem_ids {
            let Some(def) = data.skill_gems.get(gem_id) else {
                continue;
            };
            if !def.is_support {
                continue;
            }
            total += 1.0;
            match def.gem_colour {
                Some(1) => r += 1.0,
                Some(2) => g += 1.0,
                Some(3) => b += 1.0,
                _ => {}
            }
        }
    }
    session.set_multiplier("RedSupportGems", r);
    session.set_multiplier("GreenSupportGems", g);
    session.set_multiplier("BlueSupportGems", b);
    for (colour, count) in [("Red", r), ("Green", g), ("Blue", b)] {
        session.set_condition(
            format!("Majority{colour}SocketedSupports"),
            count > total / 2.0,
        );
    }
    // Gemling "Gem Studded" (CalcSetup.lua:2594-2601): exactly one colour counts as
    // most numerous; the vendor's `>=` chain resolves ties in red, green, blue order.
    let most_numerous = if r >= g && r >= b {
        "MostNumerousRedSocketedSupports"
    } else if g >= b {
        "MostNumerousGreenSocketedSupports"
    } else {
        "MostNumerousBlueSocketedSupports"
    };
    session.set_condition(most_numerous, true);
}

/// The support gems connected to the calculated skill, by colour: what lines
/// such as "Skills have 6% increased Skill Speed per Connected Green Support
/// Gem" count (`overlay-common/special_mods.json`).
pub(super) fn inject_connected_support_counts(
    session: &mut CalculationSession,
    connected_supports: [u32; 3],
) {
    let names = [
        "ConnectedRedSupportGems",
        "ConnectedGreenSupportGems",
        "ConnectedBlueSupportGems",
    ];
    for (name, count) in names.into_iter().zip(connected_supports) {
        session.set_multiplier(name, f64::from(count));
    }
}

/// Whether `jewel` is a Grand Spectrum. The unique jewel has three bases,
/// each with a single line counting the Grand Spectrums socketed: Maximum
/// Life on the Ruby, Spirit on the Emerald, all Elemental Resistances on the
/// Sapphire. Items do not keep their name, so the line identifies it.
fn is_grand_spectrum(jewel: &pobr_data::item::Item) -> bool {
    jewel
        .modifier_texts
        .iter()
        .any(|line| line.ends_with("per socketed Grand Spectrum"))
}

/// The augments in the equipment's sockets, as `(Idols, the others)`. The
/// game's base items class every augment (Rune, Soul Core, Idol) as
/// `SoulCore`. The socketable category distinguishes Idols: some ancient
/// Idols, such as Carved Majesty, only have the generic `rune` base tag.
/// A socket entry that is no augment,
/// such as the skill a Talisman grants, is not counted.
fn augments_in_equipment(build: &Build, data: &BuildData) -> (usize, usize) {
    let (mut idols, mut others) = (0, 0);
    for name in build.items.values().flat_map(|item| &item.socketed) {
        let Some(base) = data.base_items.get(name) else {
            continue;
        };
        if base.item_class != "SoulCore" {
            continue;
        }
        let is_idol = data.augments.get(name).map_or_else(
            || base.tags.iter().any(|tag| tag == "idol"),
            |augment| augment.slots.values().any(|slot| slot.kind == "Idol"),
        );
        if is_idol {
            idols += 1;
        } else {
            others += 1;
        }
    }
    (idols, others)
}

/// Build-derived counts and equipment facts; actor-derived values belong to core.
///
/// Written before the Spirit reservation, whose efficiency can scale on these
/// facts ("… Reservation Efficiency of Skills per Idol in your Equipment"),
/// and before `prepare_player_stats`: the attributes that grant Life, Mana
/// and Accuracy can scale on them too, as "+7 to all Attributes per Socket
/// filled" does on the filled sockets of its item. Nothing here reads what
/// either writes.
pub(super) fn inject_build_multipliers(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
) {
    // Per-slot defence scaling (`<Stat>On<Slot>`): makes mods like "+N to Armour per M
    // Item Energy Shield on Equipped Boots" (which scale by a specific item's defence
    // value) take effect (PoB2's PerStat `<Stat>On<Slot>`).
    for (var, value) in per_slot_defence_multipliers(build, data) {
        session.set_stat(var.clone(), value);
        session.set_multiplier(var, value);
    }
    // Per-slot filled socket count (`RunesSocketedIn<slot>`): makes mods like "+N to
    // <stat> per Socket filled" (which scale by this item's number of socketed runes/
    // soul cores) take effect (matching PoB2 ModParser.lua:1477-1478).
    for (var, value) in per_slot_socket_multipliers(build) {
        session.set_multiplier(var, value);
    }
    // "N% of the Accuracy Rating on the Weapon".
    for (var, value) in super::item::weapon::weapon_accuracy_multipliers(build) {
        session.set_stat(var.clone(), value);
        session.set_multiplier(var, value);
    }
    // "for each Corrupted Item Equipped": the gear in the equipment slots.
    // Flasks and charms are not counted, nor jewels, which are socketed.
    let corrupted = build.items.values().filter(|item| item.corrupted).count();
    session.set_multiplier("CorruptedItem", corrupted as f64);
    session.set_multiplier("NonCorruptedItem", (build.items.len() - corrupted) as f64);
    // "… per socketed Grand Spectrum": the Grand Spectrums among the jewels
    // in the build's sockets, whatever their base.
    let grand_spectrums = build.jewel_items().filter(|jewel| is_grand_spectrum(jewel));
    session.set_multiplier("GrandSpectrum", grand_spectrums.count() as f64);
    // "… per Idol in your Equipment" and "… per non-Idol Augment in your
    // Equipment": what the equipment's sockets hold.
    let (idols, others) = augments_in_equipment(build, data);
    session.set_multiplier("IdolsInEquipment", idols as f64);
    session.set_multiplier("NonIdolAugmentsInEquipment", others as f64);
    // "for each of your empty Charm slots": the charms in the Charm slots.
    // The build keeps only the charms a PoB2 build marks active, so an
    // unticked charm's slot counts as empty, as PoB2 counts it.
    let charms = build
        .utility_slots
        .iter()
        .filter(|(slot, _)| slot.starts_with("Charm "))
        .count();
    session.set_empty_charm_slots(charms);
    // GrenadeTypes (matching vendor CalcPerform.lua:1238-1242: counts the number of
    // distinct granted effects among enabled active skills with `SkillType.Grenade`,
    // deduplicated) — the Demolitionist ascendancy's "… for every different Grenade
    // fired …"'s Multiplier limitVar denominator.
    session.set_multiplier("GrenadeTypes", grenade_type_count(build, data));
    // The Gemling ascendancy's Virtuous Barrier per-Attribute-Mote count
    // (virtuous_mote_counts). Consumed only by Virtuous Barrier's
    // `<res> INC ×<Attr>MoteSkillCount`; for a build without this ascendancy,
    // these three multipliers are referenced by nothing.
    let (str_mote, dex_mote, int_mote) = virtuous_mote_counts(build, data);
    session.set_multiplier("StrengthMoteSkillCount", str_mote);
    session.set_multiplier("DexterityMoteSkillCount", dex_mote);
    session.set_multiplier("IntelligenceMoteSkillCount", int_mote);
    // The Smith of Kitava ascendancy's body-armour-connected notable count (matching
    // vendor CalcSetup.lua:840-841: the number of allocated notables flagged
    // `applyToArmour=true` in tree.lua → `Multiplier:AllocatedConnectedNotable`).
    // Consumed by Masterwork's "+200 to Armour for each Connected Notable Passive Skill
    // Allocated".
    let connected_notables = build
        .tree
        .allocated_nodes
        .iter()
        .filter(|id| {
            data.passive_nodes_for(build.tree_version.as_deref())
                .get(&id.0)
                .is_some_and(|n| n.apply_to_armour)
        })
        .count();
    if connected_notables > 0 {
        session.set_multiplier("AllocatedConnectedNotable", connected_notables as f64);
    }
    // Equipment attribute requirement snapshot (matching vendor CalcPerform.lua:1848-1857's
    // `output[attr.."RequirementsOn"..slot] = floor(itemReq × reqMult)`) — the fetch
    // source for "Gain Armour equal to N% of total Strength Requirements of Equipped
    // Boots, Gloves and Helmet" (a PercentStat, `StrRequirementsOn<slot>`).
    // The separate requirement snapshot also includes gems and supports.
    for (var, value) in super::requirements::requirement_snapshots(build, data) {
        session.set_stat(var, value);
    }
    for (var, value) in per_slot_attribute_requirements(build, data) {
        session.set_stat(var, value);
    }
}

/// Each equipped item's attribute requirements (`{Str,Dex,Int}RequirementsOn<slot>` → value).
/// The slot name root matches the PercentStat tag's stat name (`StrRequirementsOnboots`
/// etc., lowercase slot name = the engine's parsed output); no requirement/empty slot
/// produces nothing.
pub(super) fn per_slot_attribute_requirements(
    build: &Build,
    data: &BuildData,
) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    for (slot, item) in build.equipped_items() {
        let Some(def) = data.base_items.get(&item.base.to_string()) else {
            continue;
        };
        let slot_key = slot.id();
        for (attr, req) in [
            ("Str", def.req_str),
            ("Dex", def.req_dex),
            ("Int", def.req_int),
        ] {
            if req > 0 {
                out.push((format!("{attr}RequirementsOn{slot_key}"), f64::from(req)));
            }
        }
    }
    out
}

/// Attribute-Mote count (the Gemling ascendancy's Virtuous Barrier): base 3/3/3 + each
/// skill the character has from a gem, by the gem's required attributes
/// (single-attribute gets +2, multi-attribute gets +1 each). Returns `(Str, Dex, Int)`.
///
/// A support gem that grants a skill (tag `grants_active_skill`, Eonyr's
/// Thunder) gives the character that skill and counts too. Groups an item or
/// a passive grants are left out: the build already lists the same skill as a
/// group of its own, and counting both would count it twice.
pub(super) fn virtuous_mote_counts(build: &Build, data: &BuildData) -> (f64, f64, f64) {
    let (mut s, mut d, mut i) = (3.0, 3.0, 3.0);
    for group in build.enabled_socket_groups() {
        if group.source.is_some() {
            continue;
        }
        for gem_id in &group.gem_ids {
            let Some(def) = data.skill_gems.get(gem_id) else {
                continue;
            };
            if def.is_support && !def.gem_tags.iter().any(|tag| tag == "grants_active_skill") {
                continue;
            }
            let req = [def.str_pct > 0, def.dex_pct > 0, def.int_pct > 0];
            let n_attr = req.iter().filter(|&&r| r).count();
            if n_attr == 0 {
                continue;
            }
            let mote = if n_attr == 1 { 2.0 } else { 1.0 };
            if req[0] {
                s += mote;
            }
            if req[1] {
                d += mote;
            }
            if req[2] {
                i += mote;
            }
        }
    }
    (s, d, i)
}

/// Stages 5/5a/5b: enemy configuration (setup_enemy) + the config interpreter's enemy bucket + player-applied elemental exposure.
pub(super) fn inject_enemy(
    session: &mut SourceWriter,
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    enemy_tier: EnemyTier,
    resolved_config: &crate::config_resolve::ResolvedConfig,
) {
    // Use the build's resolved level, also used for corpse Life; the
    // calculation session's actor does not carry the character's level.
    let enemy_level = resolved_enemy_level(build, data, options, enemy_tier);
    session.setup_enemy(enemy_level, enemy_tier);
    session.add_enemy_modifiers(super::enemy_config::modifiers(build));
    // Config "Disable EHP gain on block/suppress" (vendor CalcDefence.lua:3261).
    if matches!(
        build.config.raw_inputs.values.get("DisableEHPGainOnBlock"),
        Some(pobr_core::rules::config_interpreter::ConfigInputValue::Bool(true))
    ) {
        session.set_condition("DisableEHPGainOnBlock", true);
    }

    // 5a'. Output of the config interpreter's enemy bucket: actor-ized enemy condition
    //      entries (matching vendor's `enemyModList:NewMod("Condition:<X>", FLAG, ...)`,
    //      carrying a `Condition:Effective` tag + EnemyConfig attribution). Naturally
    //      inert under `mode_effective=false`; the cfg-side `Enemy<X>` condition is
    //      kept alive by `config_resolve`'s reverse bridge, preserving existing semantics.
    if !resolved_config.enemy_mods.is_empty() {
        session.add_enemy_modifiers(resolved_config.enemy_mods.clone());
    }

    // 5b. Player-applied elemental exposure (build config's `conditionEnemy*Exposure`)
    //     → an enemy resistance reduction (PoB2 config's default -20% per point). Only
    //     takes effect under effective semantics, must run after setup_enemy.
    if options.mode_effective {
        let exposure = [
            resolved_config
                .config
                .conditions
                .get("EnemyFireExposure")
                .copied(),
            resolved_config
                .config
                .conditions
                .get("EnemyColdExposure")
                .copied(),
            resolved_config
                .config
                .conditions
                .get("EnemyLightningExposure")
                .copied(),
        ]
        .map(|c| c.unwrap_or(false));
        if exposure.iter().any(|&on| on) {
            session.apply_enemy_exposure(exposure, EXPOSURE_MAGNITUDE);
        }
    }
}

/// Stages 1b/1b-ii: main skill base mod / quality / unselected set / DoT flag /
/// corpse explosion / crossbow reload / support / trigger injection + skill damage
/// multiplier MORE. Weapon base crit is injected with the hand sources.
pub(super) fn inject_main_skill_mods(
    context: &mut CalculationContext,
    session: &mut SourceWriter,
    ctx: &StageCtx<'_>,
) {
    let (build, data, options) = (ctx.build, ctx.data, ctx.options);
    let main_skill = &ctx.main.main_skill;
    let dmg_mult = ctx.weapons.dmg_mult;
    // 1b. Main skill cost / cooldown / base damage + this group's support gems'
    // multipliers → attributed modifiers. Attack/cast speed all go through the generic
    // chain (charges / support more / skill quality / attackSpeedMultiplier), no more
    // per-skill hardcoding.
    if let Some((skill, group, skill_id)) = main_skill {
        // Use the resolved instance's set: repeated effect ids can select different
        // forms, and additional effects need not have their own gem_skills entry.
        let main_set_key = data.selected_set_key(skill_id, ctx.main.main_stat_set_index);
        if let Some((gem_level, bonus_levels)) =
            super::skill::resolve::selected_effect_levels(build, data, group)
        {
            session.set_main_skill_level(gem_level, bonus_levels);
        }
        session.add_modifiers(skill_base_modifiers(
            context,
            skill,
            skill_id,
            main_set_key.as_deref(),
        ));
        session.add_modifiers(super::skill::mods::skill_timing_modifiers(
            data,
            group,
            skill_id,
            &ctx.main.main_skill_types,
        ));
        session.add_modifiers(super::skill::mods::totem_base_modifiers(
            build,
            data,
            group,
            skill_id,
            &ctx.main.main_skill_types,
        ));
        // 1b-i-q. Main skill gem's quality stats (T1.7): the quality segment is
        //         mapped via stat-map and injected with SourceKind::GemQuality
        //         attribution (id prefix gem.<effect id>.q<Q>).
        session.add_modifiers(main_skill_quality_modifiers(
            context,
            group,
            data,
            skill_id,
            super::skill::resolve::gemling_quality_flag(build, data),
        ));
        // 1b-i-g. Main skill's unselected statSet global-only merge (CalcActiveSkill.lua:124-140).
        session.add_modifiers(unselected_set_global_modifiers(
            context, group, data, skill_id,
        ));
        // 1b-i-d. The selected statSet's dotIs* flags → `DotIs<X>` FLAG (booleans hung
        //         directly on statSet baseMods; calc::skill_dot preserves the dotCfg
        //         bits based on these).
        session.add_modifiers(dot_flag_modifiers(group, data, skill_id));
        // Without an explicit corpse-Life assumption, use the same monster
        // level as enemy mitigation, including the selected tier's floor.
        session.add_modifiers(corpse_explosion_modifiers(
            build,
            data,
            resolved_enemy_level(build, data, options, ctx.enemy_tier),
            group,
            skill,
            skill_id,
        ));
        // 1b-i-x. Crossbow reload data channel: CrossbowReloadTimeBase (the weapon's
        //         reload_time_ms) + CrossbowBoltCount (the ammo sibling skill's stat),
        //         consumed by perform's `fill_crossbow_reload`. Returns empty for a
        //         non-crossbow/grenade skill.
        session.add_modifiers(crossbow_reload_modifiers(build, data, group, skill_id));
        if ctx
            .main
            .main_skill_types
            .iter()
            .any(|kind| kind == "NonWeaponAttack")
        {
            session.add_modifiers(pobr_core::skill_env::shield_elemental_damage_modifiers(
                skill_id, skill, build, data,
            ));
        }
        session.add_modifiers(support_modifiers(
            context,
            group,
            data,
            skill_id,
            ctx.main.main_stat_set_index,
        ));

        // 1b-iii. Trigger chain:
        // ① Data-driven recognition (trigger_configs.json's four-level key → a match
        //    against a gem in the group / the main skill id);
        // ② Built-in triggers (`skill_types` includes `Triggered`/`InbuiltTrigger`,
        //    matching PoB2's `isTriggered`).
        // Injects the trigger cooldown + trigger source's **sub-calculation**
        // statistics (post-calculation attack speed/hit/crit) as BASE, driving perform's
        // `fill_trigger` to write out a non-placeholder trigger_rate_cap /
        // skill_trigger_rate. Returns empty with no trigger relation, keeping the panel
        // at 0 (backward compatible).
        session.add_modifiers(trigger_modifiers(
            context, build, data, options, skill, group, skill_id,
        ));
    }

    // 1b-ii. Skill damage multiplier → `AddedDamage` MORE, so that **added flat damage**
    //        (weapon+item added) is scaled by baseMultiplier alongside the weapon hit
    //        (the weapon hit is already scaled at base_input × dmg_mult).
    if (dmg_mult - 1.0).abs() > f64::EPSILON {
        let origin = ModifierSource::new(SourceId::new(SourceKind::SkillGem, "skill.damageMult"))
            .with_raw_text(format!("skill damage multiplier {dmg_mult:.2}"));
        session.add_modifiers(vec![
            Modifier::number("AddedDamage", ModType::More, (dmg_mult - 1.0) * 100.0)
                .with_origin(origin),
        ]);
    }
}

/// The game's Ancestral Boost rule for Strikes: "An Ancestrally Boosted Strike
/// will target 2 additional Enemies" ([Keywords](https://poe2db.tw/us/Keywords);
/// the Ancestral Cry stat `ancestral_cry_empowered_attacks_strike_X_additional_enemies`
/// is the same 2). A skill that is Ancestrally Boosted (`Condition:AncestrallyBoosted`,
/// set by the gem stats of Ancestral Call and Fist of War) has one boost effect,
/// doubled by `double_ancestral_boost_effect`; each effect adds two targets to a
/// Strike skill (`MeleeSingleTarget`). Slam attacks are not Strikes, so they get
/// no targets. The targets are shown as `StrikeTargets` and multiply nothing.
/// The boost-effect count is written by `CalculationSession::bridge_player_conditions`
/// together with the condition.
fn ancestral_boost_modifiers() -> Vec<Modifier> {
    let origin = ModifierSource::new(SourceId::new(
        SourceKind::CharacterBase,
        "base.AncestralBoost",
    ))
    .with_raw_text("Ancestrally Boosted Strikes target 2 additional Enemies");
    vec![
        Modifier::number("AdditionalStrikeTarget", ModType::Base, 2.0)
            .with_tag(pobr_core::ModTag::multiplier(
                "AncestralBoostEffect",
                1.0,
                None,
            ))
            .with_tag(pobr_core::ModTag::SkillTypes(
                pobr_data::skill::SkillTypes::from_pob2_name("MeleeSingleTarget")
                    .expect("MeleeSingleTarget is a game skill type"),
            ))
            .with_tag(pobr_core::ModTag::condition("AncestrallyBoosted", false))
            .with_origin(origin),
    ]
}

/// Stage 1: character base (level + class-derived attributes → BASE) + elemental resistance penalty (campaign progress tier).
pub(super) fn inject_character_base(
    session: &mut SourceWriter,
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    resolved_config: &crate::config_resolve::ResolvedConfig,
) {
    // 1. Character base (level + class-derived attributes) → CharacterBase-attributed BASE modifiers.
    if options.inject_character_base
        && let Some(base) = character_base(build, data)
    {
        // The derivation coefficients are read from the injected character_constants domain (value-for-value equal to Default).
        session.add_modifiers(base.modifiers(&data.constants.character_constants));
        // character_inherent_mana_regeneration_rate_per_minute_% = 240:
        // a character regenerates 4% of maximum Mana per second.
        session.add_modifiers(vec![
            Modifier::number(
                "ManaRegenPercent",
                ModType::Base,
                data.constants
                    .character()
                    .mana_regeneration_rate_per_minute_pct
                    / 60.0,
            )
            .with_tag(pobr_core::ModTag::condition("NoInherentManaRegen", true))
            .with_origin(
                ModifierSource::new(SourceId::new(
                    SourceKind::CharacterBase,
                    "base.ManaRegenPercent",
                ))
                .with_raw_text("character inherent Mana regeneration"),
            ),
        ]);
        session.add_modifiers(ancestral_boost_modifiers());
        // Elemental resistance penalty (fire/cold/lightning; chaos has no penalty): the
        // XML Config's explicit `resistancePenalty` tier takes priority; when omitted,
        // falls back to PoB2 CalcSetup.lua's `configInput.resistancePenalty or -60`
        // (i.e. Endgame). Tier → penalty modifier goes through [`CampaignProgress`]'s
        // existing table (attributed with `campaign.resistance_penalty`; Act1's penalty
        // is 0 and produces no modifier).
        let progress = resolved_config
            .config
            .campaign_progress
            .unwrap_or(CampaignProgress::Endgame);
        session.add_modifiers(progress.modifiers());
    }
}

/// Stage 2: equipment attribution path injection — per-item filter / Kalandra mirror /
/// local mod (weapon·defence·Spirit) stripping / add_item / slot bonus-effect numeric
/// copies. `off_weapon_active` = whether the off-hand weapon source is consumed;
/// `main_weapon_active` = whether the main skill uses Weapon1 as its damage source (a weapon attack).
pub(super) fn inject_items(
    session: &mut SourceWriter,
    build: &Build,
    data: &BuildData,
    off_weapon_active: bool,
    main_weapon_active: bool,
) -> Result<(), BuildError> {
    // Slot bonus effect ("N% increased bonuses gained from Equipped Rings and Amulets",
    // the Ritualist ascendancy etc.): the corresponding slot's item mods get an
    // additional scaled copy appended (matching PoB2 CalcPerform.lua:1326-1370's
    // `EffectOfBonusesFrom<Slot>` ScaleAddMod semantics; only takes effect when scale>0).
    let bonus_scales = slot_bonus_effect_scales(build, data);
    let granted_skill_texts = super::granted_skills::resolved_granted_skill_texts(build, data);
    for (slot, item) in build.equipped_items() {
        // Kalandra's Touch's "Reflects opposite Ring": mirrors every mod of the ring in
        // the opposite slot (matching vendor CalcSetup.lua:1221-1243), while the source
        // is still attributed to the slot Kalandra's Touch is in.
        let item = kalandra_reflected_ring(build, slot, item).unwrap_or(item);
        let mut filtered = filter_item_parseable(
            item,
            engine_ctx(data),
            session,
            LineHolder::Slot(slot.id()),
            &granted_skill_texts,
            "the skill the item grants (granted_skills)",
        );
        // Main-hand weapon: strips local physical damage boost/added (already counted
        // into weapon_contribution as an independent weapon-source multiplier zone ×
        // baseMultiplier); leaving it in the global set would double-count and
        // incorrectly fold it into the additive bucket (PoB treats it as an independent
        // multiplier zone). Dual-wielding off-hand: Weapon2 is stripped the same way
        // when consumed as an off-hand weapon source — its local mods are already
        // folded into the off-hand WeaponContribution (when not consumed, the global
        // injection is left untouched).
        if slot == EquipmentSlot::Weapon1 || (slot == EquipmentSlot::Weapon2 && off_weapon_active) {
            take_item_lines(
                session,
                slot,
                &mut filtered,
                LineFate::ReadBy("local to the weapon, in its own damage (weapon_contribution)"),
                |t| is_weapon_local_mod(t, &data.local_mods.weapon),
            );
        }
        // Bare "Adds N to M <type> Damage" is stripped for a weapon that isn't the
        // damage source (#10-3, the root cause of titan/smith over-counting): vendor
        // Item.lua:1923-1928 folds every bare add-type on a weapon into weaponData
        // (local, only applies alongside that weapon's attacks). When the main skill
        // doesn't use this weapon as its damage source (a non-weapon attack like Shield
        // Wall / a spell / an unconsumed off-hand), these mods must not enter the
        // global additive bucket (titan Nebuloch's "Adds 30 to 52 Chaos damage" scaled
        // by added effectiveness → TotalDPS 1.05x otherwise). When this weapon **is**
        // the damage source, bare elemental/chaos adds stay modifiers and
        // add_weapon_item scopes them to the hand attacking with this weapon
        // (the same result as vendor's weaponData for that hand, confirmed pinned
        // at deadeye/twister 1.00x).
        let weapon_source_inactive = (slot == EquipmentSlot::Weapon1 && !main_weapon_active)
            || (slot == EquipmentSlot::Weapon2 && !off_weapon_active);
        if weapon_source_inactive && data.weapon_base(&item.base.to_string()).is_some() {
            const TYPED_ADDS_SUFFIXES: [&str; 5] = [
                "physical damage",
                "fire damage",
                "cold damage",
                "lightning damage",
                "chaos damage",
            ];
            take_item_lines(
                session,
                slot,
                &mut filtered,
                LineFate::LeftOut(
                    "added damage local to a weapon the main skill does not hit with",
                ),
                |t| {
                    let clean = clean_item_text(t);
                    TYPED_ADDS_SUFFIXES
                        .iter()
                        .any(|s| parse_adds_with_suffix(&clean, s).is_some())
                },
            );
        }
        // Armour item: strips local "increased / +flat Armour/Evasion/ES" (already
        // folded into the rolled per-item base value / the base-fallback multiplier
        // zone, see defence_base_modifiers); leaving it in the global set would
        // double-count (and incorrectly turn it into a global additive term).
        // Determining an armour item: has a base armour entry **or** the text gives a
        // rolled defence line (fallback coverage for a unique without a catalog entry).
        let rd = &item.rolled_defence;
        // A per-level defensive item (e.g. a purely-implicit unique glove) also counts
        // as an armour item — its `Has +N per level` is already folded into the
        // per-item base value (item_rolled_defence), and must be stripped from the
        // global path to avoid double/incorrect global injection.
        let has_per_level_def = item_per_level_defence(item).iter().any(|&v| v > 0.0);
        let is_armour_piece = data.armour_base(&item.base.to_string()).is_some()
            || rd.armour.is_some()
            || rd.evasion.is_some()
            || rd.energy_shield.is_some()
            || has_per_level_def;
        if is_armour_piece {
            // "N% increased Runic Ward" scales the item's own Runic Ward
            // (item_ward_modifiers); on a piece without any it scales nothing.
            take_item_lines(
                session,
                slot,
                &mut filtered,
                LineFate::ReadBy("local to the item, in its own defences (defence_base_modifiers)"),
                |t| {
                    let c = clean_item_text(t);
                    parse_local_defence_inc(&c).is_some()
                        || parse_local_defence_flat(&c).is_some()
                        || parse_has_per_level_defence(&c).is_some()
                        || pobr_core::skill_env::parse_local_ward_inc(&c).is_some()
                },
            );
            // "+N to maximum Runic Ward" of the item's own modifiers is part
            // of its Runic Ward (`local_ward`, The Brass Dome); a socketed
            // alloy's is the character's (`base_maximum_ward`).
            for (section, texts) in item_sections_mut(&mut filtered) {
                if section == ItemModSection::Enchant {
                    continue;
                }
                let source = LineHolder::Slot(slot.id()).source(section);
                texts.retain(|text| {
                    let local = pobr_core::skill_env::parse_local_ward_flat(&clean_item_text(text))
                        .is_some();
                    if local {
                        session.note_line(
                            &source,
                            text,
                            LineFate::ReadBy("the item's own Runic Ward (item_ward_modifiers)"),
                        );
                    }
                    !local
                });
            }
        }
        // A shield's own Block chance lines are part of its block chance
        // (shield_block_modifiers); in the global set they would add to the
        // character's "increased Block chance" instead of being multiplied
        // by it.
        if slot == EquipmentSlot::Weapon2 && shield_base_block(item, data).is_some() {
            take_item_lines(
                session,
                slot,
                &mut filtered,
                LineFate::ReadBy("the shield's own Block chance (shield_block_modifiers)"),
                |t| pobr_core::skill_env::parse_local_block_chance(&clean_item_text(t)).is_some(),
            );
        }
        // An item with a Spirit base (a weapon): strips local `increased Spirit` /
        // `+N to Spirit` — already folded into the rolled `Spirit:` line
        // (Item.lua:1724-1727's calcLocal conversion), or recomputed by
        // item_spirit_modifiers from the base; leaving it in the global set would
        // double-count (13-G11).
        let has_spirit_base = item.rolled_defence.spirit.is_some()
            || data
                .base_items
                .get(&item.base.to_string())
                .and_then(|b| b.spirit)
                .is_some();
        if has_spirit_base {
            take_item_lines(
                session,
                slot,
                &mut filtered,
                LineFate::ReadBy("the item's own Spirit (item_spirit_modifiers)"),
                |t| is_local_spirit_mod(&clean_item_text(t)),
            );
        }
        // Weapon items go through add_weapon_item: a flagless crit-damage mod is
        // converted to a per-hand condition (matching vendor Item.lua:1954-1961; 0.22.0
        // added CritMultiplier to the conversion list; only weapon bases convert — a
        // non-weapon item in Weapon2, like a shield/quiver/foci, doesn't).
        let is_weapon_item = matches!(slot, EquipmentSlot::Weapon1 | EquipmentSlot::Weapon2)
            && data.weapon_base(&item.base.to_string()).is_some();
        if is_weapon_item {
            // Local critical chance is already folded into the weapon base.
            // Strip it even when this weapon is not an active damage source,
            // so spells and attacks using another source cannot inherit it.
            take_item_lines(
                session,
                slot,
                &mut filtered,
                LineFate::ReadBy("local to the weapon, its own Critical Hit chance"),
                |t| parse_weapon_local_crit(t).is_some(),
            );
            session
                .add_weapon_item(slot, &filtered)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
        } else {
            session
                .add_item(slot, &filtered)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
        }

        // Slot bonus-effect copy: when this slot has an `EffectOfBonusesFrom<Slot>`
        // INC, appends a **numeric delta copy** of this item's already-injected mods
        // (matching vendor CalcPerform.lua:1347-1369, which groups BASE/INC numeric
        // mods and applies `ScaleAddMod(mod, slotEffectMod)` — the numeric scaling uses
        // [`vendor_scale_mod_value`]'s truncation semantics, delta =
        // trunc(round(v×(1+s),2))−v; a flag copy is a no-op and skipped). The Kalandra
        // mirror already replaced `filtered` above, matching vendor :1328-1334's
        // taking mods from the opposite slot. A negative scale (e.g. a foci's -50%,
        // CalcSetup.lua:1209-1220) follows the same path: full value + negative copy =
        // net ×(1+scale), equivalent to vendor's combinedList+ScaleAddList merge
        // (vendor truncates the scaled copy via `m_modf(round(v*scale,2))`; here the
        // float is kept as-is, with a per-item deviation of ≤0.5).
        if let Some(&(_, scale)) = bonus_scales
            .iter()
            .find(|(s, scale)| *s == slot && *scale != 0.0)
        {
            let ingest = pobr_core::ingest_item_with_ctx(slot, &filtered, engine_ctx(data))
                .map_err(|e| BuildError::Parse(e.to_string()))?;
            let scaled: Vec<Modifier> = ingest
                .modifiers
                .into_iter()
                .filter_map(|m| match m.value {
                    pobr_core::ModValue::Number(v) => {
                        let delta = vendor_scale_mod_value(v, 1.0 + scale) - v;
                        (delta != 0.0).then_some(Modifier {
                            value: pobr_core::ModValue::Number(delta),
                            ..m
                        })
                    }
                    _ => None,
                })
                .collect();
            session.add_modifiers(scaled);
        }
    }
    Ok(())
}

/// Takes off `item`, in `slot`, every line `take` matches before its
/// modifiers are stored, noting each with `fate`.
fn take_item_lines(
    session: &mut SourceWriter,
    slot: EquipmentSlot,
    item: &mut Item,
    fate: LineFate,
    take: impl Fn(&str) -> bool,
) {
    let holder = LineHolder::Slot(slot.id());
    for (section, texts) in item_sections_mut(item) {
        let source = holder.source(section);
        texts.retain(|text| {
            let taken = take(text);
            if taken {
                session.note_line(&source, text, fate);
            }
            !taken
        });
    }
}

/// Stage 2a: the Transcendent limbs. Every stat a limb carries is global
/// (`attack_speed_+%`, `evasion_rating_+%`, `block_chance_+%`, …, never a
/// `local_` one), so its lines are taken as they read, attributed to the
/// limb's slot. Lines no rule parses are listed as unsupported.
pub(super) fn inject_limbs(
    session: &mut SourceWriter,
    build: &Build,
    data: &BuildData,
) -> Result<(), BuildError> {
    for (slot, limb) in &build.limbs {
        let filtered = filter_item_parseable(
            limb,
            engine_ctx(data),
            session,
            LineHolder::Slot(slot.id()),
            &[],
            "",
        );
        session
            .add_limb(*slot, &filtered)
            .map_err(|e| BuildError::Parse(e.to_string()))?;
    }
    Ok(())
}

/// Mark's self offensive buff and non-main-group exposure support sources.
pub(super) fn inject_self_buff_exposure(
    context: &mut CalculationContext,
    session: &mut SourceWriter,
    build: &Build,
    data: &BuildData,
    main_skill_group: Option<&SocketGroup>,
) {
    // 4c. The **self offensive buff** (gain-as-extra) a Mark grants the player on
    //     activation → SkillGem-attributed modifier. Data-driven: an enabled gem's stat
    //     includes `*_damage_buff_damage_%_to_gain_as_<type>` (Freezing Mark→Cold,
    //     Voltaic Mark→Lightning), mapped to `DamageGainAs<Type>` BASE, injected into
    //     the gain matrix.
    session.add_modifiers(self_buff_offensive_modifiers(build, data));
    // 4c'. Exposure-effect supports outside the main group: the `<El>ExposureEffect`
    //     INC of compatible supports in the secondary group where the exposure source
    //     lives is injected globally. The main group's supports are already fully
    //     injected by support_modifiers, and are skipped inside this function to avoid
    //     double injection.
    session.add_modifiers(exposure_support_modifiers(
        context,
        build,
        data,
        main_skill_group,
    ));
    //      A skill group inflicting exposure lets the configured exposure on
    //      the enemy hold, whichever skill is being calculated.
    if pobr_core::skill_env::groups_inflict_exposure(&mut context.stat_map_ctx(), build, data) {
        use pobr_core::calc::setup_env::{EXPOSURE_ELEMENTS, can_apply_exposure};
        for element in EXPOSURE_ELEMENTS {
            session.set_condition(can_apply_exposure(element), true);
        }
    }
    // 4c''. A skill that inflicts Withered lets the character have it on the
    //      enemy, whichever skill is being calculated.
    if let Some(skill_id) = pobr_core::skill_env::skill_inflicting_withered(build, data) {
        let origin = ModifierSource::new(SourceId::new(
            SourceKind::SkillGem,
            format!("skill.{skill_id}.inflicts_withered"),
        ));
        session.add_modifiers(vec![
            Modifier::flag(pobr_core::calc::withered::CAN_WITHER).with_origin(origin),
        ]);
    }
    //      Likewise a skill whose hits Crush leaves the enemy Crushed.
    if let Some(skill_id) = pobr_core::skill_env::skill_crushing_on_hit(build, data) {
        let origin = ModifierSource::new(SourceId::new(
            SourceKind::SkillGem,
            format!("skill.{skill_id}.crushes_on_hit"),
        ));
        session.add_modifiers(vec![
            Modifier::new(
                "EnemyModifier",
                ModType::List,
                pobr_core::ModValue::NestedMods(vec![Modifier::flag("Condition:Crushed")]),
            )
            .with_origin(origin),
        ]);
    }
    // 4c'''. The Blind on the enemy is the strongest one the character's
    //       skills inflict: the effect a skill and its supports give its
    //       Blinds (Blind II's "20% increased Blind Effect") is the enemy's,
    //       as the character's own "increased Blind Effect" is.
    if let Some((skill_id, increase)) = pobr_core::skill_env::strongest_skill_blind(build, data) {
        let origin = ModifierSource::new(SourceId::new(
            SourceKind::SkillGem,
            format!("skill.{skill_id}.blind_effect"),
        ));
        let effect = Modifier::number("BlindEffect", ModType::Inc, increase)
            .with_tag(pobr_core::ModTag::condition("Effective", false));
        session.add_modifiers(vec![
            Modifier::new(
                "EnemyModifier",
                ModType::List,
                pobr_core::ModValue::NestedMods(vec![effect]),
            )
            .with_origin(origin),
        ]);
    }
}

/// Reads the complete source database before actor snapshots and condition
/// bridging, under the build's multipliers ([`inject_build_multipliers`]).
pub(super) fn inject_spirit_reservation(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
) {
    // 4d. Spirit reservation aggregation for persistent-reservation effects →
    //     `SkillSpiritReservationBase` BASE, summed by perform's fill into
    //     OutputTable::spirit_reserved (overload is only reported, not blocked). db is
    //     passed as a read-only view to fetch tree/item ReservationEfficiency mods (the
    //     tree/items are already ingested by this point); computed first then injected
    //     to avoid a mutable/immutable borrow conflict within the same statement.
    let spirit_mods = spirit_reservation_modifiers(build, data, session.mod_db(), session.config());
    session.add_modifiers(spirit_mods);
}

/// 2b. Jewels (passive tree/abyss sockets): mods injected **globally** (most jewels
///     are global mods; radius jewels are currently approximated as global too). Follows
///     add_item's skip-and-collect error tolerance.
pub(super) fn stage_inject_jewels(
    session: &mut SourceWriter,
    ctx: &StageCtx<'_>,
) -> Result<(), BuildError> {
    let jewel_effect =
        pobr_core::skill_env::CorruptedJewelEffect::from_jewels(ctx.build.jewel_items());
    // Radius directives are consumed by geometry, not the global mod parser.
    // Exempt only validated directives with available socket geometry; unknown
    // nested grants must remain visible to the trade optimizer's safety gate.
    let nodes = ctx
        .data
        .passive_nodes_for(ctx.build.tree_version.as_deref());
    let mut radius_texts = Vec::new();
    let tree_state = crate::jewel_tree::passive_jewel_state(ctx.build, ctx.data);
    radius_texts.extend(tree_state.handled_modifiers);
    // A transformed node's own lines are reported with every other allocated
    // node's, in `stage_inject_passives`.
    for node in &ctx.build.tree.allocated_nodes {
        if tree_state.unresolved.contains(&node.0) {
            session.record_unsupported_modifier_text(format!(
                "Tree:{}: missing timeless jewel seed data",
                node.0
            ));
        }
    }
    for radius in &ctx.build.radius_jewels {
        if !nodes
            .get(&radius.socket_node)
            .is_some_and(|n| n.x.is_some() && n.y.is_some())
        {
            continue;
        }
        radius_texts.extend(
            radius
                .grant_lines
                .iter()
                .filter(|line| {
                    parse_grant_line(line)
                        .is_some_and(|(_, grant)| gate_parses(engine_ctx(ctx.data), &grant))
                })
                .cloned(),
        );
        for (kind, inc) in [
            ("Small", radius.small_effect_inc),
            ("Notable", radius.notable_effect_inc),
        ] {
            if inc > 0 {
                radius_texts.push(format!(
                    "{inc}% increased Effect of {kind} Passive Skills in Radius"
                ));
            }
        }
        if let Some(label) = &radius.radius_label {
            radius_texts.push(format!("Upgrades Radius to {label}"));
        }
    }
    let parse_ctx = engine_ctx(ctx.data);
    for (index, (socket, jewel)) in ctx.build.jewels.iter().enumerate() {
        // A jewel's lines are attributed to it, `jewel.<n>` by its place among
        // the build's jewels.
        let source = SourceId::new(SourceKind::Jewel, format!("jewel.{}", index + 1));
        let filtered = filter_item_parseable(
            jewel,
            parse_ctx,
            session,
            LineHolder::Jewel(&source),
            &radius_texts,
            "the jewel's radius on the passive tree (jewel_tree)",
        );
        let texts = filtered
            .implicit_texts
            .iter()
            .chain(&filtered.modifier_texts)
            .chain(&filtered.enchant_texts);
        // The Adorned: every mod on a corrupted jewel of the rarity it names (magic
        // or rare) in one of the tree's own sockets is scaled by `1 + N/100`, the
        // value truncated after rounding to two places. Granted sockets (Sinister
        // sockets) and equipment sockets are not "Jewel Socket Passive Skills";
        // a passive skill that holds a socket scales its jewel by its own effect.
        let scale = super::collect::jewel_socket_increase(*socket, ctx.build, ctx.data, || {
            jewel_effect.increase_for(jewel)
        })
        .map(|inc| 1.0 + inc / 100.0);
        let mut mods: Vec<pobr_core::Modifier> = Vec::new();
        for text in texts {
            let outcome = parse_ctx
                .parse(text)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
            let Ok(parsed_mods) = outcome.into_applicable(text) else {
                session.record_unsupported_modifier_text(text.clone());
                session.note_line(&source, text, LineFate::Unparsed);
                continue;
            };
            session.note_line(&source, text, LineFate::Parsed);
            for mut m in parsed_mods {
                if let Some(scale) = scale
                    && let pobr_core::ModValue::Number(v) = m.value
                {
                    m.value = pobr_core::ModValue::Number(scale_trunc_2dp(v, scale));
                }
                let origin = ModifierSource::new(source.clone()).with_raw_text(text.clone());
                mods.push(m.with_origin(origin));
            }
        }
        session.add_modifiers(mods);
    }
    Ok(())
}

/// Radius grants are parsed and scaled per affected allocated node. Invalid
/// grant text remains diagnosed by the dedicated item gate above.
pub(super) fn stage_inject_radius_jewels(
    session: &mut SourceWriter,
    ctx: &StageCtx<'_>,
) -> Result<(), BuildError> {
    session.add_modifiers(radius_jewel_grant_modifiers(ctx.build, ctx.data));
    Ok(())
}

/// 2c/2d/2e. config-derived mod injection: quest reward global text, the config
/// interpreter's player mods, the customMods line channel — all three share the
/// [`ResolvedConfig`](crate::config_resolve::ResolvedConfig) output, injected adjacent
/// to each other per the existing assembly order.
pub(super) fn stage_inject_config_mods(
    session: &mut SourceWriter,
    ctx: &StageCtx<'_>,
) -> Result<(), BuildError> {
    let (data, resolved_config) = (ctx.data, &ctx.resolved_config);
    // 2c. Quest rewards / global config mods (PoB2's `questRewards`): injected as
    //     **global** modifier text (permanent global boosts to attributes / resistances
    //     / defence inc etc.). Follows add_modifier_texts's error tolerance. Quest
    //     still goes through the legacy text channel (not switched to declarative
    //     mods until vendor/parser naming is unified; `config_resolve` already
    //     excludes quest-attributed entries from the injection list to avoid
    //     double-counting).
    add_config_lines(
        session,
        data,
        &resolved_config.config.global_modifier_texts,
        &SourceId::new(SourceKind::CampaignReward, "quest"),
    )?;

    // 2d. Output of the config interpreter: player modifiers attributed to
    //     `SourceKind::Config`. Combat-gated entries (`Condition:Combat` tag) are
    //     naturally inert under mode_combat=false (D5); the list is empty when the
    //     catalog is missing (tolerant fallback — conditions still go through
    //     `resolved_config.config` via the legacy channel).
    if !resolved_config.player_mods.is_empty() {
        session.add_modifiers(resolved_config.player_mods.clone());
    }

    // 2e. The build's custom modifiers, one modifier a line, colour codes
    //     already stripped by the interpreter.
    add_config_lines(
        session,
        data,
        &resolved_config.custom_mod_lines,
        &SourceId::new(SourceKind::Config, "config.custom"),
    )
}

/// Stores the lines of one of the configuration's text channels. A gem level
/// or quality bonus among them was read where gem levels are settled
/// ([`gem_property_bonuses`](super::skill::resolve::gem_property_bonuses)) and
/// leaves nothing to store. A line no rule parses whole is not applied in
/// part: it is recorded as a dropped input, marked `Config:` for where it is.
/// The lines are attributed to `source`: the quest rewards, or the custom lines.
fn add_config_lines(
    session: &mut SourceWriter,
    data: &BuildData,
    lines: &[String],
    source: &SourceId,
) -> Result<(), BuildError> {
    let parse_ctx = engine_ctx(data);
    let mut mods = Vec::new();
    for line in lines {
        if parse_gem_property_bonus(line).is_some() {
            session.note_line(source, line, LineFate::ReadBy(GEM_PROPERTY_READER));
            continue;
        }
        if !gate_parses(parse_ctx, line) {
            session.record_unsupported_modifier_text(format!("Config: {line}"));
            session.note_line(source, line, LineFate::Unparsed);
            continue;
        }
        session.note_line(source, line, LineFate::Parsed);
        let outcome = parse_ctx
            .parse(line)
            .map_err(|e| BuildError::Parse(e.to_string()))?;
        let origin = ModifierSource::new(source.clone()).with_raw_text(line.clone());
        mods.extend(
            outcome
                .mods
                .into_iter()
                .map(|m| m.with_origin(origin.clone())),
        );
    }
    session.add_modifiers(mods);
    Ok(())
}

/// 3/3a'/3b/3b'/3c. Passive tree injection: node mods (node-level attribution) →
/// anointed notables → small-passive effect scaling delta → radius-jewel Notable
/// effect scaling delta → mod-granted keystone mapping. Positioned per the
/// existing assembly order: after equipment and config injection, before skill gems.
pub(super) fn stage_inject_passives(
    session: &mut SourceWriter,
    ctx: &StageCtx<'_>,
) -> Result<(), BuildError> {
    let (build, data) = (ctx.build, ctx.data);
    // 3. Passive tree: NodeId → node mod text (node-level attribution).
    let mut unsupported = Vec::new();
    let mut passive_nodes = resolve_passive_nodes(build, data, &mut unsupported);
    // 3a'. Anointed notables (vendor `Allocates <name>` enchant → `GrantedPassive`
    //      LIST, ModParser.lua:5809 → CalcSetup.lua:1322-1331 merges notableMap
    //      into allocNodes): matched by name against Notable nodes and appended as
    //      an AllocatedNode (same node-level attribution).
    append_granted_passives(build, data, &mut passive_nodes, &mut unsupported);
    // By node, not by the order the build lists its nodes in: the same tree
    // gives the same diagnostics.
    unsupported.sort_by_key(|(node, _)| *node);
    let tree_nodes = data.passive_nodes_for(build.tree_version.as_deref());
    for (node, line) in unsupported {
        let ascendancy = tree_nodes
            .get(&node)
            .is_some_and(|def| def.ascendancy_id.is_some());
        let source = pobr_core::passive::node_source(node, ascendancy);
        // A gem level or quality bonus is read from the node's text
        // (`gem_property_bonuses`), whether or not a rule parses it, and so is
        // what the node says about the socket it holds.
        if parse_gem_property_bonus(&line).is_some() {
            session.note_line(&source, &line, LineFate::ReadBy(GEM_PROPERTY_READER));
        } else if super::collect::is_socket_line(&line) {
            let reader = super::collect::SOCKETED_JEWEL_READER;
            session.note_line(&source, &line, LineFate::ReadBy(reader));
        } else {
            session.record_unsupported_modifier_text(format!("Tree:{node}: {line}"));
            session.note_line(&source, &line, LineFate::Unparsed);
        }
    }
    let passive_nodes = passive_nodes;
    // A gem level or quality bonus a rule also recognises is still read from
    // the node's text, where gem levels are settled.
    for node in &passive_nodes {
        let source = pobr_core::passive::node_source(node.node_id.0, node.ascendancy);
        for line in &node.modifier_texts {
            if parse_gem_property_bonus(line).is_some() {
                session.note_line(&source, line, LineFate::ReadBy(GEM_PROPERTY_READER));
            } else if super::collect::is_socket_line(line) {
                let reader = super::collect::SOCKETED_JEWEL_READER;
                session.note_line(&source, line, LineFate::ReadBy(reader));
            }
        }
    }
    if !passive_nodes.is_empty() {
        session
            .add_passive_nodes(&passive_nodes)
            .map_err(|e| BuildError::Parse(e.to_string()))?;

        session.add_modifiers(passive_effect_copies(build, data, &passive_nodes)?);
    }

    // 3c. Mod-granted keystone mapping: stats on tree keystone nodes (**excluding
    //     already-allocated ones**) are parsed into a keystone-name → mods map and
    //     injected via `session.set_keystone_mods`; consumed by merge_keystones in
    //     env_finalize stage 1/5 based on the player db's `Keystone` LIST entries
    //     ("You have <X>" / bare-name lines). Mods for already-allocated keystones
    //     are already injected by add_passive_nodes above, so excluding them from
    //     the map is PoBR's equivalent of PoB2's `env.keystonesAdded` dedup
    //     (CalcPerform.lua:66-76; see the keystone_merge.rs module doc for the
    //     tree-path modelling difference).
    session.set_keystone_mods(keystone_mod_map(build, data, &passive_nodes));
    Ok(())
}

/// 6. Inject extra global texts (campaign rewards / debug overrides).
pub(super) fn stage_inject_extra_texts(
    session: &mut SourceWriter,
    ctx: &StageCtx<'_>,
) -> Result<(), BuildError> {
    // A gem level or quality bonus among them is a line of the build view
    // (`with_extra_gem_bonuses`) and leaves nothing to store here.
    let texts = ctx
        .options
        .extra_modifier_texts
        .iter()
        .filter(|line| parse_gem_property_bonus(line).is_none());
    session
        .add_modifier_texts(texts)
        .map_err(|e| BuildError::Parse(e.to_string()))
}
