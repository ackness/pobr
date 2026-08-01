//! General-purpose PoB2 parity harness: walks `examples/demo-bd-test/builds/*/`,
//! using each build's `meta.json::player_stats` (golden values exported by
//! PoB2/Lua) as the reference, and compares them against PoBR's calculated output.
//!
//! Design goals:
//! - **Zero hardcoding, zero per-skill specialization** — the same comparison
//!   logic applies to every class/ascendancy/skill.
//! - **Baseline measurement**: doesn't hard-fail by default; prints a
//!   "PoBR vs PoB2" comparison per build plus an aggregate hit rate, serving
//!   as a live dashboard of alignment progress
//!   (`cargo test -p pobr-build --test ninja_parity -- --nocapture`).
//! - **Regression gate**: `parity_no_regression` asserts the aggregate hit
//!   rate doesn't fall below the recorded baseline (prevents changes from regressing).
//!
//! Defence/attributes are compared using PoB2's PlayerStat convention; DPS
//! (strongly tied to how complete the skill pipeline is) is reported in a
//! separate column and doesn't count toward the defensive hit rate, so an
//! incomplete offence pipeline can't mask defensive-side parity signal.
//!
//! **Default convention = `mode_effective=true`**: PoB2's main panel (i.e.
//! what the golden values are exported from) is always EFFECTIVE outside
//! CALCS mode (vendor `CalcSetup.lua:583-588`), matching golden. The panel
//! convention (`mode_effective=false`) is still guarded by
//! [`panel_mode_no_regression`], so a convention regression can't go
//! unnoticed.

use pobr_build::corpus::{CorpusLine, LineSource};
use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build_from_code};
use pobr_core::calc::{MinimalInput, OutputTable};
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};
use std::path::{Path, PathBuf};

fn builds_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/demo-bd-test/builds")
        .canonicalize()
        .expect("builds dir exists")
}

fn discover_builds() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(builds_dir())
        .expect("read builds dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir() && p.join("code.txt").exists() && p.join("meta.json").exists())
        .collect();
    dirs.sort();
    // Debug aid: POBR_ONLY_BUILD=<substring> narrows the dashboard to matching
    // build dirs so POBR_DBG_* channels stay readable. Never set in CI.
    if let Ok(filter) = std::env::var("POBR_ONLY_BUILD") {
        dirs.retain(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().contains(&filter))
        });
    }
    dirs
}

fn load_data() -> BuildData {
    // Pins the data version being checked against the golden values (decoupled
    // from the active DATA_VERSION -- the latter can advance to newer data
    // without falsely failing parity; see pobr_data::GOLDEN_PARITY_DATA_VERSION).
    let data = GameData::new(repo_data_root().join(pobr_data::GOLDEN_PARITY_DATA_VERSION));
    BuildData::load(&data).expect("load BuildData")
}

/// Reads meta.json::player_stats (PoB2 golden values).
fn golden_stats(dir: &Path) -> serde_json::Map<String, serde_json::Value> {
    let text = std::fs::read_to_string(dir.join("meta.json")).expect("read meta.json");
    // PoB2's export contains `Infinity`/`NaN` literals (invalid JSON) -- replace them with parseable placeholders before parsing.
    let sanitized = text
        .replace("-Infinity", "-1e308")
        .replace("Infinity", "1e308")
        .replace("NaN", "0");
    let json: serde_json::Value = serde_json::from_str(&sanitized).expect("parse meta.json");
    json.get("player_stats")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default()
}

fn golden(stats: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<f64> {
    stats.get(key).and_then(|v| v.as_f64())
}

/// Calculates a build under a given convention (`mode_effective`: false = panel
/// convention, true = PoB2's main-panel EFFECTIVE convention, vendor
/// `CalcSetup.lua:583-588` -- always EFFECTIVE outside CALCS mode).
fn run_build_mode(dir: &Path, data: &BuildData, mode_effective: bool) -> Option<OutputTable> {
    if std::env::var("POBR_DBG_DEFRES").is_ok() {
        eprintln!(
            "[POBR_BUILD] >>> {} (mode_effective={mode_effective})",
            dir.file_name().unwrap().to_string_lossy()
        );
    }
    let code = std::fs::read_to_string(dir.join("code.txt")).ok()?;
    let build = parse_build_from_code(code.trim()).ok()?;
    let opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective,
        extra_modifier_texts: vec![],
        ..Default::default()
    };
    calculate_with_data(&build, data, &opts).ok()
}

/// Default convention: effective.
fn run_build(dir: &Path, data: &BuildData) -> Option<OutputTable> {
    run_build_mode(dir, data, true)
}

/// A comparison column: (display label, PoB2 key, PoBR value).
struct Row {
    label: &'static str,
    golden: Option<f64>,
    pobr: f64,
}

/// After sanitizing, golden's `Infinity` becomes `1e308`; treat anything >=
/// this threshold as ∞-equivalent (for ratio purposes).
const GOLDEN_INF: f64 = 1e307;

/// ∞-equivalence check (covers both pobr's `f64::INFINITY` and golden's sanitized placeholder).
fn is_inf_like(v: f64) -> bool {
    !v.is_finite() || v >= GOLDEN_INF
}

fn ratio(pobr: f64, golden: f64) -> f64 {
    if is_inf_like(golden) {
        // Both sides ∞ -> hit (1.0); golden ∞ but pobr finite -> 0 (miss).
        if is_inf_like(pobr) { 1.0 } else { 0.0 }
    } else if golden == 0.0 {
        if pobr == 0.0 { 1.0 } else { f64::INFINITY }
    } else {
        pobr / golden
    }
}

/// The defence/attribute panel's **core columns** (the original 8-column
/// baseline used throughout W1; a subset metric guarding against dilution
/// from column expansion, per the owner's dual-metric ruling in -index §4).
fn defensive_core_rows(
    out: &OutputTable,
    g: &serde_json::Map<String, serde_json::Value>,
) -> Vec<Row> {
    vec![
        Row {
            label: "Life",
            golden: golden(g, "Life"),
            pobr: out.life,
        },
        Row {
            label: "Mana",
            golden: golden(g, "Mana"),
            pobr: out.mana,
        },
        Row {
            label: "EnergyShield",
            golden: golden(g, "EnergyShield"),
            pobr: out.energy_shield,
        },
        Row {
            label: "Armour",
            golden: golden(g, "Armour"),
            pobr: out.armour,
        },
        Row {
            label: "Evasion",
            golden: golden(g, "Evasion"),
            pobr: out.evasion,
        },
        Row {
            label: "FireResist",
            golden: golden(g, "FireResist"),
            pobr: out.fire_resistance,
        },
        Row {
            label: "ColdResist",
            golden: golden(g, "ColdResist"),
            pobr: out.cold_resistance,
        },
        Row {
            label: "LightningResist",
            golden: golden(g, "LightningResist"),
            pobr: out.lightning_resistance,
        },
    ]
}

/// Defence extension columns (expanding 8->25: the new EHP/max-hit convention
/// plus the Block/Spirit/Evade/Deflect/pool convention panels, per Track F's
/// "expand defensive_rows" checklist).
fn defensive_extended_rows(
    out: &OutputTable,
    g: &serde_json::Map<String, serde_json::Value>,
) -> Vec<Row> {
    vec![
        Row {
            label: "TotalEHP",
            golden: golden(g, "TotalEHP"),
            pobr: out.total_ehp,
        },
        Row {
            label: "PhysMaxHit",
            golden: golden(g, "PhysicalMaximumHitTaken"),
            pobr: out.physical_max_hit,
        },
        Row {
            label: "FireMaxHit",
            golden: golden(g, "FireMaximumHitTaken"),
            pobr: out.fire_max_hit,
        },
        Row {
            label: "ColdMaxHit",
            golden: golden(g, "ColdMaximumHitTaken"),
            pobr: out.cold_max_hit,
        },
        Row {
            label: "LightMaxHit",
            golden: golden(g, "LightningMaximumHitTaken"),
            pobr: out.lightning_max_hit,
        },
        Row {
            label: "ChaosMaxHit",
            golden: golden(g, "ChaosMaximumHitTaken"),
            pobr: out.chaos_max_hit,
        },
        Row {
            label: "EffBlock",
            golden: golden(g, "EffectiveBlockChance"),
            pobr: out.effective_block_chance,
        },
        Row {
            label: "EffSpellBlock",
            golden: golden(g, "EffectiveSpellBlockChance"),
            pobr: out.effective_spell_block_chance,
        },
        Row {
            label: "Spirit",
            golden: golden(g, "Spirit"),
            pobr: out.spirit,
        },
        Row {
            label: "SpiritUnres",
            golden: golden(g, "SpiritUnreserved"),
            pobr: out.spirit_unreserved,
        },
        Row {
            label: "EvadeChance",
            golden: golden(g, "EvadeChance"),
            pobr: out.evade_chance,
        },
        Row {
            label: "MeleeEvade",
            golden: golden(g, "MeleeEvadeChance"),
            pobr: out.melee_evade_chance,
        },
        Row {
            label: "LifeUnres",
            golden: golden(g, "LifeUnreserved"),
            pobr: out.life_unreserved,
        },
        Row {
            label: "ManaUnres",
            golden: golden(g, "ManaUnreserved"),
            pobr: out.mana_unreserved,
        },
        Row {
            label: "ESRecoveryCap",
            golden: golden(g, "EnergyShieldRecoveryCap"),
            pobr: out.energy_shield_recovery_cap,
        },
        Row {
            label: "PhysDR",
            golden: golden(g, "PhysicalDamageReduction"),
            pobr: out.physical_damage_reduction,
        },
        Row {
            label: "DeflectChance",
            golden: golden(g, "DeflectChance"),
            pobr: out.deflect_chance,
        },
    ]
}

/// Full defence column set = the 8 core columns + 17 extension columns.
fn defensive_rows(out: &OutputTable, g: &serde_json::Map<String, serde_json::Value>) -> Vec<Row> {
    let mut rows = defensive_core_rows(out, g);
    rows.extend(defensive_extended_rows(out, g));
    rows
}

/// Offence columns (strongly tied to skill-pipeline completeness, reported separately).
fn offensive_rows(out: &OutputTable, g: &serde_json::Map<String, serde_json::Value>) -> Vec<Row> {
    // (k2 note): vendor's real identity is `TotalDPS = AverageDamage × Speed ×
    // dpsMultiplier × quantityMultiplier` (CalcOffence.lua:4407) -- golden's
    // `AverageDamage` excludes the end-factor, while PoBR's `dps` (same
    // convention as golden's `TotalDPS`) includes it. The old readout
    // `dps / action_rate` carries a structural bias for grenade-type builds
    // (measured ×1.5 for deadeye, ×1.65 for gemling, ×1.02 for twister).
    // Solves for the end-factor using golden's own identity and folds PoBR's
    // readout back to the same convention as AverageDamage (a harness readout
    // fix, zero calc behaviour change).
    let golden_end_factor = match (
        golden(g, "TotalDPS"),
        golden(g, "AverageDamage"),
        golden(g, "Speed"),
    ) {
        (Some(td), Some(ad), Some(sp)) if td.is_finite() && (ad * sp).abs() > f64::EPSILON => {
            let f = td / (ad * sp);
            if f.is_finite() && f > 0.0 { f } else { 1.0 }
        }
        _ => 1.0,
    };
    vec![
        Row {
            label: "CritChance",
            golden: golden(g, "CritChance"),
            pobr: out.crit_chance * 100.0,
        },
        Row {
            label: "CritMultiplier",
            golden: golden(g, "CritMultiplier"),
            pobr: out.crit_multiplier,
        },
        Row {
            label: "Speed",
            golden: golden(g, "Speed"),
            pobr: out.action_rate,
        },
        Row {
            label: "AverageDamage",
            golden: golden(g, "AverageDamage"),
            // PoB2's identity `TotalDPS = AverageDamage × Speed × end-factor`
            // (golden's average damage already includes hit chance/crit/enemy
            // mitigation, but not the end-factor). PoBR's side uses the same
            // identity to take `dps / action_rate / end-factor`; the old value
            // `total_hit_avg` (player-side, unmitigated, excludes hit chance)
            // was structurally mismatched against golden under the effective convention.
            pobr: if out.action_rate > 0.0 {
                out.dps / out.action_rate / golden_end_factor
            } else {
                0.0
            },
        },
        Row {
            label: "TotalDPS",
            golden: golden(g, "TotalDPS"),
            pobr: out.dps,
        },
    ]
}

/// The DoT combined-family columns (an expansion: the end-stage combined
/// panel for skill DoT + ailment DoT, PoB2's `TotalDotDPS`/`WithDotDPS`/
/// `CombinedDPS`, CalcOffence.lua:6093-6234). Strongly tied to skill-pipeline
/// completeness, counted independently of the existing 5 offence columns (a
/// separate baseline constant for the new columns, doesn't dilute/move
/// BASELINE_OFF_*). The golden keys already exist in meta.json (WithDotDPS is
/// only exported for pure-DoT builds, e.g. essence-drain).
fn dot_rows(out: &OutputTable, g: &serde_json::Map<String, serde_json::Value>) -> Vec<Row> {
    vec![
        Row {
            label: "TotalDotDPS",
            golden: golden(g, "TotalDotDPS"),
            pobr: out.total_dot_dps,
        },
        Row {
            label: "WithDotDPS",
            golden: golden(g, "WithDotDPS"),
            pobr: out.with_dot_dps,
        },
        Row {
            label: "CombinedDPS",
            golden: golden(g, "CombinedDPS"),
            pobr: out.combined_dps,
        },
    ]
}

const TOL: f64 = 0.05; // hit = relative error < 5%

/// Hit-count stats for a set of comparison columns: number of 5% hits, number of 10% near-hits, total comparisons.
#[derive(Default, Clone, Copy)]
struct Tally {
    hit5: usize,
    hit10: usize,
    total: usize,
}

impl Tally {
    fn add(&mut self, other: Tally) {
        self.hit5 += other.hit5;
        self.hit10 += other.hit10;
        self.total += other.total;
    }
}

const TOL10: f64 = 0.10; // near = relative error < 10% (a supplementary progress-visibility metric)

/// Counts only (no printing): used by the regression gate [`parity_no_regression`].
fn tally_rows(rows: &[Row]) -> Tally {
    let mut t = Tally::default();
    for r in rows {
        if let Some(gv) = r.golden {
            let rt = ratio(r.pobr, gv);
            t.total += 1;
            if (rt - 1.0).abs() < TOL {
                t.hit5 += 1;
            }
            if (rt - 1.0).abs() < TOL10 {
                t.hit10 += 1;
            }
        }
    }
    t
}

/// Prints the per-stat comparison table and returns the hit aggregate (reuses [`tally_rows`]'s counting logic).
fn print_rows(rows: &[Row]) -> Tally {
    let fmt = |v: f64| -> String {
        if is_inf_like(v) {
            "inf".into()
        } else {
            format!("{v:.2}")
        }
    };
    for r in rows {
        match r.golden {
            Some(gv) => {
                let rt = ratio(r.pobr, gv);
                let mark = if (rt - 1.0).abs() < TOL {
                    "✓"
                } else if (rt - 1.0).abs() < TOL10 {
                    "~"
                } else {
                    " "
                };
                eprintln!(
                    "  {mark} {:<16}{:>14}{:>14}{:>9.2}x",
                    r.label,
                    fmt(r.pobr),
                    fmt(gv),
                    rt
                );
            }
            None => eprintln!("    {:<16}{:>14.2}{:>14}{:>10}", r.label, r.pobr, "—", "—"),
        }
    }
    tally_rows(rows)
}

/// Iterates every build to compute the defence/offence/DoT hit aggregates.
/// `verbose` controls whether the comparison table is printed per build,
/// `mode_effective` controls the calc convention (the default gate uses
/// effective, the panel guard uses false).
/// Returns `(core-8 defence Tally, full 25-column defence Tally, offence
/// Tally, 3-column DoT Tally, names of builds that failed to parse/calc)`.
fn compute_tallies_mode(
    verbose: bool,
    mode_effective: bool,
) -> (Tally, Tally, Tally, Tally, Vec<String>) {
    let data = load_data();
    let builds = discover_builds();
    assert!(!builds.is_empty(), "no builds discovered");

    let mut def_core = Tally::default();
    let mut def = Tally::default();
    let mut off = Tally::default();
    let mut dot = Tally::default();
    let mut failed_parse = Vec::new();

    for dir in &builds {
        let name = dir.file_name().unwrap().to_string_lossy();
        let g = golden_stats(dir);
        let Some(out) = run_build_mode(dir, &data, mode_effective) else {
            failed_parse.push(name.to_string());
            if verbose {
                eprintln!("\n##### {name} :: PARSE/CALC FAILED #####");
            }
            continue;
        };
        let (def_rows, off_rows, dot_rows) = (
            defensive_rows(&out, &g),
            offensive_rows(&out, &g),
            dot_rows(&out, &g),
        );
        def_core.add(tally_rows(&defensive_core_rows(&out, &g)));
        if verbose {
            eprintln!("\n##### {name} #####");
            eprintln!(
                "  {:<18}{:>14}{:>14}{:>10}",
                "stat", "PoBR", "PoB2", "ratio"
            );
            eprintln!("  -- defensive --");
            def.add(print_rows(&def_rows));
            eprintln!("  -- offensive --");
            off.add(print_rows(&off_rows));
            eprintln!("  -- dot --");
            dot.add(print_rows(&dot_rows));
        } else {
            def.add(tally_rows(&def_rows));
            off.add(tally_rows(&off_rows));
            dot.add(tally_rows(&dot_rows));
        }
    }
    (def_core, def, off, dot, failed_parse)
}

/// Default-convention (effective) aggregate, the entry point for the main gate/report.
fn compute_tallies(verbose: bool) -> (Tally, Tally, Tally, Tally, Vec<String>) {
    compute_tallies_mode(verbose, true)
}

/// The recorded parity baseline (hit counts) — the regression gate's lower
/// bound. **Only raise it once a change is confirmed to improve overall
/// parity**, never lower it (prevents changes from silently regressing).
/// Corresponds to the ninja_parity output at the time of the commit.
///
/// Re-recorded for column expansion:
/// - `DEF_CORE`: the old 8-column subset (a guard metric against dilution
///   from column expansion; lower bound 111 frozen throughout);
/// - `DEF`: the full 25 columns after expansion (denominator = the total
///   number of golden-comparable items).
///
/// **Reviewed exceptions**:
/// OFF_HIT5 23->22 -- deadeye-explosive-grenade's TotalDPS regressed from a
/// Legacy "over-count masking an under-count" false hit (1.02x) to the real
/// 0.77x (Multishot -25% less `sup_dex.lua:3154-3156` and LightningPen +30
/// `SkillStatMap.lua:929-931`, both correct fixes).
///
/// +Re-recorded at merge (merge commit): measured on the code after merging
/// (statmap switch + quality + support gating) with (pool deduction + EHP
/// convention + 25-column expansion + finishing moves 1-3).
/// Defence 369->374 (83.1%, both branches' improvements stacked) / @10%
/// 385->390; core 130 (unchanged, 90.3%) / @10% 132->133; offence 27
/// (unchanged) / @10% 32->33. See the merge commit message for the
/// comparison against both branches' baselines.
///
/// **Re-recorded for the effective-convention switch** (a dedicated baseline
/// commit, explicitly reviewed): default convention panel->effective
/// (aligning with golden), defence's 425 rows unchanged value-for-value;
/// offence @5% 27->26, @10% 33->35.
/// **Reviewed exception (-1 @5%)**: smith-of-kitava's CritChance 1.00x->0.93x --
/// golden `HitChance`=100 (PoB2's player accuracy fully clears the cap) while
/// PoBR's accuracy aggregation undershoots (≈1015 vs 1438, gear/passive
/// accuracy mods and local weapon accuracy aren't in the aggregation, logged),
/// and under effective the crit's secondary hit check (vendor
/// CalcOffence.lua:3700) amplifies this gap. The panel-convention level is
/// still held at 27/35 by [`panel_mode_no_regression`] (PANEL_OFF_*).
// **Re-recorded for Mageblood legacies (Phase 1 #1, +17 @5% core-8
// 118->135)**: 9/18 fixtures wear full Mageblood, but the `LegacyOf*` BASE +
// `MagebloodEquipped` flag were never expanded into armour/evasion/resistances
// (declared but never built in env_finalize.rs). Implemented vendor
// CalcPerform.lua:66-142's legacies table + :1502-1528's application logic
// (stacks × duplicate amplifies globalEffect × floor) + the `legacy of (%w+)`
// handler (dynamic mod name) + the MagesLegacyEffect implicit (already in the
// special_vendor batch). Armour/evasion/resistance gaps across multiple
// builds converge in one shot -- titan Armour (Basalt INC 219 =
// floor(1.46×150)) / ice-shot Evasion (Jade BASE 2000 + Stibnite INC 150),
// etc. -- and the three canaries (physical_armour_block/
// cold_projectile_evasion_es/evasion_melee) are un-ignored.
// **Re-recorded for Virtuous Barrier Life name normalization (+1 @5% core-8
// 135->136)**: the Gemling ascendancy buff's per-Mote Life INC
// (`gem_barrier_red_grants_maximum_life_+%` -> 24% = 2×12 StrengthMote) used
// to be mapped by stat_map_engine to vendor's name `Life`, landing in a dead
// bucket -- PoBR's life-pool aggregation name is `MaximumLife` (Armour/
// Evasion/EnergyShield don't have this problem since their canonical names
// already match vendor's). After normalizing `Life`->`MaximumLife`, gemling
// Life goes 0.79x->0.96x, flipping 8 columns (including
// TotalEHP/5×MaxHit/LifeUnres) to correct.
// **Re-recorded for item ES recalculation (+2 @5% core-8 136->138)**: per-slot
// item ES changed from "trusting the item text's display line" to PoB2's
// convention "recalculate from the base DB:
// (esBase+flat)×(1+localInc/100)×(1+quality/100)" (Item.lua:1994-1996; the
// display line lags across data versions) -- titan ES 41->55, stormweaver ES
// 986->1120, each with ESRecoveryCap following. See
// calc_orchestrator/defence.rs::item_rolled_defence.
// **Re-recorded for 0.5.4b #4 Communion/LowLife + Voices (+2 @5% core-8
// 138->140)**: huntress-ritualist SpiritUnres -13.00x->1.00x / LifeUnres
// 12.74x->1.01x -- once Atziri's Communion's Spirit->Life reservation
// conversion (LifeReservePercentPerSpirit, vendor CalcDefence.lua:248-254) is
// wired in, both columns flip to correct; abyssal-lich (also wearing
// Communion) SpiritUnres inf->1.00x has the same root cause. See
// buffs.rs::spirit_reservation_modifiers's conversion branch.
// **Re-recorded for #14 defensive long-tail triage (+2 core-8 142->144/144 =
// 100%)**: abyssal-lich Life (LifeConvertToEnergyShield pool deduction, 0_5
// tree's Enhanced Barrier) + smith Armour (connected-notable multiplier +
// StrRequirements snapshot) flip to correct. See #14's individual fix commits
// and docs/adapting-to-0.5.4b.md §#14 for details.
const BASELINE_DEF_CORE_HIT5: usize = 144; // After #14's long-tail triage: 144/144 (pre-existing #7-3/4: 142; Communion: 140; ItemES: 138; Barrier-Life: 136; Mageblood: 135; migration baseline: 118; 0.5.0=139)
// **Re-recorded for the per-socket-filled fix (+1 @5%/@10%)**: gemling-legionnaire's
// body armour Morior Invictus's `+14 to Spirit per Socket filled` (×5 sockets)
// wired in via a `RunesSocketedIn{SlotName}` Multiplier -> Spirit 180->250
// (0.72x->1.00x, flips to correct). See collect.rs::filter_parseable's gate +
// the legacy/engine per-socket suffix + ingest's {SlotName} substitution +
// per_slot_socket_multipliers pre-fill for details.
// **Re-recorded for the charm base buff fix (+3 @5% / +2 @10%)**:
// huntress-ritualist-bow-shot's Sunburst Ruby Charm's (base `Ruby Charm`)
// inherent buff `+25% to Fire Resistance` is folded into the CharmBuff
// payload via base_items.charm_buff (a vendor-extracted overlay) -> FireResist
// 12->37 (0.32x->1.00x), with FireMaxHit 0.73x->1.00x and TotalEHP
// 0.93x->1.00x following.
// **Re-recorded for the Disciple of Varashta ES->armour fix (+2 @5% / +2
// @10%)**: the Sacred Rituals ascendancy notable (tree node 56857), "N% of
// your current Energy Shield is added to your Armour for determining your
// Physical Damage Reduction from Armour" -> EnergyShieldAppliesTo
// PhysicalDamageTaken=60, adding a 60% ES-borrow term to taken.rs's
// effective_applied_armour -> PhysDR 0.08x->1.03x (PhysMaxHit follows to a
// hit). sorceress-disciple-of-varashta-comet.
// **Re-recorded for the Gemling Virtuous Barrier per-Mote ascendancy buff fix
// (+7 @5% / +7 @10%)**: the ascendancy notable "Essence of Virtue" (tree node
// 11641) grants the skill Virtuous Barrier, whose buff gives an INC scaled by
// attribute Mote count (Armour/Evasion/EnergyShield ×DexterityMoteSkillCount,
// Life ×StrengthMote, etc.). Two-part fix: ① stat_map_engine's buff-domain
// allowlist gains Armour/Evasion/EnergyShield/Life/LifeRegen (the barrier stat
// was already in the data, just needed to be let through); ② inject_per_x_multipliers
// computes Attribute-Mote per vendor CalcSetup.lua:1766-1781 (base 3/3/3 +
// single-attribute gems +2 / multi-attribute gems +1 each) and provisions the
// three multipliers. mercenary-gemling-legionnaire-explosive-grenade: Armour
// 0.70x->1.00x, Evasion 0.81x->1.00x, EnergyShield 0.72x->1.00x, with
// PhysDR/per-type MaxHit/EHP following downstream.
// **Re-recorded for the essence-drain taken+Total dual fix (+7 @5% / +7 @10%,
// both fixes must be merged together)**:
// ① The verb-fronted form of "Take 30% less Damage" (tree node 28153, Phased
//   Form) wires into DamageTaken MORE -30 (taken_mult 0.7, matches the
//   oracle's AfterReductionTakenHitMulti);
// ② The Discipline aura buff switches to the `EnergyShieldTotal` direct-add
//   channel (vendor CalcDefence.lua:1331/:1394 -- **not multiplied by
//   inc/more**; the old mapping wrongly folded it into the EnergyShield
//   bucket, where it picked up the global 570% inc -> ES over-computed by
//   1.13x). Added a Total channel to defence.rs's matrix (read/conversion
//   propagation/shrink-residual/direct-add).
// Combined, sorceress-chronomancer-essence-drain flips seven columns to
// correct (ES/ESRecoveryCap 1.13x->1.03x, four MaxHit columns +TotalEHP
// 0.79x->1.03x). Warning: merging ② alone would make MaxHit worse
// (0.79->0.72, since the Total fix shrinks ES while the taken gap remains) --
// the two commits must ship in the same PR to prevent a split.
// **Re-recorded for the Spirit reservation dual-mechanism fix (+8 @5% / +7 @10%)**:
// ① Gem quality reservation efficiency (vendor CalcDefence.lua:251
//   `/(1+efficiency/100)`, data = the `base_reservation_efficiency_+%` slope
//   ×q in overlay/gem_quality_stats.json) -- a cross-build common gap, flips
//   the SpiritUnres column for seven builds (ember/frost-bomb/flicker/
//   monk-twister/pathfinder/stormweaver/detonate-dead);
// ② Blasphemy per-curse reservation (:273-284
//   `blasphemy_base_spirit_reservation_per_socketed_curse` = 60 × the number
//   of same-group AppliesCurse skills, **scaled and rounded once, then ×
//   count**) -- essence-drain 60->55×3=165, SpiritUnres 28.5x->1.00x flips
//   exactly to correct.
// Remaining: druid-comet 1.26x/coiling (totem-form reservation for Spell
// Totem + the ReservationEfficiency mod family), wolf-pack (the companion
// pipeline).
// **Re-recorded for domain-scoped reservation efficiency + Ancestral Bond
// totem reservation (+2 @5% / +3 @10%)**:
// ① SkillTypes bitmask u64->[u64;5] (320 bits, expresses the entire domain
//   including Meta=122/SummonsTotem=25 etc.; Debug/canonical keep the high
//   bits all-zero to preserve the old format -> cache stays byte-stable);
// ② Domain-scoped efficiency mods ("Meta Skills have N% increased
//   Reservation Efficiency" -> ReservationEfficiency INC + a SkillTypes tag,
//   consumed per-gem via cfg) + **deleted** calc_spirit_reservation's
//   aggregation-side global efficiency division (which double-applied
//   alongside the injection side -- the root cause of frost-bomb's panel 148
//   vs injected 166, an 18-point gap);
// ③ Ancestral Bond's "Totems reserve 75 Spirit each" -> an AncestralBond FLAG
//   + ExtraSpirit 75 + SkillType(SummonsTotem) (matches run-parsemod's
//   two-mod output), with SummonsTotem×flag entering the reservation loop
//   (CalcDefence.lua:197).
// druid-comet SpiritUnres 209/209 flips exactly to correct, frost-bomb 136/138
// (0.99x) flips to correct, disciple follows (efficiency). coiling/wolf-pack/
// gemling still miss (companion pipeline / Blasphemy ExtraSpirit interaction
// still to be audited).
// **Re-recorded for the aura magnitude multiplier bucket (+5 @5% / +6 @10% /
// core-8 +1)**: aligns buff_pass's aura multiplier bucket with vendor
// CalcPerform.lua:2204-2205 -- ① per-skill cfg (BuffSpec carries the source
// effect's type bits, so a domain mod like "Banner Skills have N% increased
// Aura Magnitudes" = AuraEffect INC + SkillTypes(Banner=89) only hits the
// matching aura); ② an independent `Magnitude` multiplier bucket ("Aura
// Skills have N% increased Magnitudes" = Magnitude INC + SkillTypes(Aura),
// forming its own (1+Σinc/100) separately from the AuraEffect bucket before
// multiplying together -- wolf-pack's Defiance Banner
// 30×(1+39%)×(1+88%)=78.4 against vendor's 78). Both parsers for the three
// mod shapes land (legacy parse_scoped_buff_magnitude + 3 overlay entries).
// wolf-pack Armour 0.71->0.98✓/EvadeChance·MeleeEvade->0.95✓/PhysDR->0.96✓,
// huntress-spirit-walker ChaosMaxHit 0.88->1.00✓ (a small positive spillover
// from an aura-point mod).
// **Re-recorded for the Tactician "A Solid Plan" reservation MORE bucket (+1
// @5%/+1 @10%)**: "Persistent Buffs have 50% less Reservation" (tree 15044)
// -> `Reserved MORE -50` gated by a Persistent+Buff AND of two tags (vendor
// ModParser.lua:1339's tagList); spirit_reservation_modifiers consumes
// `SpiritReserved`/`Reserved` inc/more + the efficiency MORE (the full
// formula at CalcDefence.lua:240-252). wolf-pack SpiritUnres -210->21 closes
// exactly (per-skill oracle reconciliation: the banner's own 10% efficiency
// was already covered by the existing quality path; the Wolf Pack companion
// only carries Persistent, not Buff, so under AND semantics it doesn't take
// the ×0.5 -- both sides agree). The engine-side template's skill_type_bit
// gained the missing Persistent/Buff bit mapping (the root cause of pre_flag
// dropping the tag).
// **Re-recorded for the BeenHitRecently condition suffix (+1 @5%/+1 @10%,
// core-8 +1)**: "… if you have/haven't been Hit Recently" (vendor
// ModParser.lua:1955/:1961) -> the Condition `BeenHitRecently` (negated
// form), whose cfg truth value already flows through the generic passthrough
// for config `conditionBeenHitRecently`. wolf-pack's tree node "Backup Plan"
// (53853, 0_5 tree) has its 40% Evasion conditional mod activate -- Evasion
// 14618->16824.47 against vendor's 16824 closes exactly (oracle
// defenceModList reconciliation: INC 165->205), EHP 0.54->0.59 spills over positively.
// **Stale-import correction (re-recorded 416->415 after PR#40 merged)**: #40
// was a pre-B3 branch -- its legacy.rs mod plus the 416 baseline were both
// measured in a pre-B3 world; after the B3 gate switched to the engine, that
// mod was already taking effect via the mod_parser_rules data channel (this
// is the same slot as B3's commit taking core-8 from 138->139), so wolf-pack
// Evasion was already 1.00x ✓ before #40 merged. Merging #40 changed zero
// behaviour (the per-build diff across all 18 builds is empty); 416 was a
// double-count. Current measured value is 415 (@10% 432 and core-8 139 match
// reality exactly, so it's kept).
// **Re-recorded for Mageblood legacies (Phase 1 #1, +50 @5% 343->393 / +56
// @10% 361->417)**: see the note above BASELINE_DEF_CORE_HIT5; armour/
// evasion/resistance columns converge across multiple builds.
// **Re-recorded for Virtuous Barrier Life name normalization (+8 @5%
// 393->401 / +8 @10% 417->425)**: see the note above BASELINE_DEF_CORE_HIT5;
// gemling's 8 columns (Life/TotalEHP/5×MaxHit/LifeUnres) flip to correct.
// **Re-recorded for item ES recalculation (+4 @5% 401->405 / +3 @10%
// 425->428)**: titan+stormweaver each flip ES+ESRecoveryCap to correct; see
// the note above BASELINE_DEF_CORE_HIT5.
// **Re-recorded for the Refraction buff's EvasionGainAsDeflection (+2 @5%
// 405->407 / +1 @10% 428->429)**: support Refraction I/II's Refractive
// Plating buff payload
// (`support_tempered_valour_deflection_rating_%_of_evasion_rating` BASE 20)
// wires in through the player buff allowlist (stat_map_engine), wolf-pack
// DeflectChance 0.80x->1.00x (@5%+@10%), pathfinder 0.93x->1.00x (@5%).
// **Re-recorded for the Refraction buff's ArmourAppliesTo<El>DamageTaken (+3
// @5% 407->410)**: the same buff's
// `support_tempered_valour_%_armour_to_apply_to_elemental_damage` payload
// (three BASE 30 entries) wires in through the same player buff allowlist,
// consumed by `calc::taken::armour_applies_pct` (tree 84 + buff 30 = 114%,
// oracle-pinned FireEffectiveAppliedArmour 21181.2). wolf-pack
// Fire/Cold/LightMaxHit 0.94x->0.96x (@5% flips to correct), TotalEHP
// 0.81x->0.88x (the remainder = Armour's own 0.98x gap + ChaosMaxHit 0.87x +
// Life 1.11x, all unrelated to this channel). No change @10%.
// **Re-recorded for 0.5.4b #4 Communion/LowLife + Voices (+3 @5% 410->413 /
// +5 @10% 429->434)**: core-8's SpiritUnres/LifeUnres flip to correct (see the
// note above BASELINE_DEF_CORE_HIT5) + abyssal-lich EnergyShield 0.93x->0.98x
// (the Voices sinister jewel's ES mod was recovered).
// **Re-recorded for #12's companion allies layer (+6 @5% 425->431 / +5 @10%
// 434->439)**: the companion damage-taken-first layer lands
// (TakenFromCompanionBeforeYou buff allowlisted + TotalCompanionLife summed
// and injected + pool_setup's companion AllyLayer; downstream: minion level
// picks up `+N to Level of Minion Skills`, minion base life switches to
// monsterAllyLifeTable, and Loyalty's -30% more minion life injects via the
// minion-domain statmap channel). spirit-walker-twister flips all 6 columns
// to correct (5×MaxHit + TotalEHP 0.89-0.90x->1.00x close exactly, from Bear
// Companion + Wild Protector's inherent 10% taken); wolf-pack's MaxHit family
// 0.82x->0.90x / ChaosMaxHit 0.72x->0.80x / TotalEHP 0.74x->0.83x (the pool
// side, 3817 vs oracle's 3826.67 = 0.9975, is already closed; the remainder =
// a uniform ~10% gap in the per-type taken multipliers + Mana 761.2 vs 770,
// both unrelated to the companion layer).
// **Re-recorded for #13's defensive-residual pinpoint fixes (+6 @5% 431->437
// / +6 @10% 439->445)**: three root causes --
// ① wolf-pack's uniform ~10% gap in per-type taken multipliers = the
//    enemy-Intimidated base condition pair (vendor CalcSetup.lua:73-77
//    `Damage INC -10 / DamageTaken INC 10 if Intimidated`) was never built:
//    the body armour mod "Enemies in your Presence are Intimidated"'s
//    enemy-side `Condition:Intimidated` flag was already in the enemy db but
//    had no consumer. setup_enemy now injects the condition pair +
//    env_finalize bridges flag->cfg `EnemyIntimidated` + the orchestrator
//    defaults `EnemyInPresence` to true (vendor CalcPerform.lua:524) ->
//    `<X>EnemyDamageMult` 0.9 takes effect (the max-hit end divisor
//    :3734-3771 + EHP damage-taken), wolf-pack's 5×MaxHit 0.80-0.90x->1.00x,
//    TotalEHP 0.83x->1.00x, PhysDR exactly 68.03.
// ② wolf-pack Mana 761.2 vs 770 = The Adorned's "97% increased Effect of
//    Jewel Socket Passive Skills containing Corrupted Magic Jewels" was never
//    built: the corrupted magic jewels (Rallying Ruby ×6, enchant
//    +Int/+Dex/+chaos res) mods weren't scaled by 1.97 (vendor
//    CalcSetup.lua:944-948/:1342-1347, ScaleAddList =
//    trunc(round(v×s,2))). orchestrator's stage_inject_jewels now scales
//    after parsing and injects -> Int 135->139 -> Mana exactly 770 (with the
//    ChaosMaxHit tail-gap closing along with it).
// ③ titan Armour 0.985x = three Runeforged-base items (buffed in 0.5.4b) had
//    a stale armour display line: item_rolled_defence's "always recalculate
//    from a known base" was expanded from ES-only to all three defences
//    (vendor Item.lua:1994-1996 + the round convention) -- gloves 96->101 /
//    helmet 192->284 / boots 58->100, Gear:Armour 6100->6239 = vendor,
//    closing titan's Armour/ES/5×MaxHit exactly; pathfinder Evasion
//    0.98x->1.00x and twister DeflectChance 0.97x->1.00x also become exact.
// **Re-recorded for #14's defensive long-tail triage (+13 @5% 431->444 / +5
// @10% 439->444)**: five clusters closed --
// ① PhysDR rounding (vendor :2402's armourReduction integer variant): 2 slots
//    for ember/deadeye;
// ② Life-pool ConvertTo deduction (CalcDefence.lua:92): 2 slots for
//    abyssal-lich Life/LifeUnres;
// ③ Blasphemy per-curse folded into baseFlat with a single round (:229-239):
//    1 slot for essence-drain SpiritUnres; ④ the altQualityStats channel
//    (gated by GemlingQuality, CalcTools.lua:147-152): 1 slot for gemling
//    SpiritUnres; ⑤ Smith's connected-notable multiplier +
//    StrRequirementsOn<slot> snapshot (CalcSetup.lua:840/CalcPerform.
//    lua:1848-1857): 5 slots including smith Armour+4×MaxHit+TotalEHP; ⑥ EHP's
//    average block switched to the four-way mean (:1067, no longer missing
//    SpellProjectileBlock=max(spell,proj)): 2 slots for smith+titan TotalEHP.
//    The remaining 6 slots are all wolf-pack (#13's territory).
const BASELINE_DEF_HIT5: usize = 450; // #13+#14 merged, measured 450/450 = 100% (#13 alone: 437, wolf-pack fully cleared; #14 alone: 444, long tail 12 slots + four-way block; migration baseline: 343; 0.5.0=415)
const BASELINE_DEF_HIT10: usize = 450; // #13+#14 merged, measured 450/450 = 100% (migration baseline: 361; 0.5.0=432)
// **Re-recorded for expanding additional granted effects (+3 @10%)**: a gem's
// additionalGrantedEffectId1..N (a foreign key in overlay/gem_effects.json --
// e.g. a banner's buff-side effect: the primary slot is the reservation-side
// ReservationPlayer, and the buff-side <X>BannerPlayer, an Aura, is in the
// additional slot) is now expanded in buff_skill_specs into an independent
// effect that participates in aura/curse/debuff classification. wolf-pack:
// Defiance Banner's Armour/Evasion MORE 30 (gated by Condition:BannerPlanted,
// now let through by a config-tagless FLAG bridge) activates -- Armour
// 0.55->0.71, Evasion 0.49->0.63, three MaxHit columns 0.89->0.91 (@10% flips
// to correct), EHP 0.28->0.37. Reaching 1.00x still needs banner valour
// scaling (AuraEffect MORE per-resource × Multiplier:BannerValour, vendor
// :1186/:2783 -- see the companion-pipeline roadmap memory). Dread Banner's
// `..._additional_maximum_all_elemental_resistances_%_to_apply` static
// mapping was removed to match vendor's convention.
// **Cumulative re-record for the offence parity fix cluster (Onslaught +
// CI->FullLife + MultiplierThreshold, three fixes merged together)**:
// - Onslaught phantom (removed item.rs's `parse_granted_buff_flag`; PoB2's
//   ModParser returns unsupported for `Grants Onslaught during effect`):
//   detonate-dead/coiling/flicker's Speed was +20% too high -> now returns
//   to 1.00x.
// - The CI->FullLife bridge (vendor CalcDefence.lua:123-126: a CI build is
//   always at full life): flicker's (CI) "while on Full Life" damage bonus
//   now activates -> AvgDamage 0.90x->0.99x, all five components hit.
// - MultiplierThreshold:enemyDistance ("... against enemies within/further
//   than N metres" wired up, including a narrow allowance in collect.rs's
//   pre-filter gate): monk-twister's node 5802 "Stand and Deliver" and other
//   within-metres nodes now activate -> monk-twister's CritMult/Avg/TotalDPS
//   flip to hits, with huntress-twister/titan-shield-wall following.
// The three fixes' builds don't overlap and their gains stack; measured
// after merging all three (master 62/70 -> 70/73).
//
// **Re-recorded for the radius-jewel × weapon-set interaction fix (gemling
// CritChance, warning: already reverted in #15)**: this used to fold
// inactive-weapon-group nodes back into the radius-grant geometry (golden was
// 10.30 = 6×+7 at the time). After re-sampling golden for 0.5.4b, real vendor
// measurement (oracle Tabulate) showed the grant is gated by the **target
// node's** `Condition:WeaponSet<N>` (CalcSetup.lua:222-223) -- an inactive
// group's grant has zero net effect -> the correct value is 8.55 = 1×+7, so
// the mechanism was reverted wholesale (see #15's item ① above BASELINE_OFF_HIT5).
// **Re-recorded for the Mageblood Diamond crit name normalization (+2 @5%
// 39->41 / +3 @10% 47->50)**: Mageblood's LegacyOfDiamond injects vendor's
// name `CritChance` INC, but calc::crit reads `CriticalStrikeChance` (PoBR's
// canonical name) -- an un-translated injection skips the parser's
// translate_vendor_name and lands in a dead bucket (same class of bug as
// Virtuous Barrier's Life->MaximumLife). After switching the table to
// `CriticalStrikeChance`, blood-mage CritChance goes 0.79x->0.96x, ember
// CritChance+CritMultiplier hit 1.00x (InevitableCrit's crit-mult penalty
// resolves together with the pre-effective-crit fix), and crit/DPS rise for
// all three Diamond builds. See calc/mageblood.rs.
// **Re-recorded for the Mageblood Silver Speed name normalization (+4 @5%
// 41->45 / +4 @10% 50->54)**: the same dead-bucket class -- LegacyOfSilver
// injects vendor's bare `Speed` INC, but PoBR's speed bucket is named
// `SkillSpeed` (SPEED_BUCKET, covers both attack and cast). After the fix,
// ember/monk-twister/smith/titan's Speed columns flip to correct and DPS
// rises (ember 0.68x->0.77x, monk-twister ->0.88x). See
// calc/mageblood.rs::LegacyOfSilver.
// **Re-recorded for refreshing the Mana multiplier after blood-mage pool
// conversion (off +1 @5% 45->46 / @10% 54->55; dot +2 @5% 9->11 / @10%
// 11->13)**: after perform's defence-resource conversion (Eldritch Battery
// ES->Mana) recalculates mana_pool and writes back cfg.stats["Mana"], it
// forgot to refresh cfg.multipliers["Mana"] (read by per-100-max-Mana mods
// like Arcane Intensity) -> a pool-conversion build's mana scaling used the
// stale pre-conversion value. After the fix, blood-mage's SpellDamage INC
// goes 39->105, TotalDPS 0.74x->0.87x, and its DoT base rises along with it.
// See perform.rs.
// **Re-recorded for 0.5.4b #4 Communion/LowLife + Voices (off +6 @5% 46->52 /
// +4 @10% 55->59)**: two new 0.5.4b mechanics stack on real builds into a
// per-build DPS cluster (the gap map's "~0.60x" entries):
// 1. Atziri's Communion's Spirit->Life reservation conversion (vendor
//    CalcDefence.lua:248-254) -> a heavily-reserved build automatically goes
//    Low Life (:335-350, unreserved <= 35%) -> the "while on Low Life" damage
//    bonus family unlocks (ritualist: tree +60 and the Direstrike buff's +70
//    attack INC; oracle's damageModList pins the gap at exactly 130 INC).
// 2. Voices' "Allocates 2 Sinister Jewel sockets" -> sinister-socket jewels
//    are now counted (recovers ritualist's crit chance/mult 13/37 INC, both
//    columns close exactly).
// huntress-ritualist TotalDPS 0.68x->0.99x (AvgDamage/CritChance/CritMultiplier
// flip along with it), witch-abyssal-lich TotalDPS 0.62x->0.91x (Speed
// 0.91x->1.00x, CritChance 0.98x, CritMultiplier 0.75x->0.97x, where
// Speed/CritChance flip @5% but DPS doesn't come back into the column).
// **Re-recorded for the 0.5.4b #4 grenade-phrase unblock (off +4 @5% 52->56 /
// +3 @10% 59->62)**: vendor 0.22.0's ModParser gem-name registration loop
// added a `not grantedEffect.fromItem` exclusion (ModParser.lua:6423) --
// `MeleeGrenadeLauncherPlayer` (name "Grenade", fromItem) no longer steals
// the skillNameList registration, so the `grenade` / `for grenade skills`
// phrases go back to matching the live `SkillType.Grenade` tag (confirmed
// both ways via run-parsemod). Reverted the extraction-side dead entries/lazy
// rewrites from the PR#53 era (0.21 semantics) + regenerated
// mod_parser_rules/parsed_mods. deadeye's 3×15 CDR tree mods (oracle's
// extraModList pins the source to Tree:21077/354/48429) now activate: deadeye
// Speed 0.65x->1.00x (flips to correct), TotalDPS 0.53x->0.83x; gemling's
// Speed/AvgDamage/TotalDPS/CombinedDPS all flip to correct at 1.00-1.04x.
// **Re-recorded for the 0.5.4b #5 Blazing Critical global fire buff (off +2
// @5% 56->58 / +2 @10% 62->64)**: 0.22.0 added a GlobalEffect/Buff tag to
// `support_blazing_crits_gain_%_fire_damage_with_attacks_on_critical_hit`
// (sup_int.lua:959) -- the 15% `DamageGainAsFire` (Attack +
// Condition:CritRecently) went from a dead mod to a global player buff. Two
// wiring changes: stat_map_engine's player buff allowlist + support_buff_specs'
// gating target gained the additional granted effect (Charged Staff's hidden
// Attack additional effect, ChargedStaffShockwavePlayer, is the actual
// compatible host for Blazing Critical). monk-twister's
// AverageDamage/TotalDPS flip both columns 0.60x->0.96x; flicker's TotalDPS
// 0.76x->0.84x and spirit-walker's 0.78x->0.89x converge but don't reach the
// column (the remainder is the squared propagation of the attack AvgDamage
// family's existing gap, not a dot-side mechanism).
// **Re-recorded for the 0.5.4b #6 attack AvgDamage family push (off +13 @5%
// 58->71 / +9 @10% 64->73)**: five general-purpose consumer-site fixes
// stack, bringing the whole family (smith/titan/flicker/monk-twister/
// spirit-walker/pathfinder/ritualist/gemling) into the 5% band:
// 1. Bifurcate's crit-damage conditional probability (vendor :3823-3846
//    `conditionalBifurcateChance = (PreBifurcate²/100)/CritChance`; PoBR had
//    mis-ported this as unconditional pre²/10000 -- old and new vendor use
//    the same formula, so this was a pre-existing bug) + weapons' flag-less
//    crit-damage mods converted to per-hand conditions (Item.lua:1954-1961,
//    **new in 0.22.0**: CritMultiplier added to the conversion list):
//    spirit-walker/pathfinder CritMult 0.94x->1.00x/0.99x, titan CritMult
//    1.20x->1.00x (oracle-pinned 6.02->5.00, exactly matching golden).
// 2. Feeding the enemyDistance placeholder into skillDist
//    (CalcActiveSkill.lua:671, **new in 0.22.0**: a configPlaceholder
//    fallback): Close Combat's 30% MORE now activates via ramp(20)=0.6 ->
//    flicker 0.838x->0.99x, smith gains its Close Combat segment.
// 3. PerStat statList support (ModParser.lua:1631 "per 75 armour and evasion
//    on equipped shield" -> a `|`-composed Multiplier var sum): smith/titan's
//    shield-defence-scaled tree notable (Tree:27687, oracle-pinned 88 INC)
//    now activates.
// 4. Hybrid mana->life cost + per-LifeCost mods (Atalui's Bloodletting:
//    `base_skill_cost_life_instead_of_mana_%` 100 + PerStat{stat=LifeCost,
//    div=20,limit=40,limitTotal}, vendor :2067/:2090-2104): smith gains +30%
//    DamageGainAsPhysical (oracle LifeCost 309 -> floor(309/20)=15 -> 30).
// 5. The crit fast-path's per-leg damage-taken blend (vendor :4395; enemy
//    armour DR depends on the single-hit amount, and the crit leg computes DR
//    using the post-crit hit): spirit-walker 0.94x->0.98x, blood-mage/
//    abyssal-lich converge along with it (0.87x->0.88x / 0.91x->0.93x,
//    don't reach the column).
// Per build: smith 0.575x->0.996x, titan 0.874x->0.978x, flicker
// 0.838x->1.003x, spirit-walker 0.892x->0.980x, monk-twister 0.958x->0.980x,
// pathfinder 0.904x->0.963x, ritualist 0.989x->1.000x. Remaining misses:
// deadeye 0.832x (a pre-0.5.4b per-hit shortfall), blood-mage 0.880x /
// abyssal-lich 0.926x (a Mageblood mod-family gap, a separate Phase 1 item --
// oracle pins blood-mage as missing `INC CritChance 107 'Mageblood'`),
// frost-bomb 0.661x (unchanged across both golden versions, a pre-existing
// cooldown-DPS gap).
// (pre-existing #7-1) frost-bomb TotalDPS 0.66x->1.00x flips to correct (off
// @5/@10 each +1): the Archmage buff's `DamageGainAsLightning` (BASE 4/100
// Mana -> 80% gain-as, act_int.lua:229-231) + two curse-chain gaps (Ethereal
// Whip's level lookup wasn't picking up +8 spell skill levels: -58->-66;
// missing a skill-local CurseEffect segment: Heightened Curse +25 + EW
// quality +10 -> enemy resist 9->-7, per-source oracle
// enemyMitigation pin).
// **Re-recorded for 0.5.4b #8 Zarokh's Gift anoint socket (off +3 @5% 73->76
// / +1 @10% 75->76)**: the "blood-mage missing `INC CritChance 107
// 'Mageblood'`" pin left over from #6 was actually a downstream symptom:
// both the Mageblood effect table and the `MagesLegacyEffect` parsing were
// already correct (per-mod comparison matches the oracle); the real root
// cause = both builds' amulet anoint `{enchant}Allocates Zarokh's Gift`
// wasn't treating the named jewel socket node 11184 as allocated (vendor
// PassiveSpec.lua:1106-1114's sockets-name-match fallback), so the jewels in
// that socket (blood-mage: Pandemonium Ornament -- CritChance INC 24 +
// CritMult INC 25/28; abyssal: an ES/defence jewel in the same slot) were
// dropped entirely. See xml_build.rs::NAMED_SOCKETS_0_5 for the fix.
// blood-mage TotalDPS 0.880x->1.00x (CritChance 88.5->92.1, CritMult
// 5.34->5.87, both exact), abyssal-lich TotalDPS 0.926x->1.00x
// (CritChance/CritMult 1.00x; ES 12124->12437 vs golden 12434, MaxHit's five
// columns + TotalEHP 0.97-0.98x->1.00x -- the def columns were already
// within the 5% band, so the def count doesn't change).
// Across all 18 builds, only these two builds' per-slot diffs change.
// **#15: the last three slots zeroed out (off 78->80, dot 36->37, offence/dot
// at full marks)**:
// ① gemling CritChance 1.20x->1.00x: a radius jewel's grant no longer lands
//    on an inactive weapon-group node -- vendor tags **every** mod on an
//    allocMode!=0 node (including jewel grants) with `Condition:WeaponSet<N>`
//    (CalcSetup.lua:222-223, the node's own gate takes priority over the
//    jewel-source branch :224-227), so an inactive group's grant has zero net
//    effect; oracle's critModList shows only 1 entry, `7 @ Tree:32763`
//    (the earlier "gate by jewel allocMode" reading in 75a348e was wrong and
//    has been reverted wholesale). The same jewel's Small grant (Crossbow
//    damage) converges along with it -> AvgDamage/TotalDPS 1.04x->0.97x
//    (flips direction, still within the band).
// ② gemling TotalDotDPS 1.10x->0.95x (moves into the band): the ignite chain
//    absorbed the crit/jewel overestimate's propagation; after the fix, the
//    residual -4.9% (952 vs 1001) decomposes into ignite chance 0.0962 vs
//    0.098636 (0.9754) × stack potential 0.2603 vs 0.26691 (0.9752) -- both
//    are proportional to hit fire damage, and 0.9754² ≈ 0.951 matches the dot
//    residual bit-for-bit, i.e. it's entirely downstream of AvgDamage's
//    existing 0.97x undershoot (previously masked by the 6× jewel grant).
// ③ wolf-pack Speed 1.43x->1.00x: main-skill-selection fallback -- when the
//    group pointed to by mainSocketGroup has no attack/spell candidate (Wolf
//    Pack = Minion+Companion), it no longer falls back to scanning other
//    groups (it used to wrongly pick the Blasphemy group's Temporal Chains,
//    a 0.7s cast -> 1.43), and instead uses the group's own mainActiveSkill
//    selection (vendor's socketGroupSkillList has no damage filter); also
//    fixed the speed bucket: a non-attack, non-spell skill no longer takes
//    Attack/CastSpeed INC (matching vendor's ModFlag matching semantics), so
//    Wolf Pack no longer wrongly picks up the weapon's `12% reduced Attack
//    Speed` (0.88->1.00). Zero regression across all 18 builds' per-slot
//    diffs (all hits).
const BASELINE_OFF_HIT5: usize = 80; // #15 full marks 80/80 (78 after #10; 76 after #8; 73 after merging #6+#7; migration baseline: 39)
const BASELINE_OFF_HIT10: usize = 80; // #15 full marks 80/80 (78 after #10; 76 after #8; migration baseline: 47; 0.5.0=74)

/// Independent baseline for the DoT three columns (TotalDotDPS/WithDotDPS/
/// CombinedDPS), measured when they were added (a separate constant for the
/// new columns, doesn't touch existing BASELINE_OFF_*). Hits 3 = wolf-pack's
/// double zero-hit (TotalDotDPS/CombinedDPS golden=0) + essence-drain
/// TotalDotDPS at 1.0000; denominator 37 = 18 builds × (TotalDotDPS +
/// CombinedDPS) + essence-drain's WithDotDPS.
///
/// **Reviewed exceptions ×2 (two false hits revealed by fixes, combined -1
/// @5% / -2 @10%)**:
/// 1. druid-oracle-comet TotalDotDPS 1.08x->1.17x -- the old 1.08x was a
///    false hit: a missing isSwitchable tree variant (druid 6898 was wrongly
///    using the base version, missing "Gain 5% of Damage as Extra Damage of a
///    random Element") happened to partially offset an existing ignite
///    overestimate. After fixing the variant (parsing/expansion matches
///    vendor CalcOffence.lua:1175-1200's convention: split evenly n/3 across
///    the three elements), the real ~1.17x deviation is exposed. **Follow-up**:
///    the 1.17x overestimate's root cause (an ailment stack-rate source
///    over-recording at 2.54x) has been fixed -> 0.45x; the remaining
///    undershoot's per-factor attribution belongs to crit magnitude/curse
///    duration/secondary-skill debuff lines (closed by a per-factor product).
/// 2. deadeye grenade CombinedDPS 1.02x -- an accidental hit: the old
///    "attack-speed throughput compensation" approximation overestimated
///    Speed by ×1.95, which formed a double-count of throughput together
///    with GrenadeActivateTwice's ×1.5, which happened to offset the per-hit
///    undershoot (0.52x). After switching to a cooldown-governed rate per
///    vendor CalcOffence.lua:2852-2856 (Speed 1.00x ✓, entering the off
///    column), the double count disappears.
///    **Progress**: the grenade gem-level gating has been removed (the
///    level chain double-confirmed via oracle: deadeye 27=vendor, gemling
///    24=vendor) + the Paragon oil-anoint quality is wired up,
///    CombinedDPS 0.52x -> 0.82x converges but doesn't come back into the
///    column. The remaining per-hit ~0.82x gap is still open (including the
///    "attack/spell area damage" deferred phrase -- its enablement
///    precondition, "after the cooldown-line fix", is now satisfied, so it
///    belongs to the parser line).
// Raised 17->20 / 21->24 (wired in damaging-ailment magnitude's "with
// Critical Hits" conditional mod: added parser stripping of the crit suffix
// + `ailment_scoped_cfg` setting CriticalStrike=true, matching vendor
// dotCfg's `skillCond["CriticalStrike"]=true`, CalcOffence.lua:5006).
// huntress poison 0.58x->1.04x, bleed 0.64x->1.11x, both TotalDotDPS/CombinedDPS
// columns become hits.
// **Cumulative re-record for the offence fix cluster's dot columns**: flicker's
// (CI->FullLife) CombinedDPS 0.90x->0.99x, monk-twister /
// titan-shield-wall's (MultiplierThreshold) CombinedDPS flip to hits
// following hit. Measured after merging all three fixes (master 22/26 -> 26/28).
// **Honest downgrade 26->25 / 28->27 (gain-as fallback fix, 2026-07-06)**:
// deadeye ignite's TotalDotDPS 0.97x✓ was a double-error cancellation -- the
// old gain-as base fallback overestimated the fire component by ~1.30x,
// which happened to compensate for PoBR's own ignite chain's ~0.69x
// undershoot (vendor's IgniteChance 2.5%/StackPotential/RollAverage formula
// chain wasn't aligned, logged for the next slice). After fixing the fire
// base, this slot honestly shows up as 0.69x✗. Same commit: offence/defence
// metrics held steady across the board (off 71/80, def 415/450, core-8
// 139/144, and this holds even after unfreezing deadeye's two frozen INC
// rows); across all 18 builds, only deadeye's own dot slot changes.
// **Re-recorded 25->26 / 27->28 (ailment stacks × dpsMultiplier, same day)**:
// vendor :3878 folds the `DPS` multiplier bucket into skillData.dpsMultiplier
// before the ailment segment, so :5046's ailmentStacks picks it up (Payload's
// second detonation ×1.5); PoBR now folds dps_end_factors into ctx.speed from
// the same source as TotalDPS (:3880's hitRate has the same semantics).
// deadeye ignite recovers 0.69x->1.04x✓; gemling dot 0.77x->1.16x
// (miss->miss doesn't count; its underlying ~1.27x overestimate is a
// separate gemling multi-factor gap); the other 16 builds are unchanged per-slot.
// **Raised 26->27 / 28->29 (grenade-phrase skillNameList pre-emption fix,
// 2026-07-06)**: at runtime vendor strips the bare `grenade` phrase into an
// inert SkillName{"Grenade"} (always fails to match = zero effect, confirmed
// both via ModCache and an oracle probe), but PoBR's old flag_phrase produced
// SkillTypes(Grenade), which actually took effect -> gemling/deadeye's
// all-types inc over-applied by +60. After rewriting the payload + unfreezing
// gemling's frozen row (both sides now agree on the dps end-factor 1.65):
// gemling's AvgDmg/TotalDPS/dot three columns close exactly at 1.00x (dot
// 0.77x->1.00x enters the column), deadeye's remaining ~2% dissipates from
// the same root cause (AvgDmg/dot both 1.00x). Across all 18 builds' per-slot
// diffs, only gemling's dot slot flips to correct.
// **Honest downgrade 27->26 / 29->28 (blood-mage curse shortfall unfrozen,
// 2026-07-07)**: unfroze "magnitudes of curses you inflict are zero"
// (Coiling Whisper) -- the whole curse mechanism chain has been verified to
// match vendor (5 curse slots occupied / ignore limit=99 / per-Curse gain-as
// ×5 via Multiplier{CurseOnEnemy}). The frozen blood-mage dot's 1.01x✓ was a
// double-error cancellation: the curse was wrongly taking full effect (the
// item explicitly says zero) which happened to compensate for an existing
// per-hit/dot undershoot. After unfreezing + fixing the same-group support's
// granted level (Chaos Mastery +1, L30->31, exactly matching vendor),
// TotalDPS 0.73x->0.84x, dot 0.54x->0.70x converge but don't come back into
// the column; the remaining ~16% per-hit undershoot is blood-mage's own root
// cause (same pattern as deadeye/gemling), and the baseline is re-recorded
// after the fix. Across all 18 builds' per-slot diffs, only blood-mage's
// three slots change. Only the legacy `split` remains on the frozen list.
// **Re-recorded for the bloodmage mana-mult fix (dot +2 @5% 9->11 / @10%
// 11->13)**: see the note above BASELINE_OFF_HIT5 -- the mana scaling raises
// blood-mage's spell DoT base, flipping its TotalDotDPS/CombinedDPS to correct.
// **Re-recorded for 0.5.4b #4 Communion/LowLife + Voices (dot +2 @5% 11->13 /
// +5 @10% 13->18)**: ritualist TotalDotDPS 0.60x->1.00x / CombinedDPS
// 0.63x->0.99x (LowLife's damage bonus + the sinister jewel's
// damaging-ailment-magnitude mod simultaneously raise the bleed/poison base);
// abyssal-lich's dot columns converge into the @10% band following the hit
// side.
// **Re-recorded for the 0.5.4b #4 grenade-phrase unblock (dot +1 @5% 13->14 /
// +1 @10% 18->19)**: gemling CombinedDPS 0.65x->1.04x flips to correct (see
// the note above BASELINE_OFF_HIT5).
// **Re-recorded for 0.5.4b #5 Blazing Critical (dot +2 @5% 14->16 / +2 @10%
// 19->21)**: the ignite fire source scales up quadratically with the global
// 15% DamageGainAsFire (chance ∝ fire/threshold × magnitude ∝ fire):
// monk-twister's TotalDotDPS 0.44x->0.98x + CombinedDPS 0.60x->0.96x flip to
// correct; flicker's dot 0.05x->0.72x, spirit-walker's 0.23x->0.87x converge
// but don't reach the column (the remainder is the same squared propagation
// of a hit-side pre-existing gap, not a dot-side mechanism). Remaining dot
// misses, per slot: deadeye 0.69x = hit 0.83x², blood-mage 0.79x≈hit 0.87x²,
// titan/abyssal/pathfinder 0.91-0.94x converge along with hit; smith's 0.20x
// has a dot-specific residual = Infernal Cry's uptime-scaled
// DamageGainAsFire 12% isn't modeled (old and new vendor agree on this
// value, a pre-existing gap not a 0.5.4b item); frost-bomb 0.87x /
// essence-drain's WithDotDPS 1.36x are unchanged across both golden versions
// (pre-existing); gemling's dot overestimate at 1.10x is the downstream
// propagation of a small hit-side fire/crit overestimate (per-component
// oracle: fire 1.03x × stacks 1.05x × crit aliasing).
// **Re-recorded for the 0.5.4b #6 AvgDamage family's dot-column follow-through
// (dot +9 @5% 16->25 / +6 @10% 21->27)**: ignite ∝ fire-source², so once the
// hit side's whole family closes, the dot columns automatically follow to
// correct (flicker 0.72x->near 0.87✓, spirit-walker 0.87x->0.98x✓,
// titan/pathfinder/monk-twister's CombinedDPS flip to correct along with
// hit). Remaining misses: smith TotalDotDPS 0.40x (= fire-source ratio
// 0.63² -- Infernal Cry's uptime-scaled DamageGainAsFire 12.04% isn't
// modeled, a pre-existing warcry-uptime mechanism gap where old and new
// vendor agree, see the BASELINE_OFF_HIT5 note); deadeye 0.69x = hit 0.83x²;
// blood-mage 0.79x (Mageblood); gemling 1.10x overestimate (pre-existing).
// (pre-existing #7-1) dot-column migration from the frost-bomb fix:
// CombinedDPS 0.66x->1.00x enters the column (@5/@10 each +1),
// TotalDotDPS 0.87x->1.07x enters @10%; druid-oracle-comet's
// TotalDotDPS 1.04x->1.06x exits @5% (downstream of EW getting stronger --
// vendor-side druid's EW was never actually applied to enemy resistMods, a
// curse-slot/priority difference, oracle-pinned; PoBR was applying it, so a
// pre-existing overestimate gets amplified by 2%, to be tracked separately).
// Net @5 change: 0; net @10 change: +2.
// **Re-recorded for 0.5.4b #8 Zarokh's Gift anoint socket (dot +4 @5%
// 27->31 / +2 @10% 31->33)**: blood-mage TotalDotDPS 0.79x->1.00x +
// CombinedDPS 0.88x->1.00x, abyssal-lich TotalDotDPS 0.94x->1.00x +
// CombinedDPS 0.93x->1.00x -- once the hit-side crit closes, the
// ignite/DoT base follows to correct (see #8's note above BASELINE_OFF_HIT5).

// **Re-recorded for pre-existing #9's warcry-uptime machinery (dot +1 @10%
// 31->32)**: Infernal Cry's uptime-scaled `DamageGainAsFire`
// (CalcOffence.lua:3229-3256, pobr-core::calc::warcry) landed, bringing smith
// TotalDotDPS 0.40x->1.06x into the @10% band (uptime/castTime/cooldown match
// the oracle bit-for-bit: 19.4116%/0.544218/6.27; gain 62×uptime = 12.035,
// bit-exact against vendor's "Uptime Scaled Infernal Cry" entry). The
// remaining +6% = smith's pre-existing +2% hit-side overestimate (previously
// masked by the missing gain's offset, AverageDamage 0.996->1.02), amplified
// through ignite ∝ fire-source² -- a hit-side pre-existing item, not a
// warcry mechanism. titan follows suit: TotalDotDPS 0.98->1.01, TotalDPS
// 0.98->1.05 (also a previously-masked pre-existing overestimate being
// exposed, still within the @5% band). The other 16 builds have no warcry,
// unchanged value-for-value.
// **Re-recorded for pre-existing #11's Blasphemy support half-wiring (dot +2
// @5% 31->33)**: support gating was extended to gems' additional granted
// effects (the gem_effects foreign key); Blasphemy's SupportBlasphemyPlayer
// (`support_blasphemy_curse_effect_+%_final` -> `CurseEffect MORE -41`@L19)
// now enters the curse's local multiplier bucket -- Temporal Chains'
// applied value -13->-8 (matches vendor bit-for-bit: mult 0.55->0.3245,
// (1+10%)×0.59×0.5boss), ignite's debuffDurationMult 1/0.87->1/0.92 follows
// to correct: druid TotalDotDPS 1.06x->1.00x (182.40 vs 182.60), monk
// frost-bomb 1.07x->0.99x (4.53 vs 4.58); witch-lich dips slightly but stays
// within band (21659->21621, still 1.00x). Curse-slot ownership is unchanged
// per build (druid's single slot still goes to Temporal Chains, EW still
// doesn't get a slot).
const BASELINE_DOT_HIT5: usize = 37; // #15 full marks 37/37 (gemling TotalDotDPS enters the band following the crit fix; #10+#11 merged: 36; migration baseline: 9; 0.5.0=26)
const BASELINE_DOT_HIT10: usize = 37; // #15 full marks 37/37 (#10+#11 merged: 36; migration baseline: 11; 0.5.0=28)

/// Baseline guarding the panel convention (`mode_effective=false`): prevents
/// a convention regression from going unnoticed (defence is identical
/// value-for-value between effective and panel, so only offence needs
/// guarding). Measured at the switch commit.
///
/// **Reviewed exception (-1 @10%)**: witch-abyssal-lich-detonate-dead's panel
/// TotalDPS 1.09x->1.12x -- wiring in the `CritInPast8Sec` phrase family
/// (vendor ModParser.lua:1904-1906, confirmed applied by vendor via oracle)
/// pushed the panel convention's pre-existing 9% over-count (panel has no
/// enemy mitigation) past the 10% band edge. The same change is a pure
/// convergence for the effective main convention (TotalDPS 0.81x->0.83x, the
/// main baseline 41/47 doesn't regress; the dot column twister at 0.96x
/// newly enters the band, 3->4). The panel side will be re-recorded once the
/// effective damage-mitigation line closes DD's over-count root cause.
/// Same commit: panel @5% 35->36 (titan enters the band); the lower bound is
/// kept conservative and not raised.
/// **Re-recorded for full SkillType enumeration (data-driven A1, panel +8
/// @5% / +6 @10%)**: the cfg side's `skill_type_bits` switched from a
/// hand-maintained allowlist (a dozen-odd bits for Attack/Spell/…) to a full
/// enumeration bitset (`SkillTypes::from_pob2_name`, generated from vendor
/// Global.lua's 290-name table); the tag side (template.rs / special_mod.rs)
/// was fully enumerated in the same commit -- a batch of `ModTag::SkillTypes`
/// domain mods (Area/Projectile/Grenade etc.) start matching correctly under
/// the panel convention. The effective main convention's
/// defence/offence/dot baselines hold steady value-for-value (a pure
/// panel-side convergence).
const PANEL_OFF_HIT5: usize = 45; // #15 measured 45 (gemling CritChance + wolf-pack Speed/DPS converge together on the panel); #6+#7 combined: 42; migration baseline: 27; 0.5.0=44
const PANEL_OFF_HIT10: usize = 47; // #15 measured 47 (same as above); #6+#7 combined: 43; migration baseline: 30; 0.5.0=46

/// Regression gate: the aggregate hit count must not fall below the recorded baseline ([`BASELINE_*`]). CI gate, prevents changes from regressing parity.
#[test]
fn parity_no_regression() {
    let (def_core, def, off, dot, failed) = compute_tallies(false);
    assert!(failed.is_empty(), "builds failed to parse/calc: {failed:?}");
    // One half of the owner's dual-metric ruling: the old 8-column subset must be >= 111 (guards against "column-expansion dilution" masking a regression).
    assert!(
        def_core.hit5 >= BASELINE_DEF_CORE_HIT5,
        "defensive core-8 @5% regressed: {} < baseline {BASELINE_DEF_CORE_HIT5}",
        def_core.hit5
    );
    assert!(
        def.hit5 >= BASELINE_DEF_HIT5,
        "defensive @5% regressed: {} < baseline {BASELINE_DEF_HIT5}",
        def.hit5
    );
    assert!(
        def.hit10 >= BASELINE_DEF_HIT10,
        "defensive @10% regressed: {} < baseline {BASELINE_DEF_HIT10}",
        def.hit10
    );
    assert!(
        off.hit5 >= BASELINE_OFF_HIT5,
        "offensive @5% regressed: {} < baseline {BASELINE_OFF_HIT5}",
        off.hit5
    );
    assert!(
        off.hit10 >= BASELINE_OFF_HIT10,
        "offensive @10% regressed: {} < baseline {BASELINE_OFF_HIT10}",
        off.hit10
    );
    assert!(
        dot.hit5 >= BASELINE_DOT_HIT5,
        "dot @5% regressed: {} < baseline {BASELINE_DOT_HIT5}",
        dot.hit5
    );
    assert!(
        dot.hit10 >= BASELINE_DOT_HIT10,
        "dot @10% regressed: {} < baseline {BASELINE_DOT_HIT10}",
        dot.hit10
    );
}

/// Panel-convention guard: the offence aggregate under `mode_effective=false`
/// must not fall below the level measured at the switch
/// ([`PANEL_OFF_HIT5`]/[`PANEL_OFF_HIT10`]). Defence is identical
/// value-for-value to effective, so it's covered by the main gate. Prevents a
/// regression in the convention switch's upstream wiring from going unnoticed.
#[test]
fn panel_mode_no_regression() {
    // The DoT three columns are only guarded by the effective main gate (the panel convention has no independent golden values, so no separate guard is set up).
    let (_, _, off, _dot, failed) = compute_tallies_mode(false, false);
    assert!(failed.is_empty(), "builds failed to parse/calc: {failed:?}");
    assert!(
        off.hit5 >= PANEL_OFF_HIT5,
        "panel offensive @5% regressed: {} < baseline {PANEL_OFF_HIT5}",
        off.hit5
    );
    assert!(
        off.hit10 >= PANEL_OFF_HIT10,
        "panel offensive @10% regressed: {} < baseline {PANEL_OFF_HIT10}",
        off.hit10
    );
}

/// Main baseline report: prints the defence + offence comparison per build and summarizes the aggregate hit rate.
#[test]
fn parity_baseline_report() {
    let (def_core, def, off, dot, failed_parse) = compute_tallies(true);
    let builds = discover_builds();

    eprintln!(
        "\n================ PARITY SUMMARY (tol {:.0}%) ================",
        TOL * 100.0
    );
    eprintln!(
        "builds: {} ({} parse/calc failed)",
        builds.len(),
        failed_parse.len()
    );
    if !failed_parse.is_empty() {
        eprintln!("  failed: {}", failed_parse.join(", "));
    }
    let pct = |n: usize, d: usize| 100.0 * n as f64 / d.max(1) as f64;
    eprintln!(
        "defensive parity (25 cols): {}/{} = {:.1}% @5%  |  {}/{} = {:.1}% @10%",
        def.hit5,
        def.total,
        pct(def.hit5, def.total),
        def.hit10,
        def.total,
        pct(def.hit10, def.total),
    );
    eprintln!(
        "defensive core-8 subset:    {}/{} = {:.1}% @5%  |  {}/{} = {:.1}% @10%",
        def_core.hit5,
        def_core.total,
        pct(def_core.hit5, def_core.total),
        def_core.hit10,
        def_core.total,
        pct(def_core.hit10, def_core.total),
    );
    eprintln!(
        "offensive parity: {}/{} = {:.1}% @5%  |  {}/{} = {:.1}% @10%",
        off.hit5,
        off.total,
        pct(off.hit5, off.total),
        off.hit10,
        off.total,
        pct(off.hit10, off.total),
    );
    eprintln!(
        "dot parity (3 cols): {}/{} = {:.1}% @5%  |  {}/{} = {:.1}% @10%",
        dot.hit5,
        dot.total,
        pct(dot.hit5, dot.total),
        dot.hit10,
        dot.total,
        pct(dot.hit10, dot.total),
    );
}

/// Collects a build's full mod-text corpus lines (all three gear text
/// blocks + jewels + **allocated tree nodes**) for unsupported
/// classification. **Bypasses** `filter_parseable` / the tree-side gate --
/// classifies the raw lines directly so gap corpus is visible (method A-1 §1;
/// tree behaviour folded in per A2 -- after B3, the gate/ingest share the
/// same engine, so tree-side gaps are production gaps too).
fn collect_corpus_lines(dir: &Path, data: &BuildData) -> Vec<CorpusLine> {
    let Ok(code) = std::fs::read_to_string(dir.join("code.txt")) else {
        return Vec::new();
    };
    let Ok(build) = parse_build_from_code(code.trim()) else {
        return Vec::new();
    };
    let build_id = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?")
        .to_string();
    let mut lines = Vec::new();
    let push_item = |item: &pobr_data::item::Item, src: LineSource, lines: &mut Vec<CorpusLine>| {
        for text in item
            .implicit_texts
            .iter()
            .chain(item.modifier_texts.iter())
            .chain(item.enchant_texts.iter())
        {
            let t = text.trim();
            if !t.is_empty() {
                lines.push(CorpusLine {
                    text: t.to_string(),
                    source: src,
                    build_id: build_id.clone(),
                });
            }
        }
    };
    let mut item_slots: Vec<_> = build.items.values().collect();
    item_slots.sort_by_key(|i| i.modifier_texts.len());
    for item in item_slots {
        push_item(item, LineSource::Item, &mut lines);
    }
    for jewel in &build.jewels {
        push_item(jewel, LineSource::Jewel, &mut lines);
    }
    // Allocated tree-node stats (with `\n`-wrapped lines flattened out -- same convention as pobr_tree::split_lines).
    // Note: this corpus classifies **individual lines**, but tree-side production has a
    // wrapped-line-merging fallback (combine_wrapped), so the Partial/Unsupported figures
    // here are an **upper bound** on the production gap.
    for node_id in &build.tree.allocated_nodes {
        let Some(def) = data.passive_nodes.get(&node_id.0) else {
            continue;
        };
        for stat in &def.stats {
            for line in stat.split('\n') {
                let t = line.trim();
                if !t.is_empty() {
                    lines.push(CorpusLine {
                        text: t.to_string(),
                        source: LineSource::Passive,
                        build_id: build_id.clone(),
                    });
                }
            }
        }
    }
    lines
}

/// Unsupported-mod-rate trend report (roadmap item "fold the unsupported-rate
/// trend curve into the report"). **Report-only, not part of any gate
/// assertion** -- the acceptance criterion is the trend, not a percentage.
/// Per-build and aggregate counts/percentages for total mod lines / parsed /
/// unsupported / err, with the Top-20 gap templates attached (the source of
/// truth for picking C-2 batches).
///
/// `cargo test -p pobr-build --test ninja_parity -- corpus_unsupported_report --nocapture`
#[test]
fn corpus_unsupported_report() {
    let builds = discover_builds();
    let data = load_data();
    let mut all_lines: Vec<CorpusLine> = Vec::new();
    for dir in &builds {
        all_lines.extend(collect_corpus_lines(dir, &data));
    }
    // The engine production section (the gate and ingest share the same parser -- these numbers ARE production behaviour).
    // Partial = the engine recognized half the line but the whole line got dropped by the gate
    // (a migration candidate); dropped-tags = pre_flag silently drops a tag (a scope-widening
    // over-apply risk that was previously completely invisible).
    if let Some(rules) = data.parser_rules.as_deref() {
        use pobr_build::corpus::build_report_engine;
        let er = build_report_engine(&all_lines, rules);
        // The vendor-verdict source: ModCache golden (vendor's pre-parsed cache of the full corpus, dumped to disk).
        // Gap templates are checked precisely against samples (lowercase after stripping brackets) --
        // "vendor is also unsupported on the same text" = a pseudo-gap (nothing to fix; per the
        // slowing-potency lesson: vendor empty mods + all leftover = PoB2 has no effect); "vendor
        // parsed" = a real gap (the migration target list).
        let vendor_verdict: std::collections::HashMap<String, bool> = {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../pobr-core/tests/fixtures/modcache_golden.json");
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .and_then(|v| v.get("entries").and_then(|e| e.as_array()).cloned())
                .map(|entries| {
                    entries
                        .iter()
                        .filter_map(|e| {
                            let text = e.get("text")?.as_str()?.trim().to_ascii_lowercase();
                            // "vendor takes effect" = parsed with no leftover -- ModCache's
                            // status doesn't itself account for leftover, but tree loading
                            // drops the whole line when extra is non-empty (B3 semantics), so
                            // any non-empty leftover is ruled "no effect".
                            let parsed = e.get("status")?.as_str()? == "parsed"
                                && e.get("leftover")
                                    .and_then(|l| l.as_str())
                                    .is_none_or(|l| l.trim().is_empty());
                            Some((text, parsed))
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        // Templates pobr deliberately downgrades/models elsewhere -- stripped from the REAL list
        // to keep them from being repeatedly mis-listed as TODO:
        // - deferred: B3's deferred lines (unsupported_pobr_extra, a double-error-cancels-a-shortfall case);
        // - modeled-elsewhere: the gem-level family (gem_property_bonuses already models this by
        //   scanning raw lines independently; the GemProperty LIST has no consumer in ModDb; unified under C5).
        let deferred: std::collections::HashSet<String> = {
            let path = repo_data_root()
                .join(pobr_data::GOLDEN_PARITY_DATA_VERSION)
                .join("overlay/mod_parser_rules.json");
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .and_then(|v| {
                    v.get("unsupported_pobr_extra")
                        .and_then(|e| e.as_array())
                        .cloned()
                })
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| Some(s.as_str()?.to_string()))
                        .collect()
                })
                .unwrap_or_default()
        };
        // Templates explicitly ruled "won't migrate" (A2 cleanup) -- same category of pseudo-TODO
        // stripping as deferred/modeled-elsewhere, to keep the REAL list from being repeatedly mis-listed:
        // - no-consumer(pobr): the `grants skill:` family (vendor's ExtraSkill LIST). PoBR's skills
        //   come from the build XML's skill list, so tree/gear-granted lines have no consumer in
        //   ModDb -- migrating the LIST would just be noise (same for the gem-level family); to be
        //   wired up together once a granted-skill consumer is scoped.
        // - wont-do(pobr): tree-allocation-rule lines (non-keystone medium radius -- not a stat
        //   mod, it's pobr-tree allocation semantics) and per-skill distance ramp (the demo suite's
        //   enemyDistance is always a placeholder -> DistanceRamp is permanently dormant, and the
        //   DSL has no 4-capture ramp form either; see the ModTag::DistanceRamp docs).
        const NO_CONSUMER_PREFIXES: &[&str] = &["grants skill:"];
        const WONT_DO_PREFIXES: &[&str] = &[
            "non-keystone passive skills in medium radius",
            "projectiles deal #% more hit damage to targets in the first",
        ];
        let verdict_of = |t: &pobr_build::corpus::EngineTemplateStat| -> &'static str {
            if t.template.starts_with("+# to level of all") {
                return "modeled-elsewhere";
            }
            if NO_CONSUMER_PREFIXES
                .iter()
                .any(|p| t.template.starts_with(p))
            {
                return "no-consumer(pobr)";
            }
            if WONT_DO_PREFIXES.iter().any(|p| t.template.starts_with(p)) {
                return "wont-do(pobr)";
            }
            let mut any_hit = false;
            for s in &t.samples {
                let key = pobr_build::corpus::strip_brackets(s)
                    .trim()
                    .to_ascii_lowercase();
                if deferred.contains(&key) {
                    return "deferred(pobr)";
                }
                match vendor_verdict.get(&key) {
                    Some(true) => return "vendor=PARSED",
                    Some(false) => any_hit = true,
                    None => {}
                }
            }
            if any_hit { "vendor=unsup" } else { "vendor=?" }
        };
        eprintln!("\n============ ENGINE PRODUCTION REPORT (A2) ============");
        eprintln!(
            "[engine] lines: {}  parsed: {} ({:.1}%)  partial: {}  unsupported: {}  gap_rate: {:.1}%",
            er.total_lines,
            er.parsed,
            if er.total_lines == 0 {
                0.0
            } else {
                er.parsed as f64 / er.total_lines as f64 * 100.0
            },
            er.partial,
            er.unsupported,
            er.gap_rate() * 100.0,
        );
        eprintln!(
            "[engine] dropped-tag lines: {}  total dropped tags: {}",
            er.lines_with_dropped_tags, er.total_dropped_tags,
        );
        let (mut real, mut pseudo, mut unknown, mut skipped) = (0usize, 0usize, 0usize, 0usize);
        for t in &er.gap_templates {
            match verdict_of(t) {
                "vendor=PARSED" => real += 1,
                "vendor=unsup" => pseudo += 1,
                "deferred(pobr)" | "modeled-elsewhere" | "no-consumer(pobr)" | "wont-do(pobr)" => {
                    skipped += 1
                }
                _ => unknown += 1,
            }
        }
        eprintln!(
            "[vendor-verdict] gap templates: {real} REAL (vendor parses, we drop)  {pseudo} pseudo (vendor also unsupported)  {skipped} deferred/modeled-elsewhere/no-consumer/wont-do  {unknown} unknown (not in ModCache)",
        );
        eprintln!("--- Top-20 engine gap templates (production drops) ---");
        for (i, t) in er.gap_templates.iter().take(20).enumerate() {
            eprintln!(
                "{:>2}. [{:?}] hit={} cnt={} {} | {}",
                i + 1,
                t.class,
                t.builds_hit,
                t.total_count,
                verdict_of(t),
                t.template,
            );
        }
        eprintln!("--- REAL gaps (vendor parses, we drop — migration list, full) ---");
        let mut shown = 0usize;
        for t in &er.gap_templates {
            if verdict_of(t) == "vendor=PARSED" {
                shown += 1;
                eprintln!(
                    "{:>2}. [{:?}] hit={} cnt={} | {} || sample: {}",
                    shown,
                    t.class,
                    t.builds_hit,
                    t.total_count,
                    t.template,
                    t.samples.first().map(String::as_str).unwrap_or(""),
                );
            }
        }
        eprintln!("--- Top-10 dropped-tag templates (scope-widening risk) ---");
        for (i, t) in er.dropped_tag_templates.iter().take(10).enumerate() {
            eprintln!(
                "{:>2}. dropped={} hit={} cnt={} | {}",
                i + 1,
                t.dropped_tags,
                t.builds_hit,
                t.total_count,
                t.template,
            );
        }
        eprintln!(
            "total distinct engine gap templates: {}  dropped-tag templates: {}",
            er.gap_templates.len(),
            er.dropped_tag_templates.len(),
        );
        assert!(er.total_lines > 0, "engine 语料为空");
    }

    // Weak assertion: the corpus isn't empty (guards against a broken fixture/collection chain silently going to zero).
    assert!(
        !all_lines.is_empty(),
        "corpus 收集为空——检查 build 装备词条收集链"
    );
}

/// Convention-switch dual-run report: calculates the same build once under
/// `mode_effective=false` (panel convention) and once under
/// `mode_effective=true` (PoB2's main-panel EFFECTIVE convention, vendor
/// `CalcSetup.lua:583-588`), printing a three-column output per stat
/// (panel / effective / PoB2 golden) plus convergence/regression markers.
///
/// A print-only dashboard (no gate):
/// `cargo test -p pobr-build --test ninja_parity -- effective_switch_dual_run_report --nocapture`
#[test]
fn effective_switch_dual_run_report() {
    let data = load_data();
    let builds = discover_builds();
    assert!(!builds.is_empty(), "no builds discovered");

    let fmt_v = |v: f64| -> String {
        if is_inf_like(v) {
            "inf".into()
        } else {
            format!("{v:.2}")
        }
    };
    // Hit-band marker: ✓ = @5%, ~ = @10%, blank = miss.
    let band = |rt: f64| -> &'static str {
        if (rt - 1.0).abs() < TOL {
            "✓"
        } else if (rt - 1.0).abs() < TOL10 {
            "~"
        } else {
            " "
        }
    };

    let mut panel_tally = (Tally::default(), Tally::default(), Tally::default()); // (core, def, off)
    let mut eff_tally = (Tally::default(), Tally::default(), Tally::default());
    // Migration stats (@5% convention): (converged panel✗->eff✓, regressed panel✓->eff✗, both✓, both✗).
    let mut moved: Vec<String> = Vec::new();

    for dir in &builds {
        let name = dir.file_name().unwrap().to_string_lossy();
        let g = golden_stats(dir);
        let (Some(panel), Some(eff)) = (
            run_build_mode(dir, &data, false),
            run_build_mode(dir, &data, true),
        ) else {
            eprintln!("\n##### {name} :: PARSE/CALC FAILED #####");
            continue;
        };

        panel_tally
            .0
            .add(tally_rows(&defensive_core_rows(&panel, &g)));
        panel_tally.1.add(tally_rows(&defensive_rows(&panel, &g)));
        panel_tally.2.add(tally_rows(&offensive_rows(&panel, &g)));
        eff_tally.0.add(tally_rows(&defensive_core_rows(&eff, &g)));
        eff_tally.1.add(tally_rows(&defensive_rows(&eff, &g)));
        eff_tally.2.add(tally_rows(&offensive_rows(&eff, &g)));

        eprintln!("\n##### {name} #####");
        eprintln!(
            "  {:<18}{:>14}{:>14}{:>14}{:>9}{:>9}",
            "stat", "panel", "effective", "PoB2", "p-ratio", "e-ratio"
        );
        let p_rows = defensive_rows(&panel, &g)
            .into_iter()
            .chain(offensive_rows(&panel, &g));
        let e_rows = defensive_rows(&eff, &g)
            .into_iter()
            .chain(offensive_rows(&eff, &g));
        for (p, e) in p_rows.zip(e_rows) {
            let Some(gv) = p.golden else {
                continue;
            };
            let (rp, re) = (ratio(p.pobr, gv), ratio(e.pobr, gv));
            let (bp, be) = (band(rp), band(re));
            let trans = match (bp == "✓", be == "✓") {
                (false, true) => " ↑5%",
                (true, false) => " ↓LOST",
                _ if (rp - re).abs() > 1e-9 => " Δ",
                _ => "",
            };
            if bp != be || (rp - re).abs() > 1e-9 {
                moved.push(format!(
                    "{name} :: {:<16} panel {:.3}x → eff {:.3}x{trans}",
                    p.label, rp, re
                ));
            }
            eprintln!(
                "  {bp}{be} {:<16}{:>14}{:>14}{:>14}{:>8.2}x{:>8.2}x{trans}",
                p.label,
                fmt_v(p.pobr),
                fmt_v(e.pobr),
                fmt_v(gv),
                rp,
                re
            );
        }
    }

    let pct = |n: usize, d: usize| 100.0 * n as f64 / d.max(1) as f64;
    eprintln!("\n================ EFFECTIVE-SWITCH DUAL-RUN SUMMARY ================");
    for (label, p, e) in [
        ("def core-8", panel_tally.0, eff_tally.0),
        ("def 25-col", panel_tally.1, eff_tally.1),
        ("offensive ", panel_tally.2, eff_tally.2),
    ] {
        eprintln!(
            "{label}: panel {}/{} = {:.1}% @5% ({:.1}% @10%)  →  effective {}/{} = {:.1}% @5% ({:.1}% @10%)",
            p.hit5,
            p.total,
            pct(p.hit5, p.total),
            pct(p.hit10, p.total),
            e.hit5,
            e.total,
            pct(e.hit5, e.total),
            pct(e.hit10, e.total),
        );
    }
    eprintln!("\n-- 口径间逐值变化（panel ≠ effective 或命中带迁移） --");
    for m in &moved {
        eprintln!("  {m}");
    }
}

/// /F-3: EHP old vs new convention, 18-build dual-run comparison report.
///
/// After the F-3 convention switch: canonical `total_ehp`/`*_max_hit` are the
/// new convention; the "old" column takes `total_ehp_lowest_max_hit` (the old
/// pipeline still runs in parallel, keeping a revert path available); the old
/// per-type max-hit values are no longer listed separately (old and new are
/// mathematically equivalent under neutral input, see the F-2 report §3.1).
/// A print-only dashboard (no gate):
/// `cargo test -p pobr-build --test ninja_parity -- ehp_dual_run_report --nocapture`
#[test]
fn ehp_dual_run_report() {
    let data = load_data();
    let builds = discover_builds();
    assert!(!builds.is_empty(), "no builds discovered");

    // After sanitizing, golden's Infinity becomes 1e308 -- treat it as ∞ for display and ratio purposes.
    let fmt_v = |v: f64| -> String {
        if !v.is_finite() || v >= 1e307 {
            "inf".into()
        } else {
            format!("{v:.0}")
        }
    };
    let fmt_ratio = |pobr: f64, golden: Option<f64>| -> String {
        match golden {
            Some(g) if g >= 1e307 || !g.is_finite() => {
                if !pobr.is_finite() {
                    "✓inf".into()
                } else {
                    "fin/inf".into()
                }
            }
            Some(g) if g != 0.0 => format!("{:.2}x", pobr / g),
            Some(_) => "g=0".into(),
            None => "—".into(),
        }
    };

    eprintln!("\n========== M2 F-2 EHP 双跑对照（旧 lowest-max-hit vs 新 PoB2 口径） ==========");
    for dir in &builds {
        let name = dir.file_name().unwrap().to_string_lossy();
        let g = golden_stats(dir);
        let Some(out) = run_build(dir, &data) else {
            eprintln!("\n##### {name} :: PARSE/CALC FAILED #####");
            continue;
        };
        eprintln!("\n##### {name} #####");
        let g_ehp = golden(&g, "TotalEHP");
        eprintln!(
            "  TotalEHP        old {:>12}  new {:>12}  golden {:>12}  old {}  new {}",
            fmt_v(out.total_ehp_lowest_max_hit),
            fmt_v(out.total_ehp_pob2),
            g_ehp.map_or("—".into(), fmt_v),
            fmt_ratio(out.total_ehp_lowest_max_hit, g_ehp),
            fmt_ratio(out.total_ehp_pob2, g_ehp),
        );
        eprintln!(
            "  hitsToDie {:>8}  mitigatedHits {:>8}  enemyDamageIn {:>8}",
            fmt_v(out.number_of_damaging_hits),
            fmt_v(out.number_of_mitigated_hits),
            fmt_v(out.total_enemy_damage_in),
        );
        for (label, key, now_v) in [
            (
                "PhysMaxHit",
                "PhysicalMaximumHitTaken",
                out.physical_max_hit,
            ),
            ("FireMaxHit", "FireMaximumHitTaken", out.fire_max_hit),
            ("ColdMaxHit", "ColdMaximumHitTaken", out.cold_max_hit),
            (
                "LightMaxHit",
                "LightningMaximumHitTaken",
                out.lightning_max_hit,
            ),
            ("ChaosMaxHit", "ChaosMaximumHitTaken", out.chaos_max_hit),
        ] {
            let gv = golden(&g, key);
            eprintln!(
                "  {label:<14}  now {:>12}  golden {:>12}  {}",
                fmt_v(now_v),
                gv.map_or("—".into(), fmt_v),
                fmt_ratio(now_v, gv),
            );
        }
    }
    eprintln!(
        "\n（F-3 已切换：total_ehp/*_max_hit = PoB2 口径；旧 lowest-max-hit 口径保留在 total_ehp_lowest_max_hit）"
    );
}

/// Acceptance fixtures for four special categories ("MoM/CI/taken-as/shield
/// block"), checked @5% against golden.
///
/// How each category is covered:
/// - **MoM**: sorceress-stormweaver-comet (a `DamageTakenFromManaBeforeLife`
///   source; the mana pool folds into TotalHitPool) -- TotalEHP /
///   PhysicalMaximumHitTaken @5%;
/// - **CI**: monk-invoker-frost-bomb (the CI keystone) -- TotalEHP @5% +
///   ChaosMaximumHitTaken both ∞ (chaos immunity);
/// - **Shield block**: warrior-titan / warrior-smith-of-kitava --
///   EffectiveBlockChance / EffectiveSpellBlockChance @5% (the block-chance
///   layer; TotalEHP's 0.24-0.48x residual is a known gap: upstream armour
///   aggregation + block recovery's GainWhenHit (vendor :3168-3177) isn't
///   implemented, see the F-3 commit message's residual checklist, not
///   asserted here);
/// - **taken-as**: the 18-build golden set has no carrier for this mod, so
///   it's covered by a pobr-core synthetic fixture (`tests/taken_as.rs`'s
///   Lightning Coil type + `tests/ehp_pob2.rs`'s end-to-end test, with
///   expected values hand-computed from the CalcDefence.lua:356-455 formula).
#[test]
fn m2_f3_specialty_fixtures() {
    let data = load_data();
    let dir = builds_dir();
    let run = |name: &str| -> (OutputTable, serde_json::Map<String, serde_json::Value>) {
        let d = dir.join(name);
        let g = golden_stats(&d);
        let out = run_build(&d, &data).unwrap_or_else(|| panic!("{name} 计算失败"));
        (out, g)
    };
    let assert_5pct = |build: &str, stat: &str, pobr: f64, golden_v: Option<f64>| {
        let gv = golden_v.unwrap_or_else(|| panic!("{build} golden 缺 {stat}"));
        let rt = ratio(pobr, gv);
        assert!(
            (rt - 1.0).abs() < TOL,
            "{build} {stat}: pobr {pobr:.1} vs golden {gv:.1} = {rt:.3}x（超 5% 容差）"
        );
    };

    // MoM category.
    let (out, g) = run("sorceress-stormweaver-comet");
    assert_5pct(
        "sorceress-stormweaver-comet",
        "TotalEHP",
        out.total_ehp,
        golden(&g, "TotalEHP"),
    );
    assert_5pct(
        "sorceress-stormweaver-comet",
        "PhysicalMaximumHitTaken",
        out.physical_max_hit,
        golden(&g, "PhysicalMaximumHitTaken"),
    );

    // CI category.
    let (out, g) = run("monk-invoker-frost-bomb");
    assert_5pct(
        "monk-invoker-frost-bomb",
        "TotalEHP",
        out.total_ehp,
        golden(&g, "TotalEHP"),
    );
    // CI chaos immunity: both sides ∞ (golden's placeholder after sanitizing is 1e308).
    assert!(
        is_inf_like(out.chaos_max_hit)
            && golden(&g, "ChaosMaximumHitTaken").is_some_and(is_inf_like),
        "CI 混沌免疫应双 ∞"
    );

    // Shield block category (the block-chance layer for the two warrior builds).
    for name in [
        "warrior-titan-shield-wall",
        "warrior-smith-of-kitava-shield-wall",
    ] {
        let (out, g) = run(name);
        assert_5pct(
            name,
            "EffectiveBlockChance",
            out.effective_block_chance,
            golden(&g, "EffectiveBlockChance"),
        );
        assert_5pct(
            name,
            "EffectiveSpellBlockChance",
            out.effective_spell_block_chance,
            golden(&g, "EffectiveSpellBlockChance"),
        );
    }
}
