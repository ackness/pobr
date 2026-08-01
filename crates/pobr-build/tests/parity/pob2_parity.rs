//! PoB2 numeric-alignment harness: uses the `<PlayerStat>` values embedded in
//! real ninja build codes (values PoB2 itself computed when it exported them)
//! as the golden reference, and compares them against PoBR's calculated output.
//!
//! **Key point**: the XML decoded from a PoB Build Code contains
//! `<PlayerStat stat="X" value="Y"/>` — this is PoB2's authoritative answer,
//! no need to run PoB2 separately. This test prints a "PoBR vs PoB2"
//! comparison table per build (visible with `--nocapture`), and asserts the
//! invariants that **should currently hold**; known gaps don't hard-fail —
//! they serve as a live measure of alignment progress.
//!
//! Run with: `cargo test -p pobr-build --test pob2_parity -- --nocapture`

use pobr_build::{
    BuildData, DataOrchestratorOptions, calculate_with_data, decode_pob_code, parse_build_from_code,
};
use pobr_core::calc::{MinimalInput, OutputTable};
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};
use std::collections::HashMap;

const DEADEYE: &str = include_str!("../../../../examples/demo-bd-test/ninja-bd-deadeye.txt");
const MARTIAL: &str = include_str!("../../../../examples/demo-bd-test/ninja-bd-marial-artist.txt");

/// Extracts every `<PlayerStat stat="X" value="Y"/>` from the decoded Build XML (PoB2's reference values).
fn parse_player_stats(xml: &str) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    for chunk in xml.split("<PlayerStat ").skip(1) {
        let stat = between(chunk, "stat=\"", "\"");
        let value = between(chunk, "value=\"", "\"");
        if let (Some(s), Some(v)) = (stat, value)
            && let Ok(num) = v.parse::<f64>()
        {
            out.insert(s.to_string(), num);
        }
    }
    out
}

fn between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let i = s.find(start)? + start.len();
    let rest = &s[i..];
    let j = rest.find(end)?;
    Some(&rest[..j])
}

fn load_data() -> BuildData {
    // Pins the data version being checked against the golden values (decoupled from the active DATA_VERSION); see pobr_data::GOLDEN_PARITY_DATA_VERSION.
    let data = GameData::new(repo_data_root().join(pobr_data::GOLDEN_PARITY_DATA_VERSION));
    BuildData::load(&data).expect("load BuildData")
}

/// (display label, PoB2 key, PoBR value)
fn compare_row(out: &OutputTable, label: &str, pob2: Option<f64>, pobr: f64) -> String {
    match pob2 {
        Some(v2) => {
            let ratio = if v2 != 0.0 {
                format!("{:.2}x", pobr / v2)
            } else if pobr == 0.0 {
                "1.00x".into()
            } else {
                "inf".into()
            };
            let _ = out;
            format!("{label:<14}{pobr:>15.2}{v2:>15.2}{ratio:>10}")
        }
        None => format!("{label:<14}{pobr:>15.2}{:>15}{:>10}", "—", "—"),
    }
}

fn report(name: &str, code: &str, data: &BuildData) -> (OutputTable, HashMap<String, f64>) {
    let xml = decode_pob_code(code).expect("decode");
    let pob2 = parse_player_stats(&xml);
    let build = parse_build_from_code(code).expect("parse build");
    // Panel convention (PoB2's PlayerStat defence/attribute values are panel values; DPS is a separate case, see below).
    let opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: false,
        extra_modifier_texts: vec![],
        ..Default::default()
    };
    let out = calculate_with_data(&build, data, &opts).expect("calc");

    eprintln!("\n===== {name} :: PoBR vs PoB2 (embedded) =====");
    eprintln!("{:<14}{:>15}{:>15}{:>10}", "stat", "PoBR", "PoB2", "ratio");
    let rows: &[(&str, &str, f64)] = &[
        ("Life", "Life", out.life),
        ("Mana", "Mana", out.mana),
        ("EnergyShield", "EnergyShield", out.energy_shield),
        ("Armour", "Armour", out.armour),
        ("Evasion", "Evasion", out.evasion),
        ("FireRes", "FireResist", out.fire_resistance),
        ("ColdRes", "ColdResist", out.cold_resistance),
        ("LightRes", "LightningResist", out.lightning_resistance),
        // PoBR's crit_chance is a fraction (0.05), PoB2's CritChance is a percent (5) -> align by ×100.
        ("CritChance", "CritChance", out.crit_chance * 100.0),
        ("CritMulti", "CritMultiplier", out.crit_multiplier),
        ("AvgHit", "AverageDamage", out.total_hit_avg),
        ("DPS", "TotalDPS", out.dps),
    ];
    for (label, key, pobr) in rows {
        eprintln!(
            "{}",
            compare_row(&out, label, pob2.get(*key).copied(), *pobr)
        );
    }
    (out, pob2)
}

/// Asserts a PoB2-embedded PlayerStat and PoBR's output are within `tol` relative error (skipped if golden lacks the key).
fn assert_within(pob2: &HashMap<String, f64>, key: &str, pobr: f64, tol: f64) {
    if let Some(&golden) = pob2.get(key)
        && golden != 0.0
    {
        let ratio = pobr / golden;
        assert!(
            (ratio - 1.0).abs() < tol,
            "{key}: PoBR {pobr:.1} vs PoB2 {golden:.1} = {ratio:.3}x (tol {tol})"
        );
    }
}

/// Deadeye: prints the comparison + asserts the invariants that "should currently hold" (build parses, life is finite and positive, resistances <= cap).
#[test]
fn deadeye_parity_report() {
    let data = load_data();
    let (out, pob2) = report("DEADEYE", DEADEYE, &data);
    assert!(out.life > 0.0 && out.life.is_finite(), "life must be > 0");
    assert!(out.dps.is_finite(), "dps must be finite");
    // Already aligned with PoB2 within <10% (a regression gate to keep later changes from breaking deadeye parity):
    assert_within(&pob2, "Life", out.life, 0.10);
    assert_within(&pob2, "Armour", out.armour, 0.10);
    assert_within(&pob2, "CritChance", out.crit_chance * 100.0, 0.10);
    assert_within(&pob2, "CritMultiplier", out.crit_multiplier, 0.10);
    // Fire/Cold/Lightning resist + Evasion are **no longer asserted against
    // ninja-bd-deadeye.txt's embedded PlayerStat**: that code's embedded
    // `<PlayerStat>` was exported by a version of PoB2 that predates modeling
    // Mageblood's legacies, so it's missing Bismuth's ElementalResist +45
    // (embedded Fire 66/Cold 56/Light 75, uncapped) and Jade/Stibnite's
    // Evasion +2000/+150% (embedded 14301). The 0.5.4b authoritative golden
    // for the same build (fixture ranger-deadeye-explosive-grenade/meta.json)
    // has all three resists capped at 75 and Evasion 29774 -- PoBR's current
    // value matches it (Evasion 0.99x). So the old sample's res/evasion
    // assertions here are stale and removed; Mageblood's regression gate is
    // now carried by ninja_parity (0.5.4b oracle golden).
    // AvgDamage tolerance 0.20, DPS 0.12 (**this old sample's** convention,
    // not the primary regression gate -- the primary gate is ninja_parity's
    // structured builds): deadeye's damage-base undershoot gap (oracle
    // confirms ~0.59-0.64x physical base, caused by grenade gem level bonus
    // being deliberately suppressed + a missing Mirage Deadeye global -25%
    // more + a grenade-throughput/Speed compensation knot) used to be
    // coincidentally masked by a "missing per-type final MORE" bug (Lightning
    // Attunement's `support_cold_and_fire_damage_+%_final` wasn't injected ->
    // Fire/Cold were inflated ~2.1x), which made AvgDamage falsely land at
    // 0.894x. After Wave 12 fixed the per-type final MORE mapping, Fire/Cold
    // each converged to 1.05x per-component (double-confirmed by oracle), and
    // the real base gap was exposed (AvgHit 0.817x). The base gap is a
    // separate task for grenade cooldown throughput / Mirage data completion
    // (it's coupled with a Speed 1.71x over-calculation, so a one-sided fix
    // would send DPS flying the other way), out of scope for this wave.
    // Tolerance is loosened to match the current real deviation, to be
    // tightened once the grenade-chain data is filled in.
    //
    // Loosened further after the statmap switch (Legacy->Data): once the Data
    // channel added the Multishot -25% more that Legacy had failed to inject
    // (`sup_dex.lua:3154-3156`, now fixed), this old sample's AverageDamage
    // moved 0.817x->0.613x and TotalDPS shifted down with it -- another layer
    // of Legacy's false-positive "over-count masking an under-count" was
    // peeled away, fully exposing the real base/throughput gap (the same
    // compensation knot as the ninja deadeye row; see switch-review log §3).
    //
    // Loosened the same direction for the finishing-move fix (weapon-set
    // exclusive-node filtering, vendor CalcSetup.lua:209-233/:791-792): 22
    // exclusive nodes from the inactive WeaponSet2 (including damage nodes)
    // were previously wrongly counted, producing a false convergence; once
    // stripped per vendor semantics, the whole deviation attributes to the
    // grenade base/throughput gap recorded above. Combined, the two layers of
    // "over-count masking an under-count" were peeled away **multiplicatively**
    // (0.613x × 0.647/0.817 ≈ 0.485x, matches measurement); tolerance is
    // loosened to the combined real deviation, only to guard against further
    // regression -- to be tightened once grenade cooldown throughput / Mirage
    // data is completed.
    //
    // Tightened after 0.5.4b #4 (grenade phrase unblocked): AvgDamage now
    // measures 1.04x -- the base-side gap in the "over-count masking an
    // under-count" chain above has been closed by the accumulated fixes, so
    // tolerance tightens 0.55->0.20. TotalDPS is **no longer asserted against
    // this old sample** (same reasoning as res/evasion above): the embedded
    // PlayerStat was exported before 0.5.4b's grenade CDR rebalance (vendor
    // 0.22.0's ModParser gem loop added a `fromItem` exclusion, reviving the
    // `for grenade skills` phrase -> the 3×15 CDR tree mods take effect,
    // Speed 0.164->0.254), so the old sample's Speed convention is stale;
    // the authoritative DPS gate is ninja_parity's 0.5.4b fixture golden
    // (Speed exactly 0.254, TotalDPS 0.83x, ratchet-guarded).
    assert_within(&pob2, "AverageDamage", out.total_hit_avg, 0.20);
    // Evasion: see the comment above -- the embedded sample is missing Mageblood (14301), so it's not hard-asserted; the authoritative value comes from the fixture golden.
}

#[test]
fn martial_parity_report() {
    let data = load_data();
    let (out, pob2) = report("MARTIAL", MARTIAL, &data);
    assert!(out.life.is_finite() && out.mana.is_finite());
    assert!(out.dps.is_finite());
    // EnergyShield "1.12x overestimate" diagnosis (backlog item #11-2): **the
    // old sample's golden is stale, PoBR isn't over-computing**. This code
    // has `targetVersion="0_1"` (exported in the PoE2 0.1 era), with embedded
    // PlayerStat EnergyShield=6257; feeding the same XML into current vendor
    // (tools/pob2-oracle) gives 7008, and PoBR gives 7005.5 = 0.9996x of
    // current vendor. Conventions unchanged between 0.1 and 0.5.x
    // (Life/Mana/Evasion/three resists/Crit) match the current oracle
    // value-for-value in the embedded data; only ES (and the offence side)
    // drifted with the data version -- the same category as the stale
    // Mageblood/res sample above for deadeye. This assertion is guarded by
    // **current vendor semantics** (tolerance 0.15 covers the stale-golden
    // gap of 1.12x, only to guard against a large regression; the
    // authoritative ES gate is ninja_parity's 0.5.4b fixture golden).
    assert_within(&pob2, "EnergyShield", out.energy_shield, 0.15);
    assert_within(&pob2, "Evasion", out.evasion, 0.05);
    assert_within(&pob2, "Mana", out.mana, 0.05);
    assert_within(&pob2, "CritChance", out.crit_chance * 100.0, 0.05);
}
