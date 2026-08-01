//! Observation of the statmap data channel + oracle comparison.
//!
//! The legacy suffix heuristic and the dual-run L1/L2 diff were removed along
//! with T2.4 (the historical report and line-by-line rulings are archived at
//! `audits/rearchitecture-2026-06-10/blueprints/m1-statmap-switch-log.md`);
//! this file keeps the **Compare observation framework regression gate** and
//! **oracle sampling comparison** (a deliberate call: the Compare enum and
//! reporting framework stay long-term, and the config/parser dual-run reuse
//! the same pattern):
//!
//! ```bash
//! # Compare's pure-observation contract (runs normally, not ignored):
//! cargo test -p pobr-build --test statmap_dual_run
//! # Observation-record locator tool (manual run):
//! POBR_L2_BUILD=<build-dir-name> cargo test -p pobr-build --test statmap_dual_run -- --ignored --nocapture runtime
//! # oracle sampling requires the vendor PoB2 source + luajit:
//! POBR_POB2_SRC=/path/to/PathOfBuilding-PoE2/src \
//!   cargo test -p pobr-build --test statmap_dual_run -- --ignored --nocapture oracle
//! ```
//!
//! - **Compare's pure-observation contract**: the same build runs once each
//!   under Data and Compare mode; the output must match field-for-field
//!   (Compare doesn't change the calculation result), and after a Compare run
//!   the mapping-level outcome records can be retrieved;
//! - **oracle sampling** (>=50 stats, covering div/mult/base/value/tag/flags/
//!   per-set variants): `tools/pob2-oracle/statmap_oracle.lua` runs PoB2's
//!   real `mergeSkillInstanceMods` (controlled values injected via
//!   `extraStats`) and captures the resulting modList; names/flags/tags are
//!   normalized through the engine's translation layer and compared
//!   item-by-item against `stat_map_engine::map_stat`'s output, with the
//!   report written to `target/statmap-diff/oracle-report.md`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pobr_build::{
    BuildData, DataOrchestratorOptions, StatMapMode, calculate_with_data, parse_build_from_code,
};
use pobr_core::calc::{MinimalInput, OutputTable};
use pobr_core::modifier::{ModTag, Modifier};
use pobr_core::rules::stat_map_engine::{
    self, MappedItem, MappedOutcome, StatMapCatalog, translate_mod_name, translate_tag,
};
use pobr_data::catalog::stat_map::StatMapValue;
use pobr_data::modifier::ModType;
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};

/// Report output directory (`target/statmap-diff/`).
fn report_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/statmap-diff");
    std::fs::create_dir_all(&dir).expect("create target/statmap-diff");
    dir
}

fn load_game_data() -> GameData {
    GameData::new(repo_data_root().join(pobr_gamedata::data_version()))
}

fn load_stat_map_catalog(data: &GameData) -> StatMapCatalog {
    StatMapCatalog::new(
        data.skill_stat_map()
            .expect("load skill_stat_map.json")
            .expect("overlay/skill_stat_map.json 应已落库（M1-T2.1）"),
    )
}

// Compare observation framework

fn builds_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/demo-bd-test/builds")
        .canonicalize()
        .expect("builds dir exists")
}

fn opts(mode: StatMapMode, catalog: Option<Arc<StatMapCatalog>>) -> DataOrchestratorOptions {
    DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: false,
        extra_modifier_texts: vec![],
        stat_map_mode: mode,
        stat_map_catalog: catalog,
    }
}

/// Named expansion of OutputTable's main scalar fields (the diff row set;
/// OutputTable isn't a serde type, so these fields are hand-enumerated here —
/// the same family used by the ninja_parity report, plus offence/mechanics
/// extension fields).
fn scalar_fields(out: &OutputTable) -> Vec<(&'static str, f64)> {
    vec![
        ("life", out.life),
        ("mana", out.mana),
        ("armour", out.armour),
        ("evasion", out.evasion),
        ("energy_shield", out.energy_shield),
        ("fire_resistance", out.fire_resistance),
        ("cold_resistance", out.cold_resistance),
        ("lightning_resistance", out.lightning_resistance),
        ("crit_chance", out.crit_chance),
        ("crit_multiplier", out.crit_multiplier),
        ("total_hit_avg", out.total_hit_avg),
        ("hit_chance", out.hit_chance),
        ("action_rate", out.action_rate),
        ("effective_action_rate", out.effective_action_rate),
        ("dps", out.dps),
        ("bleed_dps", out.bleed_dps),
        ("ignite_dps", out.ignite_dps),
        ("poison_dps", out.poison_dps),
        ("aoe_radius", out.aoe_radius),
        ("projectile_count", out.projectile_count),
        ("cooldown", out.cooldown),
        ("mana_cost", out.mana_cost),
        ("life_cost", out.life_cost),
        ("spirit_reserved", out.spirit_reserved),
        ("mana_reserved", out.mana_reserved),
        ("life_reserved", out.life_reserved),
        ("total_ehp", out.total_ehp),
    ]
}

/// Compare mode's pure-observation contract: the same build runs once each
/// under Data and Compare mode; the output must be **identical
/// field-for-field** (Compare doesn't change the calculation result), and
/// after a Compare run, mapping-level outcome observation records can be
/// retrieved. Not ignored — this is the regression gate for the observation
/// framework itself.
#[test]
fn compare_mode_is_pure_observation() {
    let game_data = load_game_data();
    let Ok(Some(def)) = game_data.skill_stat_map() else {
        return; // skip when the data package isn't in place yet
    };
    let catalog = Arc::new(StatMapCatalog::new(def));
    let data = BuildData::load(&game_data).expect("load BuildData");
    // Uses a fixed sample build (same as the quality fixture, has 15×q20 gems and covers three sampling points).
    let code_path = builds_dir().join("sorceress-stormweaver-comet/code.txt");
    let Ok(code) = std::fs::read_to_string(&code_path) else {
        return;
    };
    let build = parse_build_from_code(code.trim()).expect("parse build");

    let data_out =
        calculate_with_data(&build, &data, &opts(StatMapMode::Data, None)).expect("data run");
    // Clears any leftover records so we retrieve only this Compare run's.
    let _ = pobr_build::take_stat_map_compare_records();
    let compare_out = calculate_with_data(
        &build,
        &data,
        &opts(StatMapMode::Compare, Some(catalog.clone())),
    )
    .expect("compare run");
    for ((label, dv), (_, cv)) in scalar_fields(&data_out)
        .into_iter()
        .zip(scalar_fields(&compare_out))
    {
        assert!(
            (dv - cv).abs() < 1e-12,
            "Compare 模式改变了输出字段 {label}：data={dv} compare={cv}"
        );
    }
    let records = pobr_build::take_stat_map_compare_records();
    assert!(
        !records.is_empty(),
        "Compare 模式应记录映射级 outcome（主技能/品质/support 取数点至少其一）"
    );
    assert!(
        records
            .iter()
            .all(|r| matches!(r.classification, "mapped" | "unsupported" | "unknown")),
        "观测记录分类应为 mapped/unsupported/unknown"
    );
    // Records have been taken -> taking again should be empty (take semantics).
    assert!(pobr_build::take_stat_map_compare_records().is_empty());
}

/// Observation-record locator helper: runs a single build (env
/// `POBR_L2_BUILD`, defaults to stormweaver) once under Compare mode and
/// prints the **runtime** per-stat mapping-level outcome records (with
/// sampling-point context) — a root-cause locator for field-level deviations
/// (which stat is unmapped/Unsupported at which sampling point).
#[test]
#[ignore = "观测定位（手动跑）：POBR_L2_BUILD=<build目录名> cargo test … runtime_compare_records"]
fn l2_runtime_compare_records() {
    let build_name =
        std::env::var("POBR_L2_BUILD").unwrap_or_else(|_| "sorceress-stormweaver-comet".into());
    let game_data = load_game_data();
    let catalog = Arc::new(load_stat_map_catalog(&game_data));
    let data = BuildData::load(&game_data).expect("load BuildData");
    let code = std::fs::read_to_string(builds_dir().join(&build_name).join("code.txt"))
        .expect("build code.txt");
    let build = parse_build_from_code(code.trim()).expect("parse build");
    let _ = pobr_build::take_stat_map_compare_records();
    let _ = calculate_with_data(&build, &data, &opts(StatMapMode::Compare, Some(catalog)))
        .expect("compare run");
    let records = pobr_build::take_stat_map_compare_records();
    println!(
        "== {build_name} 运行时映射级 outcome 观测（{} 条）==",
        records.len()
    );
    for r in &records {
        if r.classification != "mapped" {
            println!(
                "  [{}] {} @ {} :: {}",
                r.classification, r.stat, r.label, r.detail
            );
        }
    }
}

// Oracle sampling comparison (>=50 stats, PoB2 mergeSkillInstanceMods)

/// PoB2 `ModFlag` bits -> extraction-layer token names (vendor
/// `Data/Global.lua:213-249`; only decodes bits that could actually appear in
/// oracle samples — unknown bits stay `?0x…` so the translation layer rejects them).
const POB2_MOD_FLAG_BITS: [(u64, &str); 12] = [
    (0x1, "Attack"),
    (0x2, "Spell"),
    (0x4, "Hit"),
    (0x8, "Dot"),
    (0x10, "Cast"),
    (0x20, "Thorns"),
    (0x100, "Melee"),
    (0x200, "Area"),
    (0x400, "Projectile"),
    (0x800, "Ailment"),
    (0x1000, "MeleeHit"),
    (0x2000, "Weapon"),
];

/// PoB2 `KeywordFlag` bits -> token names (vendor `Data/Global.lua:251-292`).
const POB2_KEYWORD_FLAG_BITS: [(u64, &str); 25] = [
    (0x1, "Aura"),
    (0x2, "Curse"),
    (0x4, "Warcry"),
    (0x8, "Movement"),
    (0x10, "Physical"),
    (0x20, "Fire"),
    (0x40, "Cold"),
    (0x80, "Lightning"),
    (0x100, "Chaos"),
    (0x200, "Vaal"),
    (0x400, "Bow"),
    (0x800, "Arrow"),
    (0x1000, "Trap"),
    (0x2000, "Mine"),
    (0x4000, "Totem"),
    (0x8000, "Minion"),
    (0x10000, "Attack"),
    (0x20000, "Spell"),
    (0x40000, "Hit"),
    (0x80000, "Ailment"),
    (0x100000, "Brand"),
    (0x200000, "Poison"),
    (0x400000, "Bleed"),
    (0x800000, "Ignite"),
    (0x40000000, "MatchAll"),
];

fn decode_bits(bits: u64, table: &[(u64, &str)]) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut rest = bits;
    for (bit, name) in table {
        if bits & bit != 0 {
            tokens.push((*name).to_string());
            rest &= !bit;
        }
    }
    if rest != 0 {
        tokens.push(format!("?{rest:#x}"));
    }
    tokens
}

/// oracle JSON tag -> extraction-layer [`StatMapValue`] table (feeds [`translate_tag`] for normalization).
fn json_to_stat_map_value(v: &serde_json::Value) -> StatMapValue {
    match v {
        serde_json::Value::Bool(b) => StatMapValue::Bool(*b),
        serde_json::Value::Number(n) => StatMapValue::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => StatMapValue::Text(s.clone()),
        serde_json::Value::Array(items) => {
            StatMapValue::List(items.iter().map(json_to_stat_map_value).collect())
        }
        serde_json::Value::Object(map) => StatMapValue::Table(
            map.iter()
                .map(|(k, v)| (k.clone(), json_to_stat_map_value(v)))
                .collect(),
        ),
        serde_json::Value::Null => StatMapValue::Bool(false),
    }
}

/// Normalized comparison form for injected items from both the engine and oracle side.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
enum ComparableItem {
    Modifier {
        name: String,
        mod_type: String,
        /// A FLAG mod compares its bool truth value; a numeric mod compares as f64 (rounded to a 1e-6 tolerance before comparing).
        value: String,
        flags: u64,
        keyword_flags: u64,
        tags: Vec<String>,
    },
    SkillData {
        key: String,
        value: String,
    },
}

fn fmt_value(v: f64) -> String {
    format!("{:.6}", v)
}

fn engine_comparable(items: &[MappedItem]) -> Vec<ComparableItem> {
    let mut out: Vec<ComparableItem> = items
        .iter()
        .map(|item| match item {
            MappedItem::Modifier(m) => modifier_comparable(m),
            MappedItem::SkillData { key, value } => ComparableItem::SkillData {
                key: key.clone(),
                value: fmt_value(*value),
            },
        })
        .collect();
    out.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    out
}

fn modifier_comparable(m: &Modifier) -> ComparableItem {
    let value = match m.mod_type {
        ModType::Flag => format!("{:?}", m.value.as_bool().unwrap_or(false)),
        _ => fmt_value(m.value.as_number().unwrap_or(0.0)),
    };
    let mut tags: Vec<String> = m.tags.iter().map(tag_comparable).collect();
    tags.sort();
    ComparableItem::Modifier {
        name: m.name.to_string(),
        mod_type: m.mod_type.as_trace_label().to_string(),
        value,
        flags: m.flags.bits(),
        keyword_flags: m.keyword_flags.bits(),
        tags,
    }
}

fn tag_comparable(tag: &ModTag) -> String {
    format!("{tag:?}")
}

/// oracle-side single mod -> normalized comparison form (name/flags/keyword/tag
/// normalized through the engine's translation layer). Returns Err on
/// translation failure (sample FAILs: the engine claims Mapped but the oracle
/// form can't be normalized = a coverage mismatch).
fn oracle_mod_comparable(mod_json: &serde_json::Value) -> Result<ComparableItem, String> {
    let name = mod_json["name"].as_str().unwrap_or("?");
    let mod_type = mod_json["type"].as_str().unwrap_or("?");
    let flags_bits = mod_json["flags"].as_f64().unwrap_or(0.0) as u64;
    let kw_bits = mod_json["keywordFlags"].as_f64().unwrap_or(0.0) as u64;
    // skill_data (vendor `skill(key,…)` = SkillData LIST): a base-damage key maps to
    // `<Type>DamageMin/Max` BASE; duration maps to a SkillData item (matches engine's convention).
    if name == "SkillData" && mod_type == "LIST" {
        let key = mod_json["value"]["key"].as_str().unwrap_or("?").to_string();
        let value = mod_json["value"]["value"].as_f64().unwrap_or(0.0);
        let damage_types = ["Physical", "Fire", "Cold", "Lightning", "Chaos"];
        for ty in damage_types {
            for bound in ["Min", "Max"] {
                if key == format!("{ty}{bound}") {
                    // vendor `skill(key, nil, …tags)`'s tags must also be normalized and compared
                    // (e.g. the Multiplier tag on per_removable_frenzy_charge in
                    // `SkillStatMap.lua:99-101` -- the engine side's collect_skill_data attaches it too).
                    let mut tags = Vec::new();
                    for tag_json in mod_json["tags"].as_array().cloned().unwrap_or_default() {
                        let StatMapValue::Table(map) = json_to_stat_map_value(&tag_json) else {
                            return Err("tag 非对象".to_string());
                        };
                        let tag =
                            translate_tag(&map).map_err(|e| format!("tag 翻译失败: {e:?}"))?;
                        tags.push(tag_comparable(&tag));
                    }
                    tags.sort();
                    return Ok(ComparableItem::Modifier {
                        name: format!("{ty}Damage{bound}"),
                        mod_type: "BASE".to_string(),
                        value: fmt_value(value),
                        flags: 0,
                        keyword_flags: 0,
                        tags,
                    });
                }
            }
        }
        return Ok(ComparableItem::SkillData {
            key,
            value: fmt_value(value),
        });
    }
    let flag_tokens = decode_bits(flags_bits, &POB2_MOD_FLAG_BITS);
    let kw_tokens = decode_bits(kw_bits, &POB2_KEYWORD_FLAG_BITS);
    let translated = translate_mod_name(name, &flag_tokens, &kw_tokens)
        .map_err(|e| format!("名字翻译失败 {name}: {e:?}"))?;
    let mut tags = Vec::new();
    for tag_json in mod_json["tags"].as_array().cloned().unwrap_or_default() {
        let StatMapValue::Table(map) = json_to_stat_map_value(&tag_json) else {
            return Err("tag 非对象".to_string());
        };
        let tag = translate_tag(&map).map_err(|e| format!("tag 翻译失败: {e:?}"))?;
        tags.push(tag_comparable(&tag));
    }
    tags.sort();
    let value = match mod_type {
        "FLAG" => {
            // vendor's flag mod value is a Lua truth value.
            format!("{}", mod_json["value"].as_bool().unwrap_or(true))
        }
        _ => fmt_value(mod_json["value"].as_f64().unwrap_or(0.0)),
    };
    Ok(ComparableItem::Modifier {
        name: translated.name,
        mod_type: mod_type.to_string(),
        value,
        flags: translated.flags.bits(),
        keyword_flags: translated.keyword_flags.bits(),
        tags,
    })
}

/// The vendor PoB2 source directory (oracle's run root): explicitly set via
/// `POBR_POB2_SRC`, defaulting to the repo's `vendor/PathOfBuilding-PoE2/src`
/// (must use the env var when the worktree hasn't checked out vendor).
fn vendor_src_dir() -> PathBuf {
    if let Ok(p) = std::env::var("POBR_POB2_SRC") {
        return PathBuf::from(p);
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/PathOfBuilding-PoE2/src")
}

/// Oracle sampling: >=50 stats covering div / mult / base / value / multi-mod /
/// tag (Condition/ActorCondition/Multiplier/PerStat) / flags / conversion
/// gain-as / skill_data / per-set variants, compared against PoB2's real
/// `mergeSkillInstanceMods`.
#[test]
#[ignore = "需要 vendor PoB2 + luajit：POBR_POB2_SRC=<…>/src cargo test -p pobr-build --test statmap_dual_run -- --ignored oracle"]
fn oracle_statmap_sampling() {
    const PROBE_VALUE: f64 = 240.0;
    let game_data = load_game_data();
    let catalog = load_stat_map_catalog(&game_data);
    let data = BuildData::load(&game_data).expect("load BuildData");

    // L1 universe: stats actually present in the ingested data (global samples are only drawn from here, so the comparison covers real consumption).
    let mut universe: BTreeSet<String> = BTreeSet::new();
    for set_def in data.skill_stat_sets.values() {
        for set in &set_def.sets {
            for cs in &set.constant_stats {
                universe.insert(cs.stat.clone());
            }
            for level in &set.levels {
                for ds in &level.stats {
                    universe.insert(ds.stat.clone());
                }
            }
        }
    }

    // Sample selection (deterministic: BTreeMap ordering + a per-bucket quota)
    #[derive(Default)]
    struct Buckets {
        plain: Vec<String>,
        div: Vec<String>,
        mult: Vec<String>,
        base: Vec<String>,
        value: Vec<String>,
        multi_mod: Vec<String>,
        tag_condition: Vec<String>,
        tag_actor: Vec<String>,
        tag_multiplier: Vec<String>,
        flags: Vec<String>,
        conversion: Vec<String>,
        skill_data: Vec<String>,
    }
    let mut buckets = Buckets::default();
    let mapped = |stat: &str| {
        matches!(
            stat_map_engine::map_stat(&catalog, "", None, stat, PROBE_VALUE),
            MappedOutcome::Mapped(_)
        )
    };
    for (stat, entry) in catalog.global_entries() {
        if !universe.contains(stat) || !mapped(stat) {
            continue;
        }
        let has_tag = |ty: &str| {
            entry.mods.iter().any(|m| {
                m.tags
                    .iter()
                    .any(|t| matches!(t.get("type"), Some(StatMapValue::Text(s)) if s == ty))
            })
        };
        let has_flags = entry.mods.iter().any(|m| !m.flags.is_empty());
        let is_conversion = entry.mods.iter().any(|m| {
            m.name
                .as_deref()
                .is_some_and(|n| n.contains("DamageConvertTo") || n.contains("DamageGainAs"))
        });
        let is_skill_data = entry.mods.iter().any(|m| m.kind == "skill_data");
        let stat = stat.to_string();
        let cap = 8;
        if entry.div.is_some() && buckets.div.len() < cap {
            buckets.div.push(stat);
        } else if entry.mult.is_some() && buckets.mult.len() < cap {
            buckets.mult.push(stat);
        } else if entry.base.is_some() && buckets.base.len() < cap {
            buckets.base.push(stat);
        } else if entry.value.is_some() && buckets.value.len() < cap {
            buckets.value.push(stat);
        } else if has_tag("ActorCondition") && buckets.tag_actor.len() < cap {
            buckets.tag_actor.push(stat);
        } else if has_tag("Condition") && buckets.tag_condition.len() < cap {
            buckets.tag_condition.push(stat);
        } else if (has_tag("Multiplier") || has_tag("PerStat"))
            && buckets.tag_multiplier.len() < cap
        {
            buckets.tag_multiplier.push(stat);
        } else if has_flags && buckets.flags.len() < cap {
            buckets.flags.push(stat);
        } else if is_conversion && buckets.conversion.len() < cap {
            buckets.conversion.push(stat);
        } else if is_skill_data && buckets.skill_data.len() < cap {
            buckets.skill_data.push(stat);
        } else if entry.mods.len() > 1 && buckets.multi_mod.len() < cap {
            buckets.multi_mod.push(stat);
        } else if buckets.plain.len() < cap {
            buckets.plain.push(stat);
        }
    }
    // per-set coverage samples (default set "1"; the effect must actually exist in PoB2's data.skills).
    let mut per_set_samples: Vec<(String, String)> = Vec::new();
    for (effect, set_key, stat, _entry) in catalog.per_set_entries() {
        if set_key != "1" || per_set_samples.len() >= 12 {
            continue;
        }
        if !data.skill_stat_sets.contains_key(effect) {
            continue;
        }
        if !matches!(
            stat_map_engine::map_stat(&catalog, effect, None, stat, PROBE_VALUE),
            MappedOutcome::Mapped(_)
        ) {
            continue;
        }
        per_set_samples.push((effect.to_string(), stat.to_string()));
    }

    // Assembles probes (effect="GLOBAL" uses the global table).
    let mut probes: Vec<(String, String)> = Vec::new();
    for stat in [
        &buckets.plain,
        &buckets.div,
        &buckets.mult,
        &buckets.base,
        &buckets.value,
        &buckets.multi_mod,
        &buckets.tag_condition,
        &buckets.tag_actor,
        &buckets.tag_multiplier,
        &buckets.flags,
        &buckets.conversion,
        &buckets.skill_data,
    ]
    .into_iter()
    .flatten()
    {
        probes.push(("GLOBAL".to_string(), stat.clone()));
    }
    probes.extend(per_set_samples);
    assert!(
        probes.len() >= 50,
        "oracle 样本应 ≥50 条（实际 {}；各桶：plain={} div={} mult={} base={} value={} multi={} cond={} actor={} multplr={} flags={} conv={} sdata={} per_set={}）",
        probes.len(),
        buckets.plain.len(),
        buckets.div.len(),
        buckets.mult.len(),
        buckets.base.len(),
        buckets.value.len(),
        buckets.multi_mod.len(),
        buckets.tag_condition.len(),
        buckets.tag_actor.len(),
        buckets.tag_multiplier.len(),
        buckets.flags.len(),
        buckets.conversion.len(),
        buckets.skill_data.len(),
        probes.iter().filter(|(e, _)| e != "GLOBAL").count(),
    );

    // Runs the oracle (one process invocation; args = a repeated triple sequence)
    let vendor_src = vendor_src_dir();
    assert!(
        vendor_src.join("HeadlessWrapper.lua").exists(),
        "vendor PoB2 src 不存在：{}（设 POBR_POB2_SRC 指向 PathOfBuilding-PoE2/src）",
        vendor_src.display()
    );
    let oracle_script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/pob2-oracle/statmap_oracle.lua")
        .canonicalize()
        .expect("statmap_oracle.lua 存在");
    let luajit = std::env::var("LUAJIT").unwrap_or_else(|_| "luajit".to_string());
    let mut cmd = std::process::Command::new(&luajit);
    cmd.current_dir(&vendor_src)
        .env(
            "LUA_PATH",
            "../runtime/lua/?.lua;../runtime/lua/?/init.lua;./?.lua;;",
        )
        .env("CI", "true")
        .arg(&oracle_script);
    for (effect, stat) in &probes {
        cmd.arg(effect).arg(stat).arg(PROBE_VALUE.to_string());
    }
    let output = cmd.output().expect("运行 luajit oracle");
    assert!(
        output.status.success(),
        "oracle 进程失败：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle_json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("oracle JSON 可解析");
    let oracle_rows = oracle_json.as_array().expect("oracle JSON 是数组");
    assert_eq!(oracle_rows.len(), probes.len(), "oracle 行数与探针数一致");

    // Per-sample comparison
    let mut report = String::new();
    report.push_str("# statmap oracle 抽样对拍（M1-T2b，蓝图 T2.3）\n\n");
    report.push_str(&format!(
        "- 样本数：{}（global {} + per-set {}）；探针值 {PROBE_VALUE}\n",
        probes.len(),
        probes.iter().filter(|(e, _)| e == "GLOBAL").count(),
        probes.iter().filter(|(e, _)| e != "GLOBAL").count(),
    ));
    report.push_str("- oracle = vendor `calcs.mergeSkillInstanceMods`（CalcActiveSkill.lua:82，受控值经 extraStats 注入合成 statSet）\n");
    report.push_str("- 归一 = oracle 的 PoB2 名/flags/keyword/tag 经 `stat_map_engine::translate_mod_name`/`translate_tag` 翻译后与引擎输出逐项比对\n\n");
    report.push_str("| # | effect | stat | 结果 | 备注 |\n|---|---|---|---|---|\n");
    let mut failures = Vec::new();
    for (i, ((effect, stat), row)) in probes.iter().zip(oracle_rows).enumerate() {
        let engine_effect = if effect == "GLOBAL" { "" } else { effect };
        let outcome = stat_map_engine::map_stat(&catalog, engine_effect, None, stat, PROBE_VALUE);
        let MappedOutcome::Mapped(items) = outcome else {
            failures.push(format!("{effect}::{stat} 引擎非 Mapped：{outcome:?}"));
            report.push_str(&format!(
                "| {i} | {effect} | {stat} | FAIL | 引擎非 Mapped |\n"
            ));
            continue;
        };
        if let Some(err) = row.get("error").and_then(|e| e.as_str()) {
            failures.push(format!("{effect}::{stat} oracle 报错：{err}"));
            report.push_str(&format!(
                "| {i} | {effect} | {stat} | FAIL | oracle 报错：{err} |\n"
            ));
            continue;
        }
        let engine_side = engine_comparable(&items);
        let mut oracle_side = Vec::new();
        let mut translate_err = None;
        for mod_json in row["mods"].as_array().cloned().unwrap_or_default() {
            match oracle_mod_comparable(&mod_json) {
                Ok(item) => oracle_side.push(item),
                Err(e) => {
                    translate_err = Some(e);
                    break;
                }
            }
        }
        if let Some(e) = translate_err {
            failures.push(format!("{effect}::{stat} {e}"));
            report.push_str(&format!("| {i} | {effect} | {stat} | FAIL | {e} |\n"));
            continue;
        }
        oracle_side.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
        if engine_side == oracle_side {
            report.push_str(&format!("| {i} | {effect} | {stat} | PASS | |\n"));
        } else {
            failures.push(format!(
                "{effect}::{stat}\n  engine: {engine_side:?}\n  oracle: {oracle_side:?}"
            ));
            report.push_str(&format!(
                "| {i} | {effect} | {stat} | FAIL | engine={engine_side:?} oracle={oracle_side:?} |\n"
            ));
        }
    }
    report.push_str(&format!(
        "\n**结果：{}/{} PASS**\n",
        probes.len() - failures.len(),
        probes.len()
    ));
    std::fs::write(report_dir().join("oracle-report.md"), &report).expect("write oracle-report.md");
    println!(
        "== oracle 抽样：{}/{} PASS（报告 target/statmap-diff/oracle-report.md）==",
        probes.len() - failures.len(),
        probes.len()
    );
    assert!(
        failures.is_empty(),
        "oracle 对拍失败 {} 条：\n{}",
        failures.len(),
        failures.join("\n")
    );
}
