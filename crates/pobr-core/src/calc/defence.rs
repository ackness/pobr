use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::{Actor, Env, round};

/// Output of the ES recharge rate / delay (ES recharge, gap: es-recharge-missing).
///
/// Source: agent-docs/energy-shield.md §Recharge;
///       PoB2 `src/Data/Misc.lua` (`character_inherent_energy_shield_recharge_rate_per_minute_% = 750`);
///       PoB2 `src/Modules/CalcDefence.lua` EnergyShieldRecharge section.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EsRecharge {
    /// The fraction of ES recovered per second (e.g. 0.125 = 12.5%/s). 0 means no recharge
    /// (disabled by ZealotsOath).
    pub rate_fraction: f64,
    /// The delay (seconds) before recharge starts. Defaults to 4 seconds (after taking ES damage).
    pub delay_seconds: f64,
}

/// Aggregated avoidance-chance result (Avoidance, gap: avoidance-ailment-missing / ehp-no-avoidance-layer).
///
/// Source: agent-docs/active-defences.md §3;
///       PoB2 `src/Modules/CalcDefence.lua` avoidance section (`AvoidChanceCap=75`, ailment avoidance cap 100).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AvoidanceResult {
    /// N% chance to avoid all hit damage (capped at 75%).
    pub avoid_all_damage_from_hits: f64,
    /// N% chance to avoid projectile damage (capped at 75%).
    pub avoid_projectile_damage: f64,
    /// Per-type "chance to avoid <Type> hit damage" (`Avoid<Type>DamageChance` BASE) —
    /// vendor CalcDefence.lua:3277-3300 scales DamageIn per type by `(1 - avoid/100)`.
    /// Order = [Physical, Fire, Cold, Lightning, Chaos] (`DamageType as usize`).
    pub avoid_typed_damage: [f64; 5],
    /// N% chance to avoid stun (includes the ES implicit +50%, capped at 100%).
    pub avoid_stun: f64,
    /// N% chance to avoid ignite (capped at 100%).
    pub avoid_ignite: f64,
    /// N% chance to avoid shock (capped at 100%).
    pub avoid_shock: f64,
    /// N% chance to avoid chill (capped at 100%).
    pub avoid_chill: f64,
    /// N% chance to avoid freeze (capped at 100%).
    pub avoid_freeze: f64,
    /// N% chance to avoid poison (capped at 100%).
    pub avoid_poison: f64,
    /// N% chance to avoid bleeding (capped at 100%).
    pub avoid_bleeding: f64,
}

/// Damage-taken multiplier suite (Taken multiplier, gap: ehp-no-taken-multiplier).
///
/// Distinguishes the "hit" (WhenHit) context from the "over time" (OverTime) context.
/// Formula: `TakenMult = max(0, (1 + Σinc/100) × Π(1 + more/100))`.
/// Source: agent-docs/recovery-charges-buffs.md §4.1;
///       agent-docs/active-defences.md §PoB2 implementation;
///       PoB2 `src/Modules/CalcDefence.lua` TakenHitMult section.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TakenMultiSuite {
    /// Physical hit-taken multiplier (fraction, 1.0 = no mitigation).
    pub physical_when_hit: f64,
    /// Fire hit-taken multiplier.
    pub fire_when_hit: f64,
    /// Cold hit-taken multiplier.
    pub cold_when_hit: f64,
    /// Lightning hit-taken multiplier.
    pub lightning_when_hit: f64,
    /// Chaos hit-taken multiplier.
    pub chaos_when_hit: f64,
    /// Elemental (all) hit-taken multiplier (the generic fire/cold/lightning bonus).
    pub elemental_when_hit: f64,
    /// Over-time damage-taken multiplier, all types.
    pub all_over_time: f64,
}

/// Crit extra damage reduction (gap: crit-extra-damage-reduction-missing).
///
/// Source: agent-docs/active-defences.md §4;
///       PoB2 `src/Modules/CalcDefence.lua`:
///         `CritExtraDamageReduction = min(Sum("BASE","ReduceCritExtraDamage"), 100)`
///         `EnemyCritEffect = 1 + enemyCritChance/100 * (enemyCritDamage/100) * (1 - reduction/100)`
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CritExtraReduction {
    /// Reduction to the extra crit damage taken (percentage, 0-100, capped at 100%).
    pub reduction_pct: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DefenceOutput {
    pub armour: f64,
    pub evasion: f64,
    pub energy_shield: f64,
    pub chance_to_be_hit: f64,
}

pub fn calc_defence(actor: &mut Actor, cfg: &CalcConfig, enemy_accuracy: f64) -> DefenceOutput {
    // Five-way resource conversion matrix (CalcDefence.lua:1301-1390): the three defences go
    // through the ConvertTo/GainAs matrix + the Body Armour doubling flag, then get aggregated.
    // The old dedicated ES→Mana channel (es_to_mana_rate) has been folded into the matrix; the
    // amount converted into Mana/Life is injected by `perform` before the minimal calculation
    // (see [`calc_defence_resources`]) — this function only consumes the three final defence values.
    let keystones = crate::rules::DefenceKeystones::from_db(&actor.mod_db, cfg);
    let resources = calc_defence_resources(&actor.mod_db, cfg, &actor.base, &keystones);
    let armour = resources.armour;
    let evasion = resources.evasion;
    let energy_shield = resources.energy_shield;
    // Defensive side: monster hits the player, using monster_hit_chance (agent-docs/accuracy-and-enemy.md §2).
    let chance_to_be_hit = monster_hit_chance(evasion, enemy_accuracy);

    actor.output.armour = armour;
    actor.output.evasion = evasion;
    actor.output.energy_shield = energy_shield;
    actor.output.chance_to_be_hit = chance_to_be_hit;

    actor.breakdown.push("armour", armour);
    actor.breakdown.push("evasion", evasion);
    actor.breakdown.push("energy_shield", energy_shield);
    actor.breakdown.push("chance_to_be_hit", chance_to_be_hit);

    DefenceOutput {
        armour,
        evasion,
        energy_shield,
        chance_to_be_hit,
    }
}

/// The player's chance to hit a monster with an attack (offensive side, `calcs.hitChance`).
///
/// PoE2 formula (CalcDefence.lua `calcs.hitChance`, agent-docs/accuracy-and-enemy.md §2):
/// `rawChance = accuracy * 1.25 / (accuracy + evasion * 0.3)`, clamped to `[0.05, 1.0]`.
///
/// Edge cases:
/// - accuracy=0, evasion=0 (unset/bare panel) → 1.0 (always hits)
/// - accuracy <= 0, evasion > 0 → 0.05 (floor)
/// - accuracy > 0, evasion <= 0 → 1.0 (always hits)
///
/// **Note**: spells always hit — the caller uses 1.0 directly when `cfg.is_spell()` is true and
/// never calls this function (Bug#4 spell-must-hit, agent-docs/accuracy-and-enemy.md §3).
pub fn hit_chance(evasion: f64, accuracy: f64) -> f64 {
    if accuracy <= 0.0 && evasion <= 0.0 {
        // Both zero → no evasive target → always hits.
        return 1.0;
    }

    if accuracy <= 0.0 {
        // Accuracy is zero (or negative) with some evasion present → hit chance floors at 5%.
        return 0.05;
    }

    if evasion <= 0.0 {
        // Monster has no evasion → always hits.
        return 1.0;
    }

    // PoE2 offensive-side hit formula (agent-docs/accuracy-and-enemy.md §2):
    //   rawChance (fraction) = accuracy * 1.25 / (accuracy + evasion * 0.3)
    let raw = accuracy * 1.25 / (accuracy + evasion * 0.3);
    let chance = raw.clamp(0.05, 1.0);
    if chance > 0.9999 { 1.0 } else { round(chance) }
}

/// The monster's chance to hit the player with an attack (defensive side, `calcs.monsterHitChance`).
///
/// PoE2 defensive-side formula (CalcDefence.lua, agent-docs/accuracy-and-enemy.md §2.1 note):
/// `raw = 1 - 0.95 * evasion / (evasion + 4 * accuracy)`, clamped to `[0.05, 1.0]`.
/// **Asymmetric** with the offensive-side formula — do not mix them up.
pub fn monster_hit_chance(player_evasion: f64, enemy_accuracy: f64) -> f64 {
    if player_evasion <= 0.0 {
        return 1.0;
    }
    if enemy_accuracy <= 0.0 {
        // Enemy accuracy is zero → gives the defender maximum evasion, returns the 5% floor.
        return 0.05;
    }
    let raw = 1.0 - 0.95 * player_evasion / (player_evasion + 4.0 * enemy_accuracy);
    let chance = raw.clamp(0.05, 1.0);
    if chance > 0.9999 { 1.0 } else { round(chance) }
}

pub fn armour_reduction(armour: f64, raw_hit: f64) -> f64 {
    if armour <= 0.0 || raw_hit <= 0.0 {
        return 0.0;
    }

    round(armour / (armour + 10.0 * raw_hit))
}

// Five-way defensive resource conversion matrix
// vendor: CalcDefence.lua:1301-1390 (resourceList matrix),
//         :1150-1290 / :806-808 (Body Armour doubling flag)

/// The matrix's five resource names (order = PoB2's resourceList processing order,
/// CalcDefence.lua:1300-1306; the first [`MATRIX_DEFENCE_COUNT`] entries are defence resources,
/// which get per-slot aggregation).
/// `<Src>ConvertTo<Dst>` / `<Src>GainAs<Dst>` mod names are built by concatenating this table
/// (the parse table in W0.1 is the contract).
const MATRIX_RESOURCES: [&str; 5] = ["Armour", "Evasion", "EnergyShield", "Life", "Mana"];
/// Number of defence resources (the `MATRIX_RESOURCES` prefix: Armour/Evasion/EnergyShield).
const MATRIX_DEFENCE_COUNT: usize = 3;
/// Body Armour slot ID (`EquipmentSlot::BodyArmour.id()`, the slot the doubling flag applies to).
const BODY_ARMOUR_SLOT: &str = "bodyarmour";

/// Output of the five-way resource conversion matrix: the three final defence values + the
/// amount converted into the non-defence targets (Life/Mana).
///
/// `extra_life` / `extra_mana` correspond to PoB2's `NewMod("Extra"..name, "BASE", …)`
/// (CalcDefence.lua:1383) semantics — `perform` injects them as `MaximumLife` / `MaximumMana`
/// BASE before the minimal calculation (subject to the Life/Mana global factors).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DefenceResources {
    pub armour: f64,
    pub evasion: f64,
    pub energy_shield: f64,
    /// The amount converted from defence sources into Life (PoB2 `ExtraLife` BASE semantics).
    pub extra_life: f64,
    /// The amount converted from defence sources into Mana (includes the old dedicated ES→Mana
    /// channel; PoB2 `ExtraMana` BASE semantics).
    pub extra_mana: f64,
}

/// The five-way defensive resource conversion matrix + per-slot aggregation
/// (PoB2 CalcDefence.lua:1301-1390).
///
/// Flow (matches vendor: sources are processed **serially** in `MATRIX_RESOURCES` order — a
/// source processed earlier can have its converted-in amount re-converted by a later source, but
/// not the reverse):
/// 1. Sum `<Src>ConvertTo<Dst>` BASE, capped at 100 per pair; if a source's total conversion
///    exceeds 100, normalize proportionally (:1311-1320 — vendor's normalization loop actually
///    doesn't fire due to a misused `ipairs`, so this implements true normalization by design decision);
/// 2. Slot base values (per-item rolled equipment base, `SlotName`-tagged BASE) + the Body Armour
///    doubling flag (:1150-1290 / :806-808; the current vendor version applies the doubling in
///    the gear-stats section and re-reads the raw value in resourceList, which is its own quirk;
///    by design decision it's applied to the matrix's slot base here):
///    - `DoubleBodyArmourDefence`: armour/evasion/ES all ×2 (:1161/:1189/:1214/:1232);
///    - `Unbreakable`: armour ×2 (:1217); together with `IronReflexes`, evasion also ×2 (:1235-1237);
///    - `EnergyShieldToWard`: the equipped ES slot base no longer aggregates into ES (goes to
///      Ward instead, consumed by Track D; :1192-1205).
/// 3. Per-source conversion (:1327-1373): `rate = ConvertTo + GainAs` (GainAs doesn't reduce the source);
///    - defence sources: slot base × rate → for defence targets, goes into the **same slot's**
///      bucket / for non-defence targets, goes into the global converted-in amount; global base ×
///      rate → goes to the target's global amount; the source is shrunk once by
///      `(100 − totalConversion)/100` (vendor's repeated per-target shrinking is a bug, not
///      reproduced here);
///    - non-defence sources (Life/Mana): `ceil(global base × rate/100)` goes to the target
///      (:1364-1366); the source's own deduction belongs to the doActorLifeManaSpirit domain
///      (:73-126), the matrix doesn't reduce it.
/// 4. Per-slot aggregation of defence resources (:1374-1381, equivalent to the old
///    `scaled_defence_stat`):
///    `total = global_base × (1 + Σg_inc/100) × Πg_more
///           + Σ_slots slot_base × (1 + (Σg_inc + Σslot_inc)/100) × (Πg_more × Πslot_more)`
///    slot-scoped inc shares the same additive bucket as global inc (matches PoB2
///    `calcLib.mod({slotName=slot})`).
///
/// With no matrix mods and no keystones, this function is **bit-for-bit identical** to the old
/// three separate `scaled_defence_stat` calls (the shrink factor is always 1.0, the converted-in
/// amount is always 0.0, and the floating-point operation order matches).
pub fn calc_defence_resources(
    db: &ModDb,
    cfg: &CalcConfig,
    base: &super::ActorBaseStats,
    keystones: &crate::rules::DefenceKeystones,
) -> DefenceResources {
    // 1) ConvertTo rate matrix: per-pair cap 100 + row normalization (:1311-1320)
    let mut conv = [[0.0_f64; MATRIX_RESOURCES.len()]; MATRIX_RESOURCES.len()];
    let mut total_conv = [0.0_f64; MATRIX_RESOURCES.len()];
    for (s, src) in MATRIX_RESOURCES.iter().enumerate() {
        for (t, dst) in MATRIX_RESOURCES.iter().enumerate() {
            if s == t {
                continue;
            }
            let name = ModName::from(format!("{src}ConvertTo{dst}"));
            conv[s][t] = db.sum(ModType::Base, cfg, &[name]).clamp(0.0, 100.0);
            total_conv[s] += conv[s][t];
        }
        if total_conv[s] > 100.0 {
            let factor = 100.0 / total_conv[s];
            for rate in &mut conv[s] {
                *rate *= factor;
            }
            total_conv[s] = 100.0;
        }
    }

    // 2) Defence slot base values + Body Armour doubling flag
    let mut slots: [Vec<(String, f64)>; MATRIX_DEFENCE_COUNT] = [
        db.slot_bases(cfg, &ModName::from("Armour")),
        db.slot_bases(cfg, &ModName::from("Evasion")),
        // EnergyShieldToWard: the equipped ES slot base converts to Ward (Track D), no longer
        // aggregated into ES (:1192-1205).
        if keystones.energy_shield_to_ward {
            Vec::new()
        } else {
            db.slot_bases(cfg, &ModName::from("EnergyShield"))
        },
    ];
    for (idx, slot_list) in slots.iter_mut().enumerate() {
        for (slot, value) in slot_list.iter_mut() {
            if slot != BODY_ARMOUR_SLOT {
                continue;
            }
            if keystones.double_body_armour_defence {
                *value *= 2.0; // :1214/:1232 (armour/evasion), :1189 (ES)
            }
            match idx {
                0 if keystones.unbreakable => *value *= 2.0, // :1217
                1 if keystones.unbreakable && keystones.iron_reflexes => *value *= 2.0, // :1235-1237 / :806-808
                _ => {}
            }
        }
    }

    // 3) Per-source serial conversion (:1327-1373)
    // Global base inputs: defence resources use their own stat name BASE; Life/Mana's flat BASE
    // name is Maximum*.
    let base_inputs = [
        base.armour,
        base.evasion,
        base.energy_shield,
        base.life,
        base.mana,
    ];
    let global_base_names = [
        "Armour",
        "Evasion",
        "EnergyShield",
        "MaximumLife",
        "MaximumMana",
    ];
    // received: the amount each resource received from global conversion (for defence sources,
    // folded into their global base at processing time; for non-defence sources, becomes Extra*).
    let mut received = [0.0_f64; MATRIX_RESOURCES.len()];
    // kept_global: the retained global base of each defence source after shrinking (used for aggregation).
    let mut kept_global = [0.0_f64; MATRIX_DEFENCE_COUNT];
    // The Total direct-add channel (vendor :1331 `source.totalBase = Sum(BASE, modsTotal)`, e.g.
    // Discipline aura's `EnergyShieldTotal` — "additional **Total** Energy Shield"):
    // **added straight to the final value, bypassing inc/more** (:1394
    // `output = … × calcLib.mod(…) + res.totalBase`), propagated by conversion at the same rate
    // (:1362-1366), shrunk along with the source (:1388). Structured the same way as the global
    // base, serially.
    let mut total_received = [0.0_f64; MATRIX_RESOURCES.len()];
    let mut kept_total = [0.0_f64; MATRIX_DEFENCE_COUNT];
    for (s, src) in MATRIX_RESOURCES.iter().enumerate() {
        // The global base at this source's processing time = the input base stat + the global
        // flat BASE + amounts converted in by earlier sources (serial semantics, :1328-1329
        // `globalBase = Sum(BASE, source.mods) + source.globalBase`).
        let global_s = base_inputs[s]
            + db.sum_global_only(ModType::Base, cfg, &[ModName::from(global_base_names[s])])
            + received[s];
        // Total channel input = `<Src>Total` BASE + amounts converted in by earlier sources'
        // total (:1331/:1364, serial and structured the same way).
        let total_s =
            db.sum_global_only(ModType::Base, cfg, &[ModName::from(format!("{src}Total"))])
                + total_received[s];
        let is_defence_src = s < MATRIX_DEFENCE_COUNT;
        if is_defence_src {
            // Defence sources: earlier converted-in amounts get folded into the global base to
            // participate in this source's own conversion/shrinking (goes into kept_global), then
            // zeroed to avoid double-counting; non-defence sources keep `received` as-is (that's
            // the Extra* output, matching vendor `res.globalBase` semantics).
            received[s] = 0.0;
            total_received[s] = 0.0;
        }
        // Slot snapshot: every target of this source reads from the same snapshot (vendor
        // converts-and-shrinks in the same loop over targets, which is a bug and not reproduced here).
        let slots_snapshot = if is_defence_src {
            slots[s].clone()
        } else {
            Vec::new()
        };
        for (t, dst) in MATRIX_RESOURCES.iter().enumerate() {
            if t == s {
                continue;
            }
            // GainAs stacks on top of ConvertTo and doesn't reduce the source (:1336-1337
            // `rate = conversionRate + gainRate`).
            let gain = db
                .sum(
                    ModType::Base,
                    cfg,
                    &[ModName::from(format!("{src}GainAs{dst}"))],
                )
                .max(0.0);
            let rate = conv[s][t] + gain;
            if rate <= 0.0 {
                continue;
            }
            if is_defence_src {
                // Defence sources: slot bases move per-slot (defence targets go into the same
                // slot's bucket, subject to the target's slot-scoped factors; non-defence targets
                // go into the global converted-in amount, :1340-1352).
                for (slot, value) in &slots_snapshot {
                    if *value <= 0.0 {
                        continue;
                    }
                    let target_base = value * rate / 100.0;
                    if t < MATRIX_DEFENCE_COUNT {
                        slots[t].push((slot.clone(), target_base));
                    } else {
                        received[t] += target_base;
                    }
                }
                // The global-base portion (:1355); the Total channel propagates at the same rate (:1362-1363).
                received[t] += global_s * rate / 100.0;
                total_received[t] += total_s * rate / 100.0;
            } else {
                // Non-defence sources: rounds the global amount up with ceil (:1364-1366), same for Total (:1365).
                received[t] += (global_s * rate / 100.0).ceil();
                total_received[t] += (total_s * rate / 100.0).ceil();
            }
        }
        if is_defence_src {
            // Shrink the source once: only ConvertTo counts toward totalConversion, GainAs
            // doesn't reduce the source (:1352/:1360).
            let keep = (100.0 - total_conv[s]) / 100.0;
            for (_, value) in slots[s].iter_mut() {
                *value *= keep;
            }
            kept_global[s] = global_s * keep;
            kept_total[s] = total_s * keep;
        }
    }

    // 4) Per-slot aggregation of defence resources (:1374-1381, equivalent to the old scaled_defence_stat)
    let mut out = [0.0_f64; MATRIX_DEFENCE_COUNT];
    for (s, value) in out.iter_mut().enumerate() {
        // Global inc/more scaling name set (see [`defence_scaling_names`] for the combined-name semantics).
        let names = defence_scaling_names(MATRIX_RESOURCES[s]);
        let global_inc = db.sum_global_only(ModType::Inc, cfg, &names);
        let global_more = db.more_global_only(cfg, &names);
        // Global base = retained-after-shrinking + amounts converted in by later sources (amounts
        // received after this source was processed are not reconverted by it — serial semantics).
        let mut total = (kept_global[s] + received[s]) * (1.0 + global_inc / 100.0) * global_more;
        for (slot, slot_base) in &slots[s] {
            let slot_inc = db.sum_for_slot(ModType::Inc, cfg, &names, slot);
            let slot_more = db.more_for_slot(cfg, &names, slot);
            total +=
                slot_base * (1.0 + (global_inc + slot_inc) / 100.0) * (global_more * slot_more);
        }
        // The Total direct-add channel isn't multiplied by inc/more (vendor :1394 `… + res.totalBase`).
        total += kept_total[s] + total_received[s];
        *value = round(total);
        // Diagnostics (POBR_DBG_DEFRES=<idx>): dump a given defence resource's components (armour=0).
        if dbg_env!("POBR_DBG_DEFRES").and_then(|v| v.parse::<usize>().ok()) == Some(s) {
            let raw_base_ct = db
                .iter_mods()
                .filter(|m| {
                    m.name == ModName::from(MATRIX_RESOURCES[s]) && m.mod_type == ModType::Base
                })
                .count();
            eprintln!(
                "[POBR_DEFRES] res={} eff={} combat={} raw_base_ct={} kept_global={:.2} received={:.2} global_inc={:.2} global_more={:.4} total_conv={:.2} total_flat={:.2} slots={:?} => {:.2}",
                MATRIX_RESOURCES[s],
                cfg.mode_effective,
                cfg.mode_combat,
                raw_base_ct,
                kept_global[s],
                received[s],
                global_inc,
                global_more,
                total_conv[s],
                kept_total[s] + total_received[s],
                slots[s],
                *value
            );
        }
    }

    // Life/Mana's Total converted-in amount (total_received[3]/[4], vendor :1397
    // `NewMod("<Res>Total")` fed straight into pool calculation) currently has no output channel
    // — the only existing Total source in the data is Discipline's EnergyShieldTotal (on the
    // defence side), which is always 0 when there's no defence→Life/Mana conversion mod; wiring
    // it up will need scaled_pool to add a direct-add term (not multiplied by inc).
    DefenceResources {
        armour: out[0],
        evasion: out[1],
        energy_shield: out[2],
        extra_life: received[3],
        extra_mana: received[4],
    }
}

/// The global/slot-scoped inc/more scaling name set for a given defence stat (PoB2
/// `CalcDefence.lua` resourceList `mods`).
///
/// - `Armour`  → `[Armour, ArmourAndEvasion, Defences]`
/// - `Evasion` → `[Evasion, ArmourAndEvasion, Defences]`
/// - `EnergyShield` → `[EnergyShield, Defences]`
///
/// Note: `ArmourAndEnergyShield` / `EvasionAndEnergyShield` are **in neither set** — this
/// matches PoB2 (these combined names only apply to an item's local rolled base value; they have
/// no effect on the total when they show up globally).
fn defence_scaling_names(name: &str) -> Vec<ModName> {
    match name {
        "Armour" => vec![
            ModName::from("Armour"),
            ModName::from("ArmourAndEvasion"),
            ModName::from("Defences"),
        ],
        "Evasion" => vec![
            ModName::from("Evasion"),
            ModName::from("ArmourAndEvasion"),
            ModName::from("Defences"),
        ],
        "EnergyShield" => vec![ModName::from("EnergyShield"), ModName::from("Defences")],
        other => vec![ModName::from(other)],
    }
}

// ES Recharge (gap: es-recharge-missing)

/// The default ES recharge rate (percentage per minute), converted from
/// `character_inherent_energy_shield_recharge_rate_per_minute_% = 750`
/// (PoB2 `src/Data/Misc.lua`). 750 / 60 / 100 = 12.5%/s.
const ES_RECHARGE_RATE_PER_MINUTE_BASE: f64 = 750.0;
/// Default ES recharge start delay (seconds).
const ES_RECHARGE_DELAY_BASE: f64 = 4.0;

/// Computes the ES recharge rate and delay.
///
/// # Parameters
/// - `db` — the player's ModDb.
/// - `cfg` — the current calc config.
/// - `energy_shield` — the current final ES value (already scaled by bonuses).
/// - `zealots_oath` — whether ZealotsOath is active (ES then recovers via regen, recharge disabled).
///
/// # Basis
/// - Default rate: 750%/min (PoB2 `Misc.lua`) → 12.5%/s;
///   the `EnergyShieldRechargeRate` INC/MORE mods scale this rate.
/// - Delay: base 4 seconds; `EnergyShieldRechargeDelay` BASE (converted from 4s×(1-faster/100)
///   etc — PoB2 actually treats it as "a BASE in seconds, then a faster/100 more that shortens
///   the delay"). Here `EnergyShieldRechargeFaster` INC is used instead (>0 shortens the delay:
///   `delay / (1 + faster/100)`).
/// - `ZealotsOath` → `rate_fraction = 0` (ES relies on regen, no recharge).
///
/// Source: agent-docs/energy-shield.md §Recharge;
///       PoB2 `src/Data/Misc.lua` (constant);
///       PoB2 `src/Modules/CalcDefence.lua` EnergyShieldRecharge section.
pub fn calc_es_recharge(
    db: &ModDb,
    cfg: &CalcConfig,
    energy_shield: f64,
    zealots_oath: bool,
) -> EsRecharge {
    // ZealotsOath: ES is driven by regen, recharge is disabled (PoB2 active-defences.md §5 keystone table).
    if zealots_oath || energy_shield <= 0.0 {
        return EsRecharge {
            rate_fraction: 0.0,
            delay_seconds: ES_RECHARGE_DELAY_BASE,
        };
    }

    // Recharge rate: base 750%/min, scaled by EnergyShieldRechargeRate INC/MORE.
    let rate_inc = db.sum(
        ModType::Inc,
        cfg,
        &[ModName::from("EnergyShieldRechargeRate")],
    );
    let rate_more = db.more(cfg, &[ModName::from("EnergyShieldRechargeRate")]);
    let rate_per_min = ES_RECHARGE_RATE_PER_MINUTE_BASE * (1.0 + rate_inc / 100.0) * rate_more;
    // Convert to a per-second fraction (750%/min = 12.5%/s; divide by 100 to get a fraction).
    let rate_fraction = rate_per_min / 60.0 / 100.0;

    // Recharge delay (PoB2 CalcDefence.lua:1762-1763):
    //   rechargeBase = Override('EnergyShieldRechargeBase')
    //               or (4 + Sum('BASE','EnergyShieldRechargeFaster'))   // BASE is "seconds", added to the numerator
    //   delay = rechargeBase / (1 + Sum('INC','EnergyShieldRechargeFaster')/100)  // INC is "%", shortens the delay
    // BASE and INC are different ModTypes in different positions (numerator vs denominator), not interchangeable.
    let recharge_base = db
        .override_(cfg, ModName::from("EnergyShieldRechargeBase"))
        .unwrap_or_else(|| {
            ES_RECHARGE_DELAY_BASE
                + db.sum(
                    ModType::Base,
                    cfg,
                    &[ModName::from("EnergyShieldRechargeFaster")],
                )
        });
    let faster_inc = db.sum(
        ModType::Inc,
        cfg,
        &[ModName::from("EnergyShieldRechargeFaster")],
    );
    let delay_seconds = round(recharge_base / (1.0 + faster_inc / 100.0));

    EsRecharge {
        rate_fraction: round(rate_fraction),
        delay_seconds,
    }
}

/// The absolute per-second ES recharge amount, for panel display. `recharge.rate_fraction * energy_shield`.
pub fn es_recharge_per_second(recharge: &EsRecharge, energy_shield: f64) -> f64 {
    round(recharge.rate_fraction * energy_shield)
}

// Avoidance (gap: avoidance-ailment-missing / ehp-no-avoidance-layer)

/// The cap on avoiding "all hit damage" (PoB2 `data.misc.AvoidChanceCap = 75`).
pub const AVOID_HIT_CAP: f64 = 75.0;
/// The cap on ailment / stun avoidance (100%).
pub const AVOID_AILMENT_CAP: f64 = 100.0;

/// Computes the various avoidance chances.
///
/// # Notes
/// - `AvoidAllDamageFromHitsChance` / projectile avoidance: BASE summed then `min(_, 75)`.
/// - Ailment avoidance (stun/ignite/shock/chill/freeze/poison/bleed/all-elemental): capped at
///   100%; `<Ailment>Immune` / `ElementalAilmentImmune` flags set it straight to 100.
/// - **ES implicit stun avoidance** (PoB2 `CalcDefence.lua:2554-2557`): only when
///   `ES > totalTakenHit` **and** `EnergyShieldProtectsMana` (EB) is **absent** does
///   `notAvoidChance × 0.5` apply (halving the chance to be stunned ≡ an effective AvoidStun +50%).
///   The old implementation's "ES > 0 halves it" was too broad — vendor only grants the halving
///   when ES can absorb the entire hit, and under EB, ES protects Mana rather than the hit pool
///   and doesn't get the halving.
///   Before Track F is wired in, `total_taken_hit` is approximated by the caller using a single
///   reference-hit damage value.
/// - `ShockAvoidAppliesToElementalAilments` (Stormshroud) interaction: shock avoidance also
///   folds into the all-elemental avoidance calculation.
///
/// # Parameters
/// - `total_taken_hit` — total damage taken from a hit (PoB2 `output.totalTakenHit`, :2555).
/// - `energy_shield_protects_mana` — the EB keystone flag (:2555); the caller passes this in from
///   the C-1 `DefenceKeystones::energy_shield_protects_mana` snapshot.
///
/// Source: agent-docs/active-defences.md §3.2;
///       PoB2 `src/Modules/CalcDefence.lua` avoidance section + :2554-2558.
pub fn calc_avoidance(
    db: &ModDb,
    cfg: &CalcConfig,
    energy_shield: f64,
    total_taken_hit: f64,
    energy_shield_protects_mana: bool,
) -> AvoidanceResult {
    // Hit avoidance
    let avoid_all_raw = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("AvoidAllDamageFromHitsChance")],
    );
    let avoid_all_damage_from_hits = round(avoid_all_raw.clamp(0.0, AVOID_HIT_CAP));

    let avoid_proj_raw = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("AvoidProjectileDamageChance")],
    );
    let avoid_projectile_damage = round(avoid_proj_raw.clamp(0.0, AVOID_HIT_CAP));

    // Per-type hit avoidance (`Avoid<Type>DamageChance` BASE, e.g. Perfidy body armour's "chance
    // to Avoid <Type> Damage from Hits") — parsed correctly but previously had no consumer (a
    // dead bucket). Capped at 75% (AVOID_HIT_CAP).
    let avoid_typed_names = ["Physical", "Fire", "Cold", "Lightning", "Chaos"];
    let mut avoid_typed = [0.0; 5];
    for (i, t) in avoid_typed_names.iter().enumerate() {
        let raw = db.sum(
            ModType::Base,
            cfg,
            &[ModName::from(format!("Avoid{t}DamageChance").as_str())],
        );
        avoid_typed[i] = round(raw.clamp(0.0, AVOID_HIT_CAP));
    }

    // Ailment avoidance (capped at 100%; Immune flags set it straight to 100)

    // Stormshroud: shock avoidance also applies to all elemental ailments
    let shock_applies_to_elemental =
        db.flag(cfg, ModName::from("ShockAvoidAppliesToElementalAilments"));
    let elemental_ailment_immune = db.flag(cfg, ModName::from("ElementalAilmentImmune"));

    // Shock avoidance (used by the Stormshroud interaction; ElementalAilmentImmune also covers shock)
    let shock_immune = db.flag(cfg, ModName::from("ShockImmune")) || elemental_ailment_immune;
    let shock_avoid_raw = if shock_immune {
        100.0
    } else {
        db.sum(ModType::Base, cfg, &[ModName::from("AvoidShock")])
    };
    let avoid_shock = round(shock_avoid_raw.clamp(0.0, AVOID_AILMENT_CAP));

    let elemental_extra = if shock_applies_to_elemental {
        shock_avoid_raw
    } else {
        0.0
    };

    let avoid_elemental_base = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("AvoidElementalAilments")],
    ) + elemental_extra;

    let ignite_immune = db.flag(cfg, ModName::from("IgniteImmune")) || elemental_ailment_immune;
    let avoid_ignite_raw = if ignite_immune {
        100.0
    } else {
        db.sum(ModType::Base, cfg, &[ModName::from("AvoidIgnite")]) + avoid_elemental_base
    };
    let avoid_ignite = round(avoid_ignite_raw.clamp(0.0, AVOID_AILMENT_CAP));

    let chill_immune = db.flag(cfg, ModName::from("ChillImmune")) || elemental_ailment_immune;
    let avoid_chill_raw = if chill_immune {
        100.0
    } else {
        db.sum(ModType::Base, cfg, &[ModName::from("AvoidChill")]) + avoid_elemental_base
    };
    let avoid_chill = round(avoid_chill_raw.clamp(0.0, AVOID_AILMENT_CAP));

    let freeze_immune = db.flag(cfg, ModName::from("FreezeImmune")) || elemental_ailment_immune;
    let avoid_freeze_raw = if freeze_immune {
        100.0
    } else {
        db.sum(ModType::Base, cfg, &[ModName::from("AvoidFreeze")]) + avoid_elemental_base
    };
    let avoid_freeze = round(avoid_freeze_raw.clamp(0.0, AVOID_AILMENT_CAP));

    let poison_immune = db.flag(cfg, ModName::from("PoisonImmune"));
    let avoid_poison_raw = if poison_immune {
        100.0
    } else {
        db.sum(ModType::Base, cfg, &[ModName::from("AvoidPoison")])
    };
    let avoid_poison = round(avoid_poison_raw.clamp(0.0, AVOID_AILMENT_CAP));

    let bleed_immune = db.flag(cfg, ModName::from("BleedImmune"));
    let avoid_bleeding_raw = if bleed_immune {
        100.0
    } else {
        db.sum(ModType::Base, cfg, &[ModName::from("AvoidBleeding")])
    };
    let avoid_bleeding = round(avoid_bleeding_raw.clamp(0.0, AVOID_AILMENT_CAP));

    // Stun avoidance (includes the ES implicit 50%)
    // PoB2 CalcDefence.lua:2554-2558:
    //   notAvoidChance = StunImmune ? 0 : 100 - min(AvoidStun, 100)
    //   if ES > totalTakenHit and not EnergyShieldProtectsMana: notAvoidChance *= 0.5
    //   StunAvoidChance = 100 - notAvoidChance
    let stun_immune = db.flag(cfg, ModName::from("StunImmune"));
    let avoid_stun = if stun_immune {
        100.0
    } else {
        let stun_raw = db.sum(ModType::Base, cfg, &[ModName::from("AvoidStun")]);
        let not_avoid = (100.0 - stun_raw.min(AVOID_AILMENT_CAP)).max(0.0);
        // :2555-2557 When ES can absorb the whole hit and it's not EB, the chance to be stunned is halved.
        let effective_not_avoid = if energy_shield > total_taken_hit && !energy_shield_protects_mana
        {
            not_avoid * 0.5
        } else {
            not_avoid
        };
        round((100.0 - effective_not_avoid).clamp(0.0, AVOID_AILMENT_CAP))
    };

    AvoidanceResult {
        avoid_all_damage_from_hits,
        avoid_projectile_damage,
        avoid_typed_damage: avoid_typed,
        avoid_stun,
        avoid_ignite,
        avoid_shock,
        avoid_chill,
        avoid_freeze,
        avoid_poison,
        avoid_bleeding,
    }
}

// Evade four-way split (13-G9; PoB2 CalcDefence.lua:1394-1456)

/// The Evade four-way split result (PoB2 `EvadeChance` / `Melee|Projectile|Spell|SpellProjectileEvadeChance`,
/// CalcDefence.lua:1421-1456). Each value is a percentage (0-100, already clamped to
/// `EvadeChanceMax`/the cap).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EvadeSuite {
    /// The combined evade chance (an independent formula value when split, otherwise = melee; :1443-1449).
    pub evade_chance: f64,
    /// Melee evade chance.
    pub melee: f64,
    /// Projectile evade chance.
    pub projectile: f64,
    /// Spell evade chance.
    pub spell: f64,
    /// Spell projectile evade chance.
    pub spell_projectile: f64,
}

impl EvadeSuite {
    /// All five values set to the same number (used by the CannotEvade → 0 / AlwaysEvade → 100
    /// branches; :1421-1433).
    fn uniform(value: f64) -> Self {
        Self {
            evade_chance: value,
            melee: value,
            projectile: value,
            spell: value,
            spell_projectile: value,
        }
    }
}

/// vendor `calcs.monsterHitChance` (CalcDefence.lua:40-46): the monster's chance to hit the
/// player, as an **integer percentage** (`m_max(m_min(round(raw), 100), 5)`).
///
/// Different scale from this file's [`monster_hit_chance`] (fraction, 1e-9 precision): the evade
/// formulas consume vendor's integer-percentage intermediate value, and mixing the two produces
/// ±0.5%-level deviations, so this is implemented separately.
///
/// Edge cases: `accuracy < 0` → 5 (vendor :41-43); `evasion <= 0` → 100 (vendor's formula has a
/// numerator of 0 when evasion=0 → raw=100; this also avoids the 0/0 case when evasion=accuracy=0).
fn monster_hit_chance_pct(evasion: f64, accuracy: f64) -> f64 {
    if accuracy < 0.0 {
        return 5.0;
    }
    if evasion <= 0.0 {
        return 100.0;
    }
    let raw = (1.0 - (0.95 * evasion) / (evasion + 4.0 * accuracy)) * 100.0;
    raw.round().clamp(5.0, 100.0)
}

/// Equivalent to `calcLib.mod`: `(1 + Σinc/100) × Πmore` (vendor CalcTools.lua `calcLib.mod`).
fn scaling_mod(db: &ModDb, cfg: &CalcConfig, names: &[ModName]) -> f64 {
    (1.0 + db.sum(ModType::Inc, cfg, names) / 100.0) * db.more(cfg, names)
}

/// The Evade four-way split calculation (PoB2 CalcDefence.lua:1394-1456).
///
/// # Formulas (line-by-line)
/// - Per-type effective evasion value (:1394-1397): `<Type>Evasion = max(round(Evasion × calcLib.mod(<Type>Evasion)), 0)`
///   (integer rounding matches vendor's `round`).
/// - `evadeMax = Override(EvadeChanceMax) || EvadeChanceCap(95)` (:1436; W0.1 folds vendor's MAX
///   mod down to an Override, the consumer's clamp semantics are unchanged).
/// - Combined (:1437): `EvadeChance = 100 − (monsterHitChance(Evasion, acc) − ΣBASE EvadeChance) × enemyHitMult`.
/// - Per-type (:1438-1441): `max(0, min(evadeMax, (100 − (monsterHitChance(<Type>Evasion, acc)
///   − ΣBASE EvadeChance) × enemyHitMult) × calcLib.mod(EvadeChance, <Type>EvadeChance)))`;
///   the SpellProjectile factor name set is `EvadeChance + ProjectileEvadeChance + SpellProjectileEvadeChance` (:1441).
/// - Split decision (:1443-1448): the combined value is kept as its own independent number only
///   if melee differs from **all three** other types, otherwise combined = melee.
/// - The combined value is also `min(evadeMax)` (:1449); `UnluckyEvade` → each of the five values
///   becomes `x²/100` (:1450-1456).
/// - `CannotEvade`/enemy `CannotBeEvaded` → all 0 (:1421-1426); `AlwaysEvade` → all 100 (:1427-1433).
///
/// # Parameters
/// - `enemy_hit_mult` — the `calcLib.mod` of the enemy's `HitChance` (inc/more factor, :1435; the
///   caller reads this from the enemy ModDb, keeping this function dependent only on the player db).
/// - `enemy_cannot_be_evaded` — the enemy's `CannotBeEvaded` flag (:1421).
///
/// Note: `EnemyAccuracyDistancePenalty` (:2545-2549) depends on config input; once
/// config_interpreter is wired in, the caller pre-folds it into `enemy_accuracy` — this
/// function's formula stays unchanged.
pub fn calc_evade_suite(
    db: &ModDb,
    cfg: &CalcConfig,
    evasion: f64,
    enemy_accuracy: f64,
    enemy_hit_mult: f64,
    enemy_cannot_be_evaded: bool,
) -> EvadeSuite {
    // :1421-1426 CannotEvade / enemy CannotBeEvaded → all 0.
    if db.flag(cfg, ModName::from("CannotEvade")) || enemy_cannot_be_evaded {
        return EvadeSuite::uniform(0.0);
    }
    // :1427-1433 AlwaysEvade ("Attacks cannot Hit you") → all 100.
    if db.flag(cfg, ModName::from("AlwaysEvade")) {
        return EvadeSuite::uniform(100.0);
    }

    // :1394-1397 per-type effective evasion value (vendor rounds to an integer, floors at 0).
    let typed_evasion = |name: &str| -> f64 {
        (evasion * scaling_mod(db, cfg, &[ModName::from(name)]))
            .round()
            .max(0.0)
    };
    let melee_evasion = typed_evasion("MeleeEvasion");
    let projectile_evasion = typed_evasion("ProjectileEvasion");
    let spell_evasion = typed_evasion("SpellEvasion");
    let spell_projectile_evasion = typed_evasion("SpellProjectileEvasion");

    // :1435-1436 combined BASE and the cap.
    let evade_base = db.sum(ModType::Base, cfg, &[ModName::from("EvadeChance")]);
    let evade_max = db
        .override_(cfg, ModName::from("EvadeChanceMax"))
        .unwrap_or(cfg.constants.game().evade_chance_cap)
        .max(0.0);

    // :1438-1441 the per-type independent formula.
    let typed_chance = |type_evasion: f64, names: &[ModName]| -> f64 {
        let unscaled = 100.0
            - (monster_hit_chance_pct(type_evasion, enemy_accuracy) - evade_base) * enemy_hit_mult;
        (unscaled * scaling_mod(db, cfg, names)).clamp(0.0, evade_max)
    };
    let melee = typed_chance(
        melee_evasion,
        &[
            ModName::from("EvadeChance"),
            ModName::from("MeleeEvadeChance"),
        ],
    );
    let projectile = typed_chance(
        projectile_evasion,
        &[
            ModName::from("EvadeChance"),
            ModName::from("ProjectileEvadeChance"),
        ],
    );
    let spell = typed_chance(
        spell_evasion,
        &[
            ModName::from("EvadeChance"),
            ModName::from("SpellEvadeChance"),
        ],
    );
    let spell_projectile = typed_chance(
        spell_projectile_evasion,
        &[
            ModName::from("EvadeChance"),
            ModName::from("ProjectileEvadeChance"),
            ModName::from("SpellProjectileEvadeChance"),
        ],
    );

    // :1437 the combined independent formula (no per-type factor, no clamp to 0 — matches
    // vendor, only :1449's cap clamping applies).
    let mut evade_chance =
        100.0 - (monster_hit_chance_pct(evasion, enemy_accuracy) - evade_base) * enemy_hit_mult;
    // :1443-1448 split decision: keep the combined value only if melee differs from all three
    // other types, otherwise combined = melee.
    if !(melee != projectile && melee != spell && melee != spell_projectile) {
        evade_chance = melee;
    }
    // :1449 the combined value's cap.
    evade_chance = evade_chance.min(evade_max);

    // :1450-1456 UnluckyEvade → each value becomes x²/100.
    let unlucky = db.flag(cfg, ModName::from("UnluckyEvade"));
    let finish = |v: f64| -> f64 {
        if unlucky {
            round(v * v / 100.0)
        } else {
            round(v)
        }
    };
    EvadeSuite {
        evade_chance: finish(evade_chance),
        melee: finish(melee),
        projectile: finish(projectile),
        spell: finish(spell),
        spell_projectile: finish(spell_projectile),
    }
}

/// Track E fill orchestration (called as the last line of perform's `fill_mechanics`):
/// writes the Evade four-way split + Stun system into [`super::OutputTable`].
///
/// Enemy-side readings (accuracy / the `HitChance` factor / `CannotBeEvaded`) are pulled out
/// first, keeping `calc_evade_suite` / `calc_stun` dependent only on the player db (the pure-function convention).
///
/// Before Track F is wired in, Stun's `totalTakenHit`/`PhysicalTakenHit` are approximated by a
/// single reference-hit damage value (same source as `EhpOptions`'s `reference_hit` = life + ES;
/// the reference hit is treated as purely physical, matching ehp.rs's physical reference
/// convention); once F is wired in these switch to the real values from the pool-deduction pipeline.
///
/// Keystone toggles (CI, etc.) are passed in via the `keystones` snapshot.
pub fn fill_evade_stun(env: &mut Env, keystones: &crate::rules::DefenceKeystones) {
    let hit_names = [ModName::from("HitChance")];
    let enemy_hit_mult = (1.0 + env.enemy.mod_db.sum(ModType::Inc, &env.cfg, &hit_names) / 100.0)
        * env.enemy.mod_db.more(&env.cfg, &hit_names);
    let enemy_cannot_be_evaded = env
        .enemy
        .mod_db
        .flag(&env.cfg, ModName::from("CannotBeEvaded"));
    let enemy_accuracy = env.enemy.base.accuracy;

    let suite = calc_evade_suite(
        &env.player.mod_db,
        &env.cfg,
        env.player.output.evasion,
        enemy_accuracy,
        enemy_hit_mult,
        enemy_cannot_be_evaded,
    );
    env.player.output.evade_chance = suite.evade_chance;
    env.player.output.melee_evade_chance = suite.melee;
    env.player.output.projectile_evade_chance = suite.projectile;
    env.player.output.spell_evade_chance = suite.spell;
    env.player.output.spell_projectile_evade_chance = suite.spell_projectile;

    // Stun system (CalcDefence.lua:2525-2643; depends on fill_mechanics having already written avoid_stun)
    // Reference-hit approximation before Track F is wired in (same source as EhpOptions's reference_hit).
    let reference_hit = (env.player.output.life + env.player.output.energy_shield).max(1.0);
    let stun_inputs = super::stun::StunInputs {
        life: env.player.output.life,
        life_base_flat: env.player.base.life,
        energy_shield: env.player.output.energy_shield,
        mana: env.player.output.mana,
        total_taken_hit: reference_hit,
        physical_taken_hit: reference_hit,
        avoid_stun: env.player.output.avoid_stun,
        chaos_inoculation: keystones.chaos_inoculation,
    };
    let stun = super::stun::calc_stun(&env.player.mod_db, &env.cfg, &stun_inputs);
    env.player.output.stun_threshold = stun.threshold;
    env.player.output.self_stun_chance = stun.self_stun_chance;
    env.player.output.stun_duration = stun.stun_duration;
}

// Taken multiplier (gap: ehp-no-taken-multiplier)

/// Whether reflect's takenMult is enabled (PoB2 `CalcDefence.lua` L2248/L2281 hardcodes
/// `AnyTakenReflect = false`).
///
/// PoB2 itself leaves the reflected-damage chain inactive (comment `--this needs a rework as
/// well`). This is likewise **deferred** here: the placeholder constant is kept but the
/// `<type>ReflectedDamageTaken` chain isn't implemented, matching PoB2's current behavior. Align
/// once PoB2 upstream reworks reflected damage.
///
/// Source: PoB2 `src/Modules/CalcDefence.lua` L2248,L2266-2281 (the Reflect section + the
/// hardcoded false).
pub const ANY_TAKEN_REFLECT_ENABLED: bool = false;

/// Hit-damage source context (PoB2 `CalcDefence.lua` `hitSourceList = {"Attack","Spell"}`).
///
/// Corresponds to the `<hitType>DamageTaken` / `<type><hitType>DamageTaken` layer — only applies
/// when reading values in an attack/spell-specific context; the default (base hit context)
/// doesn't read this layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitSource {
    /// Attack source (`AttackDamageTaken` / `<type>AttackDamageTaken`).
    Attack,
    /// Spell source (`SpellDamageTaken` / `<type>SpellDamageTaken`).
    Spell,
}

impl HitSource {
    /// PoB2 ModName prefix (`"Attack"` / `"Spell"`).
    fn prefix(self) -> &'static str {
        match self {
            HitSource::Attack => "Attack",
            HitSource::Spell => "Spell",
        }
    }
}

/// Computes the hit-taken multiplier for a damage type (**base hit context**, no Attack/Spell context).
///
/// Equivalent to calling [`taken_mult_for_type_with_source`] with `None`, matching PoB2's
/// `output[<type>.."TakenHitMult"]` (L2263).
///
/// Formula: `TakenHitMult = max(0, (1 + Σinc/100) × Π(1 + more/100))`
///
/// inc sources (summed additively):
/// - `DamageTaken` (all types)
/// - `<type>DamageTaken` (per type)
/// - `DamageTakenWhenHit` (on hit)
/// - `<type>DamageTakenWhenHit` (on hit, per type)
/// - `ElementalDamageTaken` / `ElementalDamageTakenWhenHit` (if elemental)
///
/// Source: agent-docs/recovery-charges-buffs.md §4.1;
///       PoB2 `src/Modules/CalcDefence.lua` TakenHitMult section (L2250-2263).
pub fn taken_mult_for_type(db: &ModDb, cfg: &CalcConfig, damage_type: DamageType) -> f64 {
    taken_mult_for_type_with_source(db, cfg, damage_type, None)
}

/// Computes the hit-taken multiplier for a damage type, optionally layered with an Attack/Spell source context.
///
/// PoB2 `CalcDefence.lua` L2265-2269: on top of the base hit context, for each
/// `hitType ∈ {Attack,Spell}`, layer in `<hitType>DamageTaken` (e.g. `AttackDamageTaken` / `SpellDamageTaken`):
/// ```text
/// <hitType>TakenHitMult = max(0, (1 + (takenInc + Sum(INC,<hitType>DamageTaken))/100)
///                                  × takenMore × More(<hitType>DamageTaken))
/// ```
/// `source = None` degenerates to the base hit context (`<type>TakenHitMult`, no this layer).
///
/// Note: `<type><hitType>TakenHitMult` has the same value as `<hitType>TakenHitMult` in PoB2
/// (L2269), so this function doesn't distinguish the two.
///
/// Source: PoB2 `src/Modules/CalcDefence.lua` L2265-2269;
///       hitSourceList = {"Attack","Spell"} (L26).
pub fn taken_mult_for_type_with_source(
    db: &ModDb,
    cfg: &CalcConfig,
    damage_type: DamageType,
    source: Option<HitSource>,
) -> f64 {
    let type_name = damage_type_mod_prefix(damage_type);

    // Base hit bucket (base + WhenHit + Elemental*), matching PoB2's takenInc/takenMore.
    let mut inc_names = vec![
        ModName::from("DamageTaken"),
        ModName::from(format!("{type_name}DamageTaken")),
        ModName::from("DamageTakenWhenHit"),
        ModName::from(format!("{type_name}DamageTakenWhenHit")),
    ];
    if damage_type.is_elemental() {
        inc_names.push(ModName::from("ElementalDamageTaken"));
        inc_names.push(ModName::from("ElementalDamageTakenWhenHit"));
    }

    let mut more_names = vec![
        ModName::from("DamageTaken"),
        ModName::from(format!("{type_name}DamageTaken")),
        ModName::from("DamageTakenWhenHit"),
        ModName::from(format!("{type_name}DamageTakenWhenHit")),
    ];
    if damage_type.is_elemental() {
        more_names.push(ModName::from("ElementalDamageTaken"));
        more_names.push(ModName::from("ElementalDamageTakenWhenHit"));
    }

    // Attack/Spell context layer: adds in `<hitType>DamageTaken` (PoB2 L2266-2267).
    if let Some(src) = source {
        let hit_prefix = src.prefix();
        inc_names.push(ModName::from(format!("{hit_prefix}DamageTaken")));
        more_names.push(ModName::from(format!("{hit_prefix}DamageTaken")));
    }

    let inc = db.sum(ModType::Inc, cfg, &inc_names);
    let more = db.more(cfg, &more_names);
    let mult = (1.0 + inc / 100.0) * more;
    round(mult.max(0.0))
}

/// The hit-taken multiplier in PoB2's default damageCategory (`"Average"`) convention: the mean
/// of the Attack and Spell layers.
///
/// PoB2 `CalcDefence.lua` L2429-2430 (`damageCategoryConfig == "Average"`):
/// `takenMult = (<type>SpellTakenHitMult + <type>AttackTakenHitMult) / 2`.
/// Without `AttackDamageTaken`/`SpellDamageTaken` mods the two layers are equal and this
/// degenerates to the base hit context ([`taken_mult_for_type`]), so it stays consistent with
/// existing regression output.
///
/// PoE2 has removed spell suppression (`spellSuppressMult`) and deflect is rarely used
/// (`deflectMulti`), so both are treated as 1.0 and omitted, matching PoB2's default single-hit convention.
///
/// Source: PoB2 `src/Modules/CalcDefence.lua` L2013 (default `"Average"`), L2422-2430.
pub fn taken_mult_for_type_default(db: &ModDb, cfg: &CalcConfig, damage_type: DamageType) -> f64 {
    let attack = taken_mult_for_type_with_source(db, cfg, damage_type, Some(HitSource::Attack));
    let spell = taken_mult_for_type_with_source(db, cfg, damage_type, Some(HitSource::Spell));
    round((attack + spell) / 2.0)
}

/// Computes the over-time damage-taken multiplier (OverTime, as opposed to WhenHit).
///
/// Over-time damage (bleed/ignite/poison etc.) uses `DamageTaken`/`<type>DamageTaken`/`DamageTakenOverTime`
/// rather than the `WhenHit` family.
///
/// Source: agent-docs/recovery-charges-buffs.md §4.1 (the three sub-contexts: WhenHit/OverTime/Reflect);
///       PoB2 `src/Modules/CalcDefence.lua`.
pub fn taken_mult_over_time(db: &ModDb, cfg: &CalcConfig, damage_type: DamageType) -> f64 {
    let type_name = damage_type_mod_prefix(damage_type);

    let mut inc_names = vec![
        ModName::from("DamageTaken"),
        ModName::from(format!("{type_name}DamageTaken")),
        ModName::from("DamageTakenOverTime"),
        ModName::from(format!("{type_name}DamageTakenOverTime")),
    ];
    if damage_type.is_elemental() {
        inc_names.push(ModName::from("ElementalDamageTaken"));
        inc_names.push(ModName::from("ElementalDamageTakenOverTime"));
    }

    let mut more_names = vec![
        ModName::from("DamageTaken"),
        ModName::from(format!("{type_name}DamageTaken")),
        ModName::from("DamageTakenOverTime"),
        ModName::from(format!("{type_name}DamageTakenOverTime")),
    ];
    if damage_type.is_elemental() {
        more_names.push(ModName::from("ElementalDamageTaken"));
        more_names.push(ModName::from("ElementalDamageTakenOverTime"));
    }

    let inc = db.sum(ModType::Inc, cfg, &inc_names);
    let more = db.more(cfg, &more_names);
    let mult = (1.0 + inc / 100.0) * more;
    round(mult.max(0.0))
}

/// Computes the full taken-multiplier suite (WhenHit for every damage type + all OverTime).
pub fn calc_taken_multi_suite(db: &ModDb, cfg: &CalcConfig) -> TakenMultiSuite {
    TakenMultiSuite {
        physical_when_hit: taken_mult_for_type(db, cfg, DamageType::Physical),
        fire_when_hit: taken_mult_for_type(db, cfg, DamageType::Fire),
        cold_when_hit: taken_mult_for_type(db, cfg, DamageType::Cold),
        lightning_when_hit: taken_mult_for_type(db, cfg, DamageType::Lightning),
        chaos_when_hit: taken_mult_for_type(db, cfg, DamageType::Chaos),
        elemental_when_hit: {
            // Generic elemental: the global elemental contribution alone (each type already
            // includes its own share; this field only holds the pure ElementalDamageTaken contribution)
            let inc = db.sum(
                ModType::Inc,
                cfg,
                &[
                    ModName::from("ElementalDamageTaken"),
                    ModName::from("ElementalDamageTakenWhenHit"),
                ],
            );
            let more = db.more(
                cfg,
                &[
                    ModName::from("ElementalDamageTaken"),
                    ModName::from("ElementalDamageTakenWhenHit"),
                ],
            );
            round(((1.0 + inc / 100.0) * more).max(0.0))
        },
        all_over_time: {
            let inc = db.sum(
                ModType::Inc,
                cfg,
                &[
                    ModName::from("DamageTaken"),
                    ModName::from("DamageTakenOverTime"),
                ],
            );
            let more = db.more(
                cfg,
                &[
                    ModName::from("DamageTaken"),
                    ModName::from("DamageTakenOverTime"),
                ],
            );
            round(((1.0 + inc / 100.0) * more).max(0.0))
        },
    }
}

// Crit extra damage reduction (gap: crit-extra-damage-reduction-missing)

/// Computes the reduction to extra crit damage taken.
///
/// Formula (PoB2 `CalcDefence.lua`):
/// `CritExtraDamageReduction = min(Σ ReduceCritExtraDamage, 100)`
///
/// Note: only applies to the **crit-damage bonus** portion of an enemy's crit (`enemyCritDamage`),
/// not the base hit damage. At 100% this is equivalent to "takes no extra crit damage".
///
/// Source: agent-docs/active-defences.md §4;
///       PoB2 `src/Modules/CalcDefence.lua` CritExtraDamageReduction section.
pub fn calc_crit_extra_reduction(db: &ModDb, cfg: &CalcConfig) -> CritExtraReduction {
    let raw = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("ReduceCritExtraDamage")],
    );
    CritExtraReduction {
        reduction_pct: round(raw.clamp(0.0, 100.0)),
    }
}

/// Computes the enemy's crit-effect multiplier, accounting for extra-crit-damage reduction.
///
/// Formula (PoB2 `CalcDefence.lua`):
/// `EnemyCritEffect = 1 + enemyCritChance/100 * (enemyCritDamage/100) * (1 - reduction/100)`
///
/// - `enemy_crit_chance` — the enemy's crit chance (%, e.g. 5.0 = 5%).
/// - `enemy_crit_damage` — the enemy's crit-damage bonus (%, e.g. 100.0 = +100%, i.e. total damage ×2).
/// - `reduction` — [`CritExtraReduction::reduction_pct`] (0-100).
///
/// Returns the enemy's crit-weighted average damage multiplier (≥ 1.0).
pub fn enemy_crit_effect(
    enemy_crit_chance: f64,
    enemy_crit_damage: f64,
    reduction: &CritExtraReduction,
) -> f64 {
    let scale = 1.0 - reduction.reduction_pct / 100.0;
    round(1.0 + enemy_crit_chance / 100.0 * (enemy_crit_damage / 100.0) * scale)
}

/// DamageType → mod-name prefix (PoB2 ModName convention).
fn damage_type_mod_prefix(dt: DamageType) -> &'static str {
    match dt {
        DamageType::Physical => "Physical",
        DamageType::Fire => "Fire",
        DamageType::Cold => "Cold",
        DamageType::Lightning => "Lightning",
        DamageType::Chaos => "Chaos",
    }
}
