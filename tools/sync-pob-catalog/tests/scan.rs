use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use pobr_data::prelude::*;
use sync_pob_catalog::collect_catalog;

#[test]
fn scans_display_stats_sections_and_calculation_outputs() {
    let root = temp_root();
    let modules = root.join("src").join("Modules");
    fs::create_dir_all(&modules).unwrap();
    write(
        &modules.join("BuildDisplayStats.lua"),
        r#"
local displayStats = {
    { stat = "TotalDPS", label = "Hit DPS", fmt = ".1f", compPercent = true },
    { stat = "FireResist", label = "Fire Resistance", fmt = "d%%", overCapStat = "FireResistOverCap" },
    { stat = "ManaCost", label = "Mana Cost", fmt = "d", lowerIsBetter = true },
}
return displayStats
"#,
    );
    write(
        &modules.join("CalcSections.lua"),
        r#"
return {
    { label = "Skill DPS", { format = "{1:output:TotalDPS}", { breakdown = "TotalDPS" } } },
    { label = "Maximum Hit", { format = "{0:output:FireMaximumHitTaken}", { breakdown = "FireMaximumHitTaken" } } },
    { haveOutput = "TotalEHP" },
}
"#,
    );
    write(
        &modules.join("CalcOffence.lua"),
        r#"
local function calc()
    output.AverageHit = averageHit
    actor.output.TotalDPS = totalDps
end
"#,
    );
    write(
        &modules.join("CalcDefence.lua"),
        r#"
local function calc()
    output["FireResist"] = fireRes
    env.player.output.TotalEHP = totalEhp
end
"#,
    );
    write(
        &modules.join("Data.lua"),
        r#"
data.powerStatList = {
    "FullDPS",
    "TotalEHP",
}
"#,
    );

    let catalog = collect_catalog(&root).unwrap();

    assert!(catalog.display_stats.iter().any(|stat| {
        stat.id.as_str() == "TotalDPS"
            && stat.label.as_deref() == Some("Hit DPS")
            && stat.comparison_visible
            && stat.category == DisplayStatCategory::Offence
            && stat.parity_status == ParityStatus::Planned
    }));
    assert!(catalog.display_stats.iter().any(|stat| {
        stat.id.as_str() == "FireResist" && stat.category == DisplayStatCategory::Resistance
    }));
    assert!(
        catalog
            .display_stats
            .iter()
            .any(|stat| { stat.id.as_str() == "ManaCost" && stat.higher_is_better == Some(false) })
    );

    assert!(catalog.output_keys.iter().any(|entry| {
        entry.key.as_str() == "AverageHit" && entry.source_files.contains(&"CalcOffence.lua".into())
    }));
    assert!(catalog.output_keys.iter().any(|entry| {
        entry.key.as_str() == "FireMaximumHitTaken"
            && entry.source_files.contains(&"CalcSections.lua".into())
    }));
    assert!(catalog.output_keys.iter().any(|entry| {
        entry.key.as_str() == "FullDPS" && entry.source_files.contains(&"Data.lua".into())
    }));
    assert!(catalog.breakdowns.iter().any(|entry| {
        entry.id.as_str() == "FireMaximumHitTaken"
            && entry.source_files.contains(&"CalcSections.lua".into())
    }));
}

fn temp_root() -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("sync-pob-catalog-test-{now}"))
}

fn write(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
}
