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
//! `calculate*` entry points + the `stage_*` family); this module carries the
//! isomorphic work of "pouring mods into the session".

// The orchestration backbone's (`mod.rs`) types and sibling-module helpers are brought
// in via glob import — matching the style of the rest of the `calc_orchestrator::*` submodules.
use super::*;

/// Stage 1d: item base defence (armour/evasion/ES) + shield base block + per-item Spirit/Ward → BASE mods.
pub(super) fn inject_defence_base(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
) {
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
pub(super) fn inject_flasks_charms(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
) {
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
    for (slot_name, item) in &build.utility_slots {
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
        let base_buff: &[String] = data
            .base_items
            .values()
            .filter(|def| !def.charm_buff.is_empty())
            .find(|def| item_name.contains(def.name.as_str()))
            .map(|def| def.charm_buff.as_slice())
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
    session: &mut CalculationSession,
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
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
) {
    // 4b. Aura/curse skills → BuffSpec injected via `session.add_buff_skill` (per the
    //     §2.4 contract), consumed by pobr-core's buff_pass (env_finalize stage 4;
    //     cfg.mode_buffs is already set above): an aura defensive buff (Discipline→
    //     EnergyShield, Purity of Fire→FireResistance…) has the AuraEffect-family
    //     multiplier zone applied (CalcPerform.lua:2102-2105) before merging into the
    //     player db; a curse goes through priority/limit/slot assignment (:2829-2896).
    //     The static direct injection `aura_buff_modifiers` used before the C5-2 switch
    //     is now off.
    for spec in buff_skill_specs(build, data) {
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
    for spec in support_buff_specs(build, data) {
        session.add_buff_skill(spec);
    }

    // 4b'''. (Pre-existing #9) Warcry skills → WarcrySpec injected via
    //     `session.add_warcry_skill`, consumed by pobr-core's `calc::warcry` (before
    //     perform's hand pass): uptime is computed as
    //     `min((exert count/main skill speed)/(cooldown+cast time), 1)`, then the
    //     warcry's offensive effect (Infernal Cry's `DamageGainAsFire`) is scaled and
    //     injected accordingly (CalcOffence.lua:3203-3256).
    for spec in warcry_skill_specs(build, data) {
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

/// Stage 6b: PoE2 attribute derivation (final Str/Dex/Int → Life/Mana/Accuracy delta), must run after every source is injected.
pub(super) fn inject_attribute_derivation(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
) {
    // 6b. Attribute derivation (PoE2): life/mana/accuracy must use the **final**
    //     attributes (class base + item/tree/jewel +Strength/Dex/Int, scaled by
    //     `N% increased <Attr>` — matching PoB2's `calculateAttributes`,
    //     CalcPerform.lua:381-388's
    //     `output[stat] = m_max(round(calcLib.val(modDB, stat)), 0)`).
    //     character_base already injects the "class-starting, not INC-scaled" portion
    //     of the derivation; this backfills the delta of `final total − class starting`
    //     (2 life/strength, 2 mana/intelligence, 6 accuracy/dexterity, matching vendor
    //     :424-441's Life/Accuracy/Mana from Str/Dex/Int), and must run after every source is injected.
    if options.inject_character_base {
        // PoE2's attribute derivation coefficients (+2 life per strength, +2 mana per
        // intelligence, +6 accuracy per dexterity): read from the injected
        // character_constants domain, the same source CharacterBase derives from.
        let cc = &data.constants.character_constants;
        // Class starting attributes (the portion CharacterBase bakes in; an unknown
        // class = CharacterBase not injected → 0).
        let cls = character_base(build, data);
        let (cls_str, cls_dex, cls_int) = cls
            .map(|c| (c.strength, c.dexterity, c.intelligence))
            .unwrap_or((0.0, 0.0, 0.0));
        let str_total = session.attribute_total("Strength", cls_str);
        let dex_total = session.attribute_total("Dexterity", cls_dex);
        let int_total = session.attribute_total("Intelligence", cls_int);
        // (Pre-existing #7-4) The Giant's Blood keystone's "Inherent Life granted by
        // Strength is halved" (matching vendor CalcPerform.lua:500-505: the
        // HalvesLifeFromStrength flag → `Life BASE = Str × 1` instead of ×2).
        // CharacterBase already bakes in the class-starting segment
        // `cls_str × life_per_strength`; the delta here is injected as
        // "target total − baked-in segment", making the Str-derived life total =
        // str_total × the halved coefficient (confirmed against oracle's per-source Life values, wolf-pack: 802→401).
        let life_per_str = if session.has_flag("HalvesLifeFromStrength") {
            cc.life_per_strength / 2.0
        } else {
            cc.life_per_strength
        };
        let mk = |stat: &str, value: f64| {
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::CharacterBase,
                "base.attr_derived",
            ))
            .with_raw_text(format!("{stat} from attributes"));
            Modifier::number(stat, ModType::Base, value).with_origin(origin)
        };
        session.add_modifiers([
            mk(
                "MaximumLife",
                str_total * life_per_str - cls_str * cc.life_per_strength,
            ),
            mk(
                "MaximumMana",
                cc.mana_per_intelligence * (int_total - cls_int),
            ),
            mk(
                "Accuracy",
                cc.accuracy_per_dexterity * (dex_total - cls_dex),
            ),
        ]);
    }
}

/// Stage 6c: backfills per-X resource/attribute scaling amounts (PoB2's PerStat
/// denominator variables), must run after every source is injected, before perform.
/// Equipped support gems counted by color → `Red/Green/BlueSupportGems` multipliers
/// (matching PoB2 CalcSetup.lua:2015-2044: walks **enabled** socket groups, counting
/// support gems by `grantedEffect.color` (1=R/2=G/3=B, the same enum as GGG's
/// `gem_colour`) and writing to `env.modDB.multipliers`). Consumed by the pinned entries
/// in a2-real-gaps's `MultiplierThreshold{<Color>SupportGems, 10}` (produced blind
/// against the lower bound; a missing key = no effect — activates automatically once
/// this injection lands). Vendor's same-location `Majority<Color>SocketedSupports`
/// conditions have no PoBR data consumer yet and aren't injected (YAGNI, add in this
/// same function when a need arises).
pub(super) fn inject_support_gem_counts(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
) {
    let (mut r, mut g, mut b) = (0.0_f64, 0.0_f64, 0.0_f64);
    for group in build.enabled_socket_groups() {
        for gem_id in &group.gem_ids {
            let Some(def) = data.skill_gems.get(gem_id) else {
                continue;
            };
            if !def.is_support {
                continue;
            }
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
}

pub(super) fn inject_per_x_multipliers(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
) {
    // 6c. Backfills per-X resource/attribute scaling amounts (PoB2's PerStat
    //     denominator variables): writes the total attribute / Spirit BASE (after every
    //     source is injected) and character level into cfg.multipliers, so mods like
    //     `+N to <stat> per M <resource>` (parsed as ModTag::Multiplier{var, div}) can
    //     expand by count/div when queried during perform. Must run after every source
    //     is injected, before perform; attributes/Spirit don't participate in per-X
    //     self-scaling, so base_sum's value is stable. The Life/Mana denominator =
    //     **the full-pipeline pool value** (OVERRIDE → base×(1+inc)×more,
    //     `CalculationSession::pool_total`, the same source as offence's pool
    //     computation inside perform) — vendor's PerStat reads the actor's **output**
    //     (ModStore.lua:440-460's GetStat → output.Mana/Life); BASE-only would badly
    //     under-count "3% increased Spell Damage per 100 maximum Mana" (the druid
    //     ember-fusillade node Tree:19044, vendor's value 234 = 3×floor(7889/100)).
    let str_total = session.base_sum("Strength");
    let dex_total = session.base_sum("Dexterity");
    let int_total = session.base_sum("Intelligence");
    // (Pre-existing #7-4) The Spirit denominator = **the final pool value**
    // (calc_spirit_pool, including INC/MORE and conversion deductions) — vendor's
    // PerStat reads output.Spirit; BASE-only would under-count wolf-pack's Perfidy
    // "+2 Armour per 1 Spirit" by 72 base (Spirit 336 vs base 300).
    let spirit_total = session.spirit_total();
    let mana_total = session.pool_total("MaximumMana");
    let life_total = session.pool_total("MaximumLife");
    session.set_multiplier("Strength", str_total);
    session.set_multiplier("Dexterity", dex_total);
    session.set_multiplier("Intelligence", int_total);
    session.set_multiplier("Spirit", spirit_total);
    session.set_multiplier("Mana", mana_total);
    session.set_multiplier("Life", life_total);
    session.set_multiplier("Level", f64::from(build.character.level));
    // cfg.stats snapshot backfill (a value-mirroring copy): the fetch channel for
    // PerStat/PercentStat (EvalContext::stat falls back to cfg.stats) and
    // StatThreshold (the matches gate), sharing the same key space as the multiplier
    // side (aligned after special_mod::normalize_stat_name normalization). Only
    // backfills the subset computable before perform; globals only computable inside
    // perform (Armour/ES etc.) stay 0 (see CalcConfig::stats's doc).
    session.set_stat("Strength", str_total);
    session.set_stat("Dexterity", dex_total);
    session.set_stat("Intelligence", int_total);
    session.set_stat("Spirit", spirit_total);
    session.set_stat("Mana", mana_total);
    session.set_stat("Life", life_total);
    // The main skill's Life cost snapshot (matching vendor's output.LifeCost): the
    // fetch source for per-life-cost mods (PerStat stat=LifeCost, e.g. Atalui's
    // Bloodletting's gain-as-physical). Cost is resolved before damage, matching
    // vendor's CalcOffence ordering.
    let life_cost = session.life_cost_snapshot();
    if life_cost > 0.0 {
        session.set_stat("LifeCost", life_cost);
        session.set_multiplier("LifeCost", life_cost);
    }
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
    // GrenadeTypes (matching vendor CalcPerform.lua:1238-1242: counts the number of
    // distinct granted effects among enabled active skills with `SkillType.Grenade`,
    // deduplicated) — the Demolitionist ascendancy's "… for every different Grenade
    // fired …"'s Multiplier limitVar denominator.
    session.set_multiplier("GrenadeTypes", grenade_type_count(build, data));
    // The Gemling ascendancy's Virtuous Barrier per-Attribute-Mote count (matching
    // vendor CalcSetup.lua:1396,1766-1781): base {Str,Dex,Int}=3, plus each enabled
    // non-support skill gem counted by its required attributes (str/dex/int_pct>0) —
    // a single-attribute requirement gets +2, a multi-attribute one gets +1 each.
    // Consumed only by Virtuous Barrier's `<res> INC ×<Attr>MoteSkillCount` (the sole
    // consumer in this repo); for a build without this ascendancy, these three
    // multipliers are referenced by nothing → zero behavior impact.
    // ponytail: currently doesn't exclude fromNode/fromItem granted skills as vendor
    // does; no currently-modeled granted skill carries attribute requirements, so this
    // doesn't pollute the count yet. When a relevant build shows up, exclude precisely by SocketGroup::source.
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
            data.passive_nodes
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
    // ponytail: reqMult (the GlobalAttributeRequirements mod family) is always treated
    // as 1; wire up the multiplier when a build with "reduced attribute requirements" shows up.
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
/// enabled non-support skill gem counted by its required attribute count
/// (single-attribute gets +2, multi-attribute gets +1 each). Returns `(Str, Dex, Int)`.
pub(super) fn virtuous_mote_counts(build: &Build, data: &BuildData) -> (f64, f64, f64) {
    let (mut s, mut d, mut i) = (3.0, 3.0, 3.0);
    for group in build.enabled_socket_groups() {
        for gem_id in &group.gem_ids {
            let Some(def) = data.skill_gems.get(gem_id) else {
                continue;
            };
            if def.is_support {
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

/// Stage 6d: source-granted condition flags → cfg condition bridging (Bonded modifiers / Arcane Surge).
pub(super) fn inject_condition_bridges(session: &mut CalculationSession) {
    // 6d. Source-granted condition flags → cfg condition bridging: e.g. once "Gain the
    //     benefits of Bonded modifiers on Runes and Idols" grants the
    //     `Condition:CanUseBondedModifiers` flag, a rune's `Bonded: <mod>` mod (which
    //     carries a Condition tag) takes effect (matching PoB2 ModParser's
    //     `["^bonded: "]` semantics).
    if session.has_flag("Condition:CanUseBondedModifiers") {
        session.set_condition("CanUseBondedModifiers", true);
    }
    // The Arcane Surge bridge (matching vendor CalcDefence.lua:1580-1582: the
    // `Condition:ArcaneSurge` flag → the `AffectedByArcaneSurge` condition): when a
    // tree/mod-granted "chance to Gain Arcane Surge …" FLAG (which carries trigger
    // condition tags like CritRecently, evaluated against the current cfg) is true, it
    // makes the "while you have Arcane Surge" mod family (carrying the
    // Condition:AffectedByArcaneSurge tag) take effect. The druid ember-fusillade
    // example: Tree:27388 activates the source → Tree:16940 gets +30 INC.
    if session.has_flag("Condition:ArcaneSurge") {
        session.set_condition("AffectedByArcaneSurge", true);
    }
    // The Chaos Inoculation → FullLife bridge (matching vendor CalcDefence.lua:123-126:
    // under CI, `output.Life=1` and `condList["FullLife"]=true` — a CI build is always
    // treated as at full life). PoBR's existing CI wiring only models Life=1 / chaos
    // immunity (perform.rs:320-334's EhpOptions), without bridging FullLife into cfg,
    // which caused "while on Full Life"-family damage boosts (e.g. tree node
    // Tree:56453's +40% Attack Damage) to fail to apply on CI builds. Only triggers on
    // CI builds (flicker's AvgDamage: 0.90x→0.99x); non-CI builds (including ordinary
    // builds at full life) are unaffected — FullLife in PoB is determined by actual
    // life state, and a non-CI build being at full life is a separate case (not
    // modeled); this only adds back vendor's explicit CI branch, avoiding the
    // over-count that setting it globally-true would cause for builds like deadeye
    // (confirmed: setting it globally true regresses off by −2).
    if session.has_flag("ChaosInoculation") {
        session.set_condition("FullLife", true);
    }
}

/// Stages 5/5a/5b: enemy configuration (setup_enemy) + the config interpreter's enemy bucket + player-applied elemental exposure.
pub(super) fn inject_enemy(
    session: &mut CalculationSession,
    build: &Build,
    options: &DataOrchestratorOptions,
    enemy_tier: EnemyTier,
    resolved_config: &crate::config_resolve::ResolvedConfig,
) {
    // 5. Enemy + effective DPS: setup_enemy writes enemy scaling/resistances/damage
    //    reduction; mode_effective is already in cfg. Enemy level resolution matches
    //    vendor (CalcSetup.lua:529's `env.enemyLevel =
    //    build.configTab.enemyLevel or m_min(data.misc.MaxEnemyLevel, charLevel)`): the
    //    caller's explicit level (an orchestrator option ≠0) takes priority; otherwise
    //    the build XML Config's `enemyLevel` scalar; if both are missing, falls back to
    //    0 → setup_enemy internally derives it as min(MaxEnemyLevel, character level).
    let enemy_level = if options.enemy_level != 0 {
        options.enemy_level
    } else {
        config_enemy_level(build).unwrap_or(0)
    };
    session.setup_enemy(enemy_level, enemy_tier);

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

/// Stages 1b/1b-ii/1c: main skill base mod / quality / unselected set / DoT flag /
/// corpse explosion / crossbow reload / support / trigger injection + skill damage
/// multiplier MORE + weapon base crit.
pub(super) fn inject_main_skill_mods(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    main_skill: &Option<(ResolvedSkillLevel, &SocketGroup, &str)>,
    weapon: Option<&WeaponContribution>,
    dmg_mult: f64,
) {
    // 1b. Main skill cost / cooldown / base damage + this group's support gems'
    // multipliers → attributed modifiers. Attack/cast speed all go through the generic
    // chain (charges / support more / skill quality / attackSpeedMultiplier), no more
    // per-skill hardcoding.
    if let Some((skill, group, skill_id)) = main_skill {
        // The selected statSet's per-set override key (an explicit statSetIndex selection wired into the engine's set_key).
        let main_set_key = group
            .gem_skills
            .iter()
            .find(|g| g.skill_id == *skill_id)
            .and_then(|g| data.selected_set_key(skill_id, g.stat_set_index));
        session.add_modifiers(skill_base_modifiers(
            skill,
            skill_id,
            main_set_key.as_deref(),
        ));
        // 1b-i-q. Main skill gem's quality stats (T1.7): the quality segment is
        //         mapped via stat-map and injected with SourceKind::GemQuality
        //         attribution (id prefix gem.<effect id>.q<Q>).
        session.add_modifiers(main_skill_quality_modifiers(group, data, skill_id));
        // 1b-i-g. Main skill's unselected statSet global-only merge (CalcActiveSkill.lua:124-140).
        session.add_modifiers(unselected_set_global_modifiers(group, data, skill_id));
        // 1b-i-d. The selected statSet's dotIs* flags → `DotIs<X>` FLAG (booleans hung
        //         directly on statSet baseMods; calc::skill_dot preserves the dotCfg
        //         bits based on these).
        session.add_modifiers(dot_flag_modifiers(group, data, skill_id));
        // 1b-i-c. Corpse explosion base damage: the explodeCorpse-gated statSet's
        //         `monsterLife × corpseExplosionLifeMultiplier` → Physical BASE
        //         (matching vendor CalcOffence.lua:2211-2217; e.g. Detonate Dead).
        session.add_modifiers(corpse_explosion_modifiers(
            build, data, options, group, skill, skill_id,
        ));
        // 1b-i-x. Crossbow reload data channel: CrossbowReloadTimeBase (the weapon's
        //         reload_time_ms) + CrossbowBoltCount (the ammo sibling skill's stat),
        //         consumed by perform's `fill_crossbow_reload`. Returns empty for a
        //         non-crossbow/grenade skill.
        session.add_modifiers(crossbow_reload_modifiers(build, data, group, skill_id));
        session.add_modifiers(support_modifiers(group, data, skill_id));

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
            build, data, options, skill, group, skill_id,
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

    // 1c. Weapon base crit chance → Weapon1-attributed BASE SkillBaseCritChance
    //     (**attack skills only**; the base-material bucket, distinct from the mod
    //     bucket — see the same-named comment in skill_base_modifiers). Spell skills
    //     use their own base crit (injected by skill_base_modifiers) and don't pick up
    //     weapon crit — so this is skipped when the main skill has its own crit_chance.
    let main_skill_has_own_crit = main_skill
        .as_ref()
        .map(|(s, _, _)| s.crit_chance.is_some_and(|c| c > 0.0))
        .unwrap_or(false);
    if let Some(w) = weapon
        && w.crit_chance > 0.0
        && !main_skill_has_own_crit
    {
        let origin = ModifierSource::new(SourceId::new(SourceKind::Item, "weapon1.base"))
            .with_raw_text(format!("weapon base crit {}%", w.crit_chance));
        session.add_modifiers(vec![
            Modifier::number("SkillBaseCritChance", ModType::Base, w.crit_chance)
                .with_origin(origin),
        ]);
    }
}

/// Stage 1: character base (level + class-derived attributes → BASE) + elemental resistance penalty (campaign progress tier).
pub(super) fn inject_character_base(
    session: &mut CalculationSession,
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
    session: &mut CalculationSession,
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
    for (slot, item) in build.equipped_items() {
        // Kalandra's Touch's "Reflects opposite Ring": mirrors every mod of the ring in
        // the opposite slot (matching vendor CalcSetup.lua:1221-1243), while the source
        // is still attributed to the slot Kalandra's Touch is in.
        let item = kalandra_reflected_ring(build, slot, item).unwrap_or(item);
        let mut filtered = filter_item_parseable(item, engine_ctx(data));
        // Main-hand weapon: strips local physical damage boost/added (already counted
        // into weapon_contribution as an independent weapon-source multiplier zone ×
        // baseMultiplier); leaving it in the global set would double-count and
        // incorrectly fold it into the additive bucket (PoB treats it as an independent
        // multiplier zone). Dual-wielding off-hand: Weapon2 is stripped the same way
        // when consumed as an off-hand weapon source — its local mods are already
        // folded into the off-hand WeaponContribution (when not consumed, the global
        // injection is left untouched).
        if slot == EquipmentSlot::Weapon1 || (slot == EquipmentSlot::Weapon2 && off_weapon_active) {
            let drop_local = |texts: Vec<String>| -> Vec<String> {
                texts
                    .into_iter()
                    .filter(|t| !is_weapon_local_mod(t, &data.local_mods.weapon))
                    .collect()
            };
            filtered.implicit_texts = drop_local(filtered.implicit_texts);
            filtered.modifier_texts = drop_local(filtered.modifier_texts);
            filtered.enchant_texts = drop_local(filtered.enchant_texts);
        }
        // Bare "Adds N to M <type> Damage" is stripped for a weapon that isn't the
        // damage source (#10-3, the root cause of titan/smith over-counting): vendor
        // Item.lua:1923-1928 folds every bare add-type on a weapon into weaponData
        // (local, only applies alongside that weapon's attacks). When the main skill
        // doesn't use this weapon as its damage source (a non-weapon attack like Shield
        // Wall / a spell / an unconsumed off-hand), these mods must not enter the
        // global additive bucket (titan Nebuloch's "Adds 30 to 52 Chaos damage" scaled
        // by added effectiveness → TotalDPS 1.05x otherwise). When this weapon **is**
        // the damage source, current behavior is kept: bare elemental/chaos adds
        // approximate through global injection (numerically equivalent to vendor's
        // weaponData conversion, confirmed pinned at deadeye/twister 1.00x).
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
            let drop_typed_adds = |texts: Vec<String>| -> Vec<String> {
                texts
                    .into_iter()
                    .filter(|t| {
                        let clean = clean_item_text(t);
                        !TYPED_ADDS_SUFFIXES
                            .iter()
                            .any(|s| parse_adds_with_suffix(&clean, s).is_some())
                    })
                    .collect()
            };
            filtered.implicit_texts = drop_typed_adds(filtered.implicit_texts);
            filtered.modifier_texts = drop_typed_adds(filtered.modifier_texts);
            filtered.enchant_texts = drop_typed_adds(filtered.enchant_texts);
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
            let drop_def = |texts: Vec<String>| -> Vec<String> {
                texts
                    .into_iter()
                    .filter(|t| {
                        let c = clean_item_text(t);
                        parse_local_defence_inc(&c).is_none()
                            && parse_local_defence_flat(&c).is_none()
                            && parse_has_per_level_defence(&c).is_none()
                    })
                    .collect()
            };
            filtered.implicit_texts = drop_def(filtered.implicit_texts);
            filtered.modifier_texts = drop_def(filtered.modifier_texts);
            filtered.enchant_texts = drop_def(filtered.enchant_texts);
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
            let drop_spirit = |texts: Vec<String>| -> Vec<String> {
                texts
                    .into_iter()
                    .filter(|t| !is_local_spirit_mod(&clean_item_text(t)))
                    .collect()
            };
            filtered.implicit_texts = drop_spirit(filtered.implicit_texts);
            filtered.modifier_texts = drop_spirit(filtered.modifier_texts);
            filtered.enchant_texts = drop_spirit(filtered.enchant_texts);
        }
        // Weapon items go through add_weapon_item: a flagless crit-damage mod is
        // converted to a per-hand condition (matching vendor Item.lua:1954-1961; 0.22.0
        // added CritMultiplier to the conversion list; only weapon bases convert — a
        // non-weapon item in Weapon2, like a shield/quiver/foci, doesn't).
        let is_weapon_item = matches!(slot, EquipmentSlot::Weapon1 | EquipmentSlot::Weapon2)
            && data.weapon_base(&item.base.to_string()).is_some();
        if is_weapon_item {
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

/// Stages 4c/4c'/4d: Mark's self offensive buff (gain-as-extra) + non-main-group exposure supports + Spirit reservation aggregation.
pub(super) fn inject_self_buff_exposure_spirit(
    session: &mut CalculationSession,
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
    session.add_modifiers(exposure_support_modifiers(build, data, main_skill_group));
    // 4d. Spirit reservation aggregation for persistent-reservation effects →
    //     `SkillSpiritReservationBase` BASE, summed by perform's fill into
    //     OutputTable::spirit_reserved (overload is only reported, not blocked). db is
    //     passed as a read-only view to fetch tree/item ReservationEfficiency mods (the
    //     tree/items are already ingested by this point); computed first then injected
    //     to avoid a mutable/immutable borrow conflict within the same statement.
    let spirit_mods = spirit_reservation_modifiers(build, data, session.mod_db());
    session.add_modifiers(spirit_mods);
}
