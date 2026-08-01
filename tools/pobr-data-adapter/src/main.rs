//! pobr-data-adapter: adapts pathofexile-dat's raw `.dat` JSON into PoBR's minimal JSON.
//!
//! Resolves integer foreign keys (ItemClass / Tags / Implicit_Mods -> stable
//! string IDs), denormalizes, filters out dev-only placeholder entries, and
//! writes diff-friendly JSON sorted by id to `data/<patch>/base/`;
//! `manifest.json` and the `i18n/` sidecar stay at the version root.
//!
//! Usage:
//! ```text
//! # Item base domain (pathofexile-dat tables)
//! cargo run -p pobr-data-adapter -- --raw pipeline/tables --out data --patch 4.5.0.3.4
//! # Passive tree domain (GGG's official tree export data.json)
//! cargo run -p pobr-data-adapter -- --tree pipeline/tree/data.json --out data --patch 4.5.0.3.4
//! # Backfill isSwitchable per-class/ascendancy variants (vendor tree.lua -> the existing passive_tree.json)
//! cargo run -p pobr-data-adapter -- --tree-variants vendor/PathOfBuilding-PoE2/src/TreeData/0_5/tree.lua \
//!     --out data --patch 4.5.0.3.4
//! # Backfill the anointable notable pool (vendor tree.lua -> append notables missing from passive_tree.json)
//! cargo run -p pobr-data-adapter -- --tree-anoints vendor/PathOfBuilding-PoE2/src/TreeData/0_5/tree.lua \
//!     --out data --patch 4.5.0.3.4
//! ```

mod special_derived;
mod tree;
mod tree_anoints;
mod tree_coords;
mod tree_variants;
mod tree_versions;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pobr_data::catalog::{
    ArmourBaseStats, BaseItemDef, CATALOG_SCHEMA_VERSION, DataManifest, DomainSections,
    WeaponBaseStats,
};
use serde::Deserialize;

mod mods;
mod required_columns;
mod skills;

const ZH_TW: &str = "Traditional Chinese";

fn main() -> ExitCode {
    let result = match parse_args() {
        Ok(Mode::BaseItems(args)) => run(args),
        Ok(Mode::Tree(args)) => tree::run(args),
        Ok(Mode::TreeCoords(args)) => tree_coords::run(args),
        Ok(Mode::TreeVariants(args)) => tree_variants::run(args),
        Ok(Mode::TreeAnoints(args)) => tree_anoints::run(args),
        Ok(Mode::TreeVersions(args)) => tree_versions::run(args),
        Ok(Mode::SpecialDerived(args)) => special_derived::run(args),
        Err(err) => Err(err),
    };
    match result {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("pobr-data-adapter 失败：{err}");
            ExitCode::FAILURE
        }
    }
}

struct Args {
    raw: PathBuf,
    out: PathBuf,
    patch: String,
    /// CI strict mode: turns column drift into a hard failure (default false = warn and continue, see [`run`]).
    strict_columns: bool,
}

/// Adapter subcommands: item base domain (`--raw`) or passive tree domain (`--tree`), mutually exclusive.
enum Mode {
    BaseItems(Args),
    Tree(tree::TreeArgs),
    /// Backfill node x/y coordinates into the existing `passive_tree.json` from vendor `tree.lua`.
    TreeCoords(tree_coords::TreeCoordsArgs),
    /// Backfill per-class/ascendancy variants for isSwitchable nodes from vendor `tree.lua`.
    TreeVariants(tree_variants::TreeVariantsArgs),
    /// Append the anointable notable pool missing from `passive_tree.json`, from vendor `tree.lua`.
    TreeAnoints(tree_anoints::TreeAnointsArgs),
    /// Extraction of historical league tree versions (vendor
    /// `TreeData/<v>/tree.lua` -> `base/passive_trees/<v>.json`, for adapting old builds' treeVersion).
    TreeVersions(tree_versions::TreeVersionsArgs),
    /// Derives `generated/special_derived.json` from the existing `passive_tree.json`'s keystone nodes.
    SpecialDerived(special_derived::SpecialDerivedArgs),
}

fn parse_args() -> Result<Mode, String> {
    let mut raw = None;
    let mut tree = None;
    let mut tree_coords = None;
    let mut tree_variants = None;
    let mut tree_anoints = None;
    let mut tree_full = None;
    let mut tree_version = None;
    let mut special_derived = None;
    let mut out = None;
    let mut patch = None;
    let mut strict_columns = false;
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut take = |name: &str| it.next().ok_or_else(|| format!("{name} 缺少参数值"));
        match flag.as_str() {
            "--raw" => raw = Some(PathBuf::from(take("--raw")?)),
            "--tree" => tree = Some(PathBuf::from(take("--tree")?)),
            "--tree-coords" => tree_coords = Some(PathBuf::from(take("--tree-coords")?)),
            "--tree-variants" => tree_variants = Some(PathBuf::from(take("--tree-variants")?)),
            "--tree-anoints" => tree_anoints = Some(PathBuf::from(take("--tree-anoints")?)),
            "--tree-full" => tree_full = Some(PathBuf::from(take("--tree-full")?)),
            "--tree-version" => tree_version = Some(take("--tree-version")?),
            "--emit-special-derived" => {
                special_derived = Some(PathBuf::from(take("--emit-special-derived")?))
            }
            "--out" => out = Some(PathBuf::from(take("--out")?)),
            "--patch" => patch = Some(take("--patch")?),
            // Resilience: by default, a missing column only warns and
            // continues (keeping data and code decoupled); CI can add this flag to turn it into a hard failure.
            "--strict-columns" => strict_columns = true,
            other => return Err(format!("未知参数：{other}")),
        }
    }
    let out = out.ok_or("缺少 --out <data>")?;
    let patch = patch.ok_or("缺少 --patch <version>")?;
    let mode_count = [
        raw.is_some(),
        tree.is_some(),
        tree_coords.is_some(),
        tree_variants.is_some(),
        tree_anoints.is_some(),
        tree_full.is_some(),
        special_derived.is_some(),
    ]
    .into_iter()
    .filter(|&b| b)
    .count();
    if mode_count > 1 {
        return Err(
            "--raw / --tree / --tree-coords / --tree-variants / --tree-anoints / \
             --emit-special-derived 互斥，请分别运行"
                .into(),
        );
    }
    if let Some(tree_json) = special_derived {
        return Ok(Mode::SpecialDerived(special_derived::SpecialDerivedArgs {
            tree_json,
            out,
            patch,
        }));
    }
    if let Some(raw) = raw {
        Ok(Mode::BaseItems(Args {
            raw,
            out,
            patch,
            strict_columns,
        }))
    } else if let Some(data_json) = tree {
        Ok(Mode::Tree(tree::TreeArgs {
            data_json,
            out,
            patch,
        }))
    } else if let Some(tree_lua) = tree_coords {
        Ok(Mode::TreeCoords(tree_coords::TreeCoordsArgs {
            tree_lua,
            out,
            patch,
        }))
    } else if let Some(tree_lua) = tree_variants {
        Ok(Mode::TreeVariants(tree_variants::TreeVariantsArgs {
            tree_lua,
            out,
            patch,
        }))
    } else if let Some(tree_lua) = tree_anoints {
        Ok(Mode::TreeAnoints(tree_anoints::TreeAnointsArgs {
            tree_lua,
            out,
            patch,
        }))
    } else if let Some(tree_lua) = tree_full {
        Ok(Mode::TreeVersions(tree_versions::TreeVersionsArgs {
            tree_lua,
            tree_version: tree_version.ok_or("--tree-full 需要 --tree-version <如 0_3>")?,
            out,
            patch,
        }))
    } else {
        Err("缺少 --raw <pipeline/tables> / --tree <data.json> / \
             --tree-coords <tree.lua> / --tree-variants <tree.lua> / \
             --tree-anoints <tree.lua>"
            .into())
    }
}

// Raw .dat JSON row structures (only the columns we need)

#[derive(Deserialize)]
pub(crate) struct RawIndexed {
    #[serde(rename = "_index")]
    index: usize,
    #[serde(rename = "Id")]
    id: String,
}

#[derive(Deserialize)]
struct RawBaseItem {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "ItemClass")]
    item_class: Option<usize>,
    #[serde(rename = "DropLevel")]
    drop_level: Option<i64>,
    #[serde(rename = "Width")]
    width: Option<u8>,
    #[serde(rename = "Height")]
    height: Option<u8>,
    #[serde(rename = "Tags", default)]
    tags: Vec<usize>,
    #[serde(rename = "Implicit_Mods", default)]
    implicit_mods: Vec<usize>,
    #[serde(rename = "ModDomain")]
    mod_domain: Option<u32>,
}

#[derive(Deserialize)]
pub(crate) struct RawNamed {
    #[serde(rename = "_index")]
    pub(crate) index: usize,
    #[serde(rename = "Name")]
    pub(crate) name: Option<String>,
}

#[derive(Deserialize)]
struct RawWeaponType {
    /// `BaseItemTypes` row index.
    #[serde(rename = "BaseItemType")]
    base_item_type: Option<usize>,
    #[serde(rename = "DamageMin")]
    damage_min: Option<i64>,
    #[serde(rename = "DamageMax")]
    damage_max: Option<i64>,
    #[serde(rename = "Speed")]
    speed: Option<i64>,
    #[serde(rename = "CritChance")]
    crit_chance: Option<i64>,
    #[serde(rename = "RangeMax")]
    range_max: Option<i64>,
    /// Crossbow reload time (milliseconds; vendor `Export/spec.lua:62483` and
    /// `bases.lua:268-269` only export it when >0). Old exports (table
    /// snapshots without this column) fall back to `None`, leaving the
    /// output field at its default (schema-compatible; currently backfilled
    /// by the `base_item_overrides.json` overlay).
    #[serde(rename = "ReloadTime", default)]
    reload_time: Option<i64>,
}

#[derive(Deserialize)]
struct RawArmourType {
    #[serde(rename = "BaseItemType")]
    base_item_type: Option<usize>,
    #[serde(rename = "Armour")]
    armour: Option<i64>,
    #[serde(rename = "Evasion")]
    evasion: Option<i64>,
    #[serde(rename = "EnergyShield")]
    energy_shield: Option<i64>,
    #[serde(rename = "Ward")]
    ward: Option<i64>,
    /// Raw movement-speed modifier value (basis points; negative means a
    /// slowdown, e.g. `-300` = a 3% movement-speed penalty). Old exports
    /// (table snapshots without this column) fall back to `None`, leaving the output field at its default (schema-compatible).
    #[serde(rename = "IncreasedMovementSpeed", default)]
    increased_movement_speed: Option<i64>,
}

fn nn(v: Option<i64>) -> u32 {
    v.unwrap_or(0).max(0) as u32
}

/// The pair of `WeaponTypes` / `ArmourTypes` base-stat lookup tables, keyed by base item row index.
type BaseStatsLookups = (
    BTreeMap<usize, WeaponBaseStats>,
    BTreeMap<usize, ArmourBaseStats>,
);

/// Builds base-stat lookup tables from `WeaponTypes` / `ArmourTypes` (indexed by `BaseItemType` row).
fn weapon_armour_lookups(en: &Path) -> Result<BaseStatsLookups, String> {
    let weapons = read_json::<Vec<RawWeaponType>>(&en.join("WeaponTypes.json"))?;
    let mut weapon_map = BTreeMap::new();
    for w in weapons {
        if let Some(idx) = w.base_item_type {
            weapon_map.insert(
                idx,
                WeaponBaseStats {
                    physical_min: nn(w.damage_min),
                    physical_max: nn(w.damage_max),
                    speed_ms: nn(w.speed),
                    crit_chance: nn(w.crit_chance),
                    range: nn(w.range_max),
                    // Only stored when >0 (matching vendor bases.lua:268's
                    // `if ReloadTime > 0`; this column is always 0 for non-crossbow weapons).
                    reload_time_ms: w.reload_time.filter(|&v| v > 0).map(|v| v as u32),
                },
            );
        }
    }
    let armours = read_json::<Vec<RawArmourType>>(&en.join("ArmourTypes.json"))?;
    let mut armour_map = BTreeMap::new();
    for a in armours {
        if let Some(idx) = a.base_item_type {
            armour_map.insert(
                idx,
                ArmourBaseStats {
                    armour: nn(a.armour),
                    evasion: nn(a.evasion),
                    energy_shield: nn(a.energy_shield),
                    ward: nn(a.ward),
                    // Shield block chance (`ShieldTypes.Block`): the CDN has
                    // pruned this bundle for the pinned patch, so it's
                    // unreachable via the `.dat` route -> always None,
                    // backfilled by the `base_item_overrides.json` overlay during gamedata loading
                    block_chance: None,
                    // Movement penalty: PoB2's formula is `-raw/10000`
                    // (Export/Scripts/bases.lua:298), e.g. raw=-300 -> 0.03
                    // (a 3% slowdown); raw=0 -> None (diff-friendly).
                    movement_penalty: a
                        .increased_movement_speed
                        .filter(|&v| v != 0)
                        .map(|v| -(v as f64) / 10000.0),
                },
            );
        }
    }
    Ok((weapon_map, armour_map))
}

pub(crate) fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|e| format!("读取 {} 失败：{e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("解析 {} 失败：{e}", path.display()))
}

/// Builds a position-indexed lookup table from `_index -> Id` (`_index` is contiguous; out-of-range returns None).
fn id_lookup(rows: &[RawIndexed]) -> Vec<String> {
    let max = rows.iter().map(|r| r.index).max().map_or(0, |m| m + 1);
    let mut table = vec![String::new(); max];
    for r in rows {
        table[r.index] = r.id.clone();
    }
    table
}

pub(crate) fn resolve(lookup: &[String], idx: usize) -> Option<String> {
    lookup.get(idx).filter(|s| !s.is_empty()).cloned()
}

/// A dev-only placeholder / not-yet-enabled entry (excluded from the catalog).
pub(crate) fn is_placeholder(name: &str) -> bool {
    name.is_empty() || name.contains("[DNT") || name.contains("[UNUSED") || name.contains("[OLD")
}

fn run(args: Args) -> Result<String, String> {
    let en = args.raw.join("English");
    let tw = args.raw.join(ZH_TW);

    // F2 resilience: the required-column check is no longer fatal — a
    // missing column degrades via serde's default (the field is missing/empty),
    // loudly warns, and writes `_drift.json` for review; only
    // `--strict-columns` (the CI gate) turns it into a hard failure.
    let column_drift = required_columns::check_required_columns(&en, &tw)?;
    if !column_drift.is_empty() {
        eprintln!(
            "⚠ pobr-data-adapter：检测到 {} 处列漂移（按 serde 默认降级，相关字段将缺失/为空，\
             不中止；如需严格门禁用 --strict-columns）：",
            column_drift.len()
        );
        for m in &column_drift {
            eprintln!("  - {m}");
        }
        if args.strict_columns {
            return Err(format!(
                "--strict-columns：列漂移 {} 处，拒绝继续：\n  - {}",
                column_drift.len(),
                column_drift.join("\n  - ")
            ));
        }
    }

    // Foreign-key resolution tables
    let classes = id_lookup(&read_json::<Vec<RawIndexed>>(&en.join("ItemClasses.json"))?);
    let tags = id_lookup(&read_json::<Vec<RawIndexed>>(&en.join("Tags.json"))?);
    let mods = id_lookup(&read_json::<Vec<RawIndexed>>(&en.join("Mods.json"))?);
    let stats = id_lookup(&read_json::<Vec<RawIndexed>>(&en.join("Stats.json"))?);

    // Base items (English canonical + Traditional Chinese name sidecar)
    let raw_bases = read_json::<Vec<RawBaseItem>>(&en.join("BaseItemTypes.json"))?;
    let tw_names = read_json::<Vec<RawNamed>>(&tw.join("BaseItemTypes.json"))?;
    let tw_by_index: BTreeMap<usize, String> = tw_names
        .into_iter()
        .filter_map(|r| r.name.map(|n| (r.index, n)))
        .collect();

    // Weapon/armour base stats (WeaponTypes / ArmourTypes, joined by base item row index).
    let (weapon_map, armour_map) = weapon_armour_lookups(&en)?;

    let mut bases = Vec::new();
    let mut i18n_zh: BTreeMap<String, String> = BTreeMap::new();
    let total = raw_bases.len();
    for (index, raw) in raw_bases.into_iter().enumerate() {
        if is_placeholder(&raw.name) {
            continue;
        }
        let item_class = raw
            .item_class
            .and_then(|i| resolve(&classes, i))
            .unwrap_or_default();
        let tag_ids: Vec<String> = raw.tags.iter().filter_map(|&i| resolve(&tags, i)).collect();
        let implicits: Vec<String> = raw
            .implicit_mods
            .iter()
            .filter_map(|&i| resolve(&mods, i))
            .collect();

        if let Some(zh) = tw_by_index.get(&index)
            && !zh.is_empty()
            && *zh != raw.name
        {
            i18n_zh.insert(raw.id.clone(), zh.clone());
        }

        bases.push(BaseItemDef {
            req_str: 0,
            req_dex: 0,
            req_int: 0,
            id: raw.id,
            name: raw.name,
            item_class,
            drop_level: raw.drop_level.unwrap_or(0).max(0) as u32,
            width: raw.width.unwrap_or(1),
            height: raw.height.unwrap_or(1),
            tags: tag_ids,
            implicits,
            mod_domain: raw.mod_domain.unwrap_or(0),
            weapon: weapon_map.get(&index).cloned(),
            armour: armour_map.get(&index).cloned(),
            // Base spirit (`ItemSpirit.SpiritGranted`): same story as
            // block_chance, unreachable via the `.dat` bundle -> always
            // None, backfilled by the overlay during gamedata loading.
            spirit: None,
            // Charm base buff (vendor `Data/Bases/flask.lua`'s
            // `charm.buff`): `.dat` has no such column -> always empty,
            // backfilled by the overlay during gamedata loading.
            charm_buff: Vec::new(),
        });
    }

    bases.sort_by(|a, b| a.id.cmp(&b.id));

    // Write out: domain JSON goes in the base/ layer; manifest and the i18n sidecar stay at the version root.
    let version_dir = args.out.join(&args.patch);
    let base_dir = version_dir.join("base");
    fs::create_dir_all(&base_dir).map_err(|e| format!("创建输出目录失败：{e}"))?;
    fs::create_dir_all(version_dir.join("i18n").join("zh-TW"))
        .map_err(|e| format!("创建输出目录失败：{e}"))?;

    // Column drift report: if there's drift, write `_drift.json` (machine
    // readable, for regen/CI review); if not, clean up any stale file (keeps the directory clean and reproducible).
    let drift_path = version_dir.join("_drift.json");
    if column_drift.is_empty() {
        let _ = fs::remove_file(&drift_path);
    } else {
        let drift = serde_json::json!({
            "_meta": {
                "kind": "adapter-column-drift",
                "note": "GGG .dat 缺少 adapter 期望列；相关产物字段按 serde 默认降级。\
                         非致命，仅提示数据/schema 漂移待复核。",
                "patch": args.patch,
            },
            "missing_columns": column_drift,
        });
        write_pretty(&drift_path, &drift)?;
        eprintln!("   列漂移报告 → {}", drift_path.display());
    }

    write_pretty(&base_dir.join("base_items.json"), &bases)?;
    write_pretty(&version_dir.join("i18n/zh-TW/base_items.json"), &i18n_zh)?;

    // Mods + Stats domain (the stat registry + the affix pool).
    let (stat_count, mod_count, mod_filtered, mod_zh) =
        mods::adapt(&en, &tw, &stats, &tags, &base_dir, &version_dir)?;

    // Skill gem domain (SkillGems / GrantedEffects / GrantedEffectsPerLevel / ActiveSkills)
    let skills = skills::adapt_skills(&en, &tw)?;
    write_pretty(&base_dir.join("skill_gems.json"), &skills.gems)?;
    write_pretty(&base_dir.join("granted_effects.json"), &skills.effects)?;
    write_pretty(&base_dir.join("granted_effect_levels.json"), &skills.levels)?;
    write_pretty(
        &version_dir.join("i18n/zh-TW/skills.json"),
        &skills.zh_skill_names,
    )?;

    // Per-level damage stat sets (GrantedEffectStatSets* -> effect id -> per-level damage).
    let stat_sets = skills::adapt_stat_sets(&en)?;
    write_pretty(
        &base_dir.join("granted_effect_stat_sets.json"),
        &stat_sets.sets,
    )?;

    // Skill resource cost types (CostTypes -> the cost_types FK target, resolving Mana/Life/ES/... resource names).
    let cost_types = skills::adapt_cost_types(&en)?;
    write_pretty(&base_dir.join("cost_types.json"), &cost_types)?;

    // manifest: **merged** with the existing on-disk manifest — this step
    // only registers the base domains and the zh-TW language it produced;
    // overlay/generated/zh-CN etc. are maintained by the regen pipeline's
    // other steps, and rerunning just this step (e.g. `mise run data:adapt`)
    // must not wipe out those existing records.
    let manifest_path = version_dir.join("manifest.json");
    let mut manifest = fs::read_to_string(&manifest_path)
        .ok()
        .and_then(|s| serde_json::from_str::<DataManifest>(&s).ok())
        .unwrap_or_else(|| DataManifest {
            schema_version: CATALOG_SCHEMA_VERSION,
            poe_version: args.patch.clone(),
            languages: Vec::new(),
            domains: DomainSections::default(),
        });
    manifest.schema_version = CATALOG_SCHEMA_VERSION;
    manifest.poe_version = args.patch.clone();
    if !manifest.languages.iter().any(|l| l == "zh-TW") {
        manifest.languages.push("zh-TW".into());
        manifest.languages.sort();
    }
    for domain in [
        "base_items",
        "mods",
        "stats",
        "skill_gems",
        "granted_effects",
        "granted_effect_levels",
        "granted_effect_stat_sets",
        "cost_types",
    ] {
        if !manifest.domains.base.iter().any(|d| d == domain) {
            manifest.domains.base.push(domain.into());
        }
    }
    write_pretty(&manifest_path, &manifest)?;

    Ok(format!(
        "适配完成：base_items {}/{} 条（过滤 {} 个占位），zh-TW 名称 {} 条；\
         stats {} 条；mods {} 条（过滤 {} 个空壳），mods zh-TW 名称 {} 条；\
         skill_gems {}/{} 条，granted_effects {}/{} 条，\
         granted_effect_levels {} 个效果 / {} 行，zh-TW 技能名 {} 条，\
         granted_effect_stat_sets {} 个伤害效果 / {} 级（共 {} stat-set） → {}",
        bases.len(),
        total,
        total - bases.len(),
        i18n_zh.len(),
        stat_count,
        mod_count,
        mod_filtered,
        mod_zh,
        skills.gems.len(),
        skills.gems_total,
        skills.effects.len(),
        skills.effects_total,
        skills.levels.len(),
        skills.level_rows_total,
        skills.zh_skill_names.len(),
        stat_sets.sets.len(),
        stat_sets.damage_levels_total,
        stat_sets.sets_total,
        version_dir.display()
    ))
}

pub(crate) fn write_pretty<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| format!("序列化 {} 失败：{e}", path.display()))?;
    fs::write(path, format!("{json}\n")).map_err(|e| format!("写入 {} 失败：{e}", path.display()))
}
