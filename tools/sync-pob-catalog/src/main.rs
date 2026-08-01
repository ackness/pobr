use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sync_pob_catalog::buff_refs::run_check_buff_refs;
use sync_pob_catalog::extract_bases::{DEFAULT_BASE_FILES, run_extract_bases};
use sync_pob_catalog::extract_config_options::run_extract_config_options;
use sync_pob_catalog::extract_curse_priority::run_extract_curse_priority;
use sync_pob_catalog::extract_gem_effects::run_extract_gem_effects;
use sync_pob_catalog::extract_item_overlay::{
    DEFAULT_UNIQUE_FILES, run_extract_catalysts, run_extract_mod_scalability, run_extract_runes,
    run_extract_uniques,
};
use sync_pob_catalog::extract_lua::{
    DEFAULT_SKILL_FILES, DEFAULT_STAT_MAP_SKILL_FILES, ExtractLuaArgs, canonical_out_for_meta,
    resolve_luajit, run_extract_lua,
};
use sync_pob_catalog::extract_minions::{
    MinionsKind, run_extract_minion_list, run_extract_minions,
};
use sync_pob_catalog::extract_parser_rules::{diff_parser_rules, run_extract_parser_rules};
use sync_pob_catalog::extract_quality::run_extract_gem_quality;
use sync_pob_catalog::extract_special_mods::run_extract_special_mods;
use sync_pob_catalog::extract_stat_descriptions::{
    DEFAULT_STAT_DESC_FILES, run_extract_stat_descriptions,
};
use sync_pob_catalog::extract_stat_map::run_extract_stat_map;
use sync_pob_catalog::extract_stat_set_labels::run_extract_stat_set_labels;
use sync_pob_catalog::gen_skill_types::run_gen_skill_types;
use sync_pob_catalog::gen_stat_id_map::run_gen_stat_id_map;
use sync_pob_catalog::mirage_configs::run_gen_mirage_configs;
use sync_pob_catalog::trigger_configs::run_gen_trigger_configs;
use sync_pob_catalog::{
    CatalogDiff, check_against_fixture, collect_catalog, diff_catalogs, read_catalog, write_catalog,
};

const USAGE: &str = "usage:\n  sync-pob-catalog <scan|check|diff|fixture-check> --pob-root <path> [--out <path>] [--catalog <path>]\n  sync-pob-catalog extract-lua --vendor-root <path> [--what skill-overrides|gem-quality|stat-map|stat-descriptions|gem-effects|stat-set-labels|config-options|curse-priority|minions|spectres|minion-list|mod-scalability|runes|uniques|catalysts|parser-rules|special-mods] [--out <path>] [--files <a,b,c>] [--luajit <path>] [--version-file <path>]\n  sync-pob-catalog extract-bases --vendor-root <path> [--out <path>] [--files <a,b,c>] [--luajit <path>] [--version-file <path>]\n  sync-pob-catalog check-buff-refs --vendor-root <path> --defs <path> [--write]\n  sync-pob-catalog gen-mirage-configs --vendor-root <path> [--out <path>] [--version-file <path>]\n  sync-pob-catalog gen-trigger-configs --vendor-root <path> [--out <path>] [--version-file <path>]\n  sync-pob-catalog gen-stat-id-map --overlay-dir <path> [--out <path>]\n  sync-pob-catalog gen-skill-types --vendor-root <path> --out <path>\n  sync-pob-catalog parser-rules-drift --vendor-root <path> --committed <path> [--luajit <path>] [--version-file <path>]";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("sync-pob-catalog: {error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> io::Result<()> {
    let mut raw_args = env::args().skip(1);
    let command = raw_args.next();
    match command.as_deref() {
        Some(cmd @ ("extract-lua" | "extract-bases")) => run_extract_command(cmd, raw_args),
        Some("check-buff-refs") => run_check_buff_refs_command(raw_args),
        Some("gen-mirage-configs") => run_gen_mirage_configs_command(raw_args),
        Some("gen-trigger-configs") => run_gen_trigger_configs_command(raw_args),
        Some("gen-stat-id-map") => run_gen_stat_id_map_command(raw_args),
        Some("gen-skill-types") => run_gen_skill_types_command(raw_args),
        Some("parser-rules-drift") => run_parser_rules_drift_command(raw_args),
        Some(other @ ("scan" | "check" | "diff" | "fixture-check")) => {
            run_catalog_command(other, raw_args)
        }
        Some("--help") | Some("-h") | None => {
            Err(io::Error::new(io::ErrorKind::InvalidInput, USAGE))
        }
        Some(other) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown command: {other}\n{USAGE}"),
        )),
    }
}

// extract-lua / extract-bases: vendor Lua -> overlay JSON (the deterministic extraction channel)

fn run_extract_command(command: &str, args: impl Iterator<Item = String>) -> io::Result<()> {
    let parsed = ExtractCliArgs::parse(args)?;
    // Default --files: extract-bases uses the base-item data file set
    // (Data/Bases); extract-lua picks its own set per `--what` target —
    // stat-map excludes minion/spectre (summons have their own statMap
    // targets); gem-effects always reads Data/Gems.lua (--files is just a
    // placeholder for the shared call layer); other targets use the full skill file set.
    let default_files: &[&str] = if command == "extract-bases" {
        DEFAULT_BASE_FILES
    } else {
        match parsed.what.as_deref() {
            Some("stat-map") => DEFAULT_STAT_MAP_SKILL_FILES,
            // stat-descriptions: root + passive + presence/aura (the tree channel)
            Some("stat-descriptions") => DEFAULT_STAT_DESC_FILES,
            Some("gem-effects") => &["Gems"],
            // config-options always reads Modules/ConfigOptions.lua (headless bootstrap, --files is a placeholder)
            Some("config-options") => &["ConfigOptions"],
            // curse-priority always reads the data.cursePriority table literal in Modules/Data.lua (-C)
            Some("curse-priority") => &["Data"],
            // Data-production targets: minions/spectres/mod-scalability/runes/catalysts
            // have fixed extraction files (validated inside the runner);
            // uniques uses the full itemTypes set; minion-list reuses the
            // full skill file set (same as skill-overrides).
            Some("minions") => &["Minions"],
            Some("spectres") => &["Spectres"],
            Some("mod-scalability") => &["ModScalability"],
            Some("runes") => &["ModRunes"],
            Some("catalysts") => &["Item"],
            Some("uniques") => DEFAULT_UNIQUE_FILES,
            // parser-rules / special-mods always read Modules/ModParser.lua
            // (full headless bootstrap, --files is a placeholder)
            Some("parser-rules") | Some("special-mods") => &["ModParser"],
            _ => DEFAULT_SKILL_FILES,
        }
    };
    // F1: decouple `_meta.regen_command`'s `--out` from the actual argument —
    // normalize it to a canonical repo-relative path per what-target /
    // subcommand (replaying against a temp path no longer creates a self-referential diff).
    let meta_target = if command == "extract-bases" {
        "bases"
    } else {
        parsed.what.as_deref().unwrap_or("skill-overrides")
    };
    let out_for_meta = canonical_out_for_meta(
        parsed.out.as_deref(),
        meta_target,
        parsed.version_file.as_deref(),
    );
    let extract_args = ExtractLuaArgs {
        vendor_root: parsed.vendor_root,
        luajit: resolve_luajit(parsed.luajit.as_deref()),
        files: parsed
            .files
            .unwrap_or_else(|| default_files.iter().map(|s| s.to_string()).collect()),
        version_file: parsed.version_file,
        out_for_meta,
    };
    // Extraction target dispatch: extract-bases (base item overrides);
    // extract-lua by `--what` — skill-overrides (default, per-skill
    // overrides) / gem-quality (gem quality stat slopes) / stat-map
    // (SkillStatMap global + per-set overrides) / gem-effects (gem ->
    // granted-effect links) / stat-set-labels / config-options (the
    // ConfigOptions catalog) / parser-rules (the six ModParser parse-rule tables).
    let json = if command == "extract-bases" {
        if let Some(what) = parsed.what.as_deref() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("extract-bases 不支持 --what {what}\n{USAGE}"),
            ));
        }
        run_extract_bases(&extract_args)?
    } else {
        match parsed.what.as_deref() {
            None | Some("skill-overrides") => run_extract_lua(&extract_args)?,
            Some("gem-quality") => run_extract_gem_quality(&extract_args)?,
            Some("stat-map") => run_extract_stat_map(&extract_args)?,
            Some("stat-descriptions") => run_extract_stat_descriptions(&extract_args)?,
            Some("gem-effects") => run_extract_gem_effects(&extract_args)?,
            Some("stat-set-labels") => run_extract_stat_set_labels(&extract_args)?,
            Some("config-options") => run_extract_config_options(&extract_args)?,
            Some("curse-priority") => run_extract_curse_priority(&extract_args)?,
            Some("minions") => run_extract_minions(&extract_args, MinionsKind::Minions)?,
            Some("spectres") => run_extract_minions(&extract_args, MinionsKind::Spectres)?,
            Some("minion-list") => run_extract_minion_list(&extract_args)?,
            Some("mod-scalability") => run_extract_mod_scalability(&extract_args)?,
            Some("runes") => run_extract_runes(&extract_args)?,
            Some("uniques") => run_extract_uniques(&extract_args)?,
            Some("catalysts") => run_extract_catalysts(&extract_args)?,
            Some("parser-rules") => run_extract_parser_rules(&extract_args)?,
            Some("special-mods") => run_extract_special_mods(&extract_args)?,
            Some(other) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("extract-lua 未知抽取目标 --what {other}\n{USAGE}"),
                ));
            }
        }
    };
    match parsed.out {
        Some(out) => {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&out, json)?;
            eprintln!("{command}: wrote {}", out.display());
            Ok(())
        }
        None => {
            print!("{json}");
            Ok(())
        }
    }
}

// gen-mirage-configs: the tool's 5 embedded mirage configs -> overlay JSON

fn run_gen_mirage_configs_command(args: impl Iterator<Item = String>) -> io::Result<()> {
    let parsed = ExtractCliArgs::parse(args)?;
    // F1: same as run_extract_command -- normalize `--out` to a canonical relative path before it goes into _meta.
    let out_for_meta = canonical_out_for_meta(
        parsed.out.as_deref(),
        "mirage-configs",
        parsed.version_file.as_deref(),
    );
    let extract_args = ExtractLuaArgs {
        vendor_root: parsed.vendor_root,
        luajit: resolve_luajit(parsed.luajit.as_deref()),
        // Only here to reuse ExtractLuaArgs' shape; this command doesn't run luajit.
        files: vec!["CalcMirages".to_string()],
        version_file: parsed.version_file,
        out_for_meta,
    };
    let json = run_gen_mirage_configs(&extract_args)?;
    match parsed.out {
        Some(out) => {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&out, json)?;
            eprintln!("gen-mirage-configs: wrote {}", out.display());
            Ok(())
        }
        None => {
            print!("{json}");
            Ok(())
        }
    }
}

// gen-trigger-configs: the tool's 61 embedded trigger configs -> overlay JSON

fn run_gen_trigger_configs_command(args: impl Iterator<Item = String>) -> io::Result<()> {
    let parsed = ExtractCliArgs::parse(args)?;
    let extract_args = ExtractLuaArgs {
        vendor_root: parsed.vendor_root,
        luajit: resolve_luajit(parsed.luajit.as_deref()),
        // Only here to reuse ExtractLuaArgs' shape; this command doesn't run
        // luajit (it reconciles directly against the source).
        files: vec!["CalcTriggers".to_string()],
        version_file: parsed.version_file,
        out_for_meta: parsed
            .out
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
    };
    let json = run_gen_trigger_configs(&extract_args)?;
    match parsed.out {
        Some(out) => {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&out, json)?;
            eprintln!("gen-trigger-configs: wrote {}", out.display());
            Ok(())
        }
        None => {
            print!("{json}");
            Ok(())
        }
    }
}

// gen-stat-id-map: consumes two overlays and runs the engine to derive stat_id -> modifier

fn run_gen_stat_id_map_command(mut args: impl Iterator<Item = String>) -> io::Result<()> {
    let mut overlay_dir = None;
    let mut out = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--overlay-dir" => overlay_dir = args.next().map(PathBuf::from),
            "--out" => out = args.next().map(PathBuf::from),
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown argument: {other}\n{USAGE}"),
                ));
            }
        }
    }
    let Some(overlay_dir) = overlay_dir else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("gen-stat-id-map 缺少 --overlay-dir <path>\n{USAGE}"),
        ));
    };
    // Normalize _meta.regen_command's --out to a canonical relative path (consistent with the other targets).
    let out_for_meta = canonical_out_for_meta(out.as_deref(), "stat-id-map", None);
    let json = run_gen_stat_id_map(&overlay_dir, out_for_meta)?;
    match out {
        Some(out) => {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&out, json)?;
            eprintln!("gen-stat-id-map: wrote {}", out.display());
            Ok(())
        }
        None => {
            print!("{json}");
            Ok(())
        }
    }
}

// gen-skill-types (data-driven A1): the full SkillType enum from Global.lua -> a pobr-data static table

fn run_gen_skill_types_command(mut args: impl Iterator<Item = String>) -> io::Result<()> {
    let mut vendor_root = None;
    let mut out = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--vendor-root" => vendor_root = args.next().map(PathBuf::from),
            "--out" => out = args.next().map(PathBuf::from),
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown argument: {other}\n{USAGE}"),
                ));
            }
        }
    }
    let (Some(vendor_root), Some(out)) = (vendor_root, out) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("gen-skill-types 需要 --vendor-root <path> 与 --out <path>\n{USAGE}"),
        ));
    };
    let message = run_gen_skill_types(&vendor_root, &out)?;
    eprintln!("{message}");
    Ok(())
}

#[derive(Debug)]
struct ExtractCliArgs {
    vendor_root: PathBuf,
    out: Option<PathBuf>,
    /// Explicit `--files` list; `None` means use each target's default file set based on `--what`.
    files: Option<Vec<String>>,
    luajit: Option<PathBuf>,
    version_file: Option<PathBuf>,
    /// Extraction target (`None` defaults to skill-overrides).
    what: Option<String>,
}

impl ExtractCliArgs {
    fn parse(mut args: impl Iterator<Item = String>) -> io::Result<Self> {
        let mut vendor_root = None;
        let mut out = None;
        let mut files = None;
        let mut luajit = None;
        let mut version_file = None;
        let mut what = None;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--what" => what = args.next(),
                "--vendor-root" => vendor_root = args.next().map(PathBuf::from),
                "--out" => out = args.next().map(PathBuf::from),
                "--files" => {
                    files = args.next().map(|list| {
                        list.split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                }
                "--luajit" => luajit = args.next().map(PathBuf::from),
                "--version-file" => version_file = args.next().map(PathBuf::from),
                other => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("unknown argument: {other}\n{USAGE}"),
                    ));
                }
            }
        }
        let Some(vendor_root) = vendor_root else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("extract-lua 缺少 --vendor-root <path>\n{USAGE}"),
            ));
        };
        Ok(Self {
            vendor_root,
            out,
            files,
            luajit,
            version_file,
            what,
        })
    }
}

// check-buff-refs: reconcile buff_definitions.json against vendor line-range hashes

fn run_check_buff_refs_command(mut args: impl Iterator<Item = String>) -> io::Result<()> {
    let mut vendor_root = None;
    let mut defs = None;
    let mut write = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--vendor-root" => vendor_root = args.next().map(PathBuf::from),
            "--defs" => defs = args.next().map(PathBuf::from),
            "--write" => write = true,
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown argument: {other}\n{USAGE}"),
                ));
            }
        }
    }
    let (Some(vendor_root), Some(defs)) = (vendor_root, defs) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("check-buff-refs 需要 --vendor-root <path> 与 --defs <path>\n{USAGE}"),
        ));
    };
    let drifts = run_check_buff_refs(&vendor_root, &defs, write)?;
    if drifts.is_empty() {
        eprintln!("check-buff-refs: 全部 vendor_ref 行段 hash 一致");
        return Ok(());
    }
    for drift in &drifts {
        eprintln!(
            "check-buff-refs: DRIFT `{}` 登记 {} 实算 {}",
            drift.id,
            drift.recorded,
            drift.actual.as_deref().unwrap_or("<行号越界>")
        );
    }
    if write {
        eprintln!(
            "check-buff-refs: 已回写 {} 条 hash 到 {}（请人工复核归纳内容仍忠实 vendor）",
            drifts.len(),
            defs.display()
        );
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "check-buff-refs: {} 条 vendor_ref 行段漂移（vendor 升级后须人工复核 + --write 刷新）",
            drifts.len()
        )))
    }
}

// parser-rules drift diff: byte-diff freshly re-extracted output against what's committed (task 3)

fn run_parser_rules_drift_command(mut args: impl Iterator<Item = String>) -> io::Result<()> {
    let mut vendor_root = None;
    let mut committed = None;
    let mut luajit = None;
    let mut version_file = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--vendor-root" => vendor_root = args.next().map(PathBuf::from),
            "--committed" => committed = args.next().map(PathBuf::from),
            "--luajit" => luajit = args.next().map(PathBuf::from),
            "--version-file" => version_file = args.next().map(PathBuf::from),
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown argument: {other}\n{USAGE}"),
                ));
            }
        }
    }
    let Some(vendor_root) = vendor_root else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("parser-rules-drift 缺少 --vendor-root <path>\n{USAGE}"),
        ));
    };
    let Some(committed) = committed else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("parser-rules-drift 缺少 --committed <path>\n{USAGE}"),
        ));
    };

    let committed_text = fs::read_to_string(&committed).map_err(|error| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("无法读取已提交文件 {}：{error}", committed.display()),
        )
    })?;
    // Trust the committed file's own recorded _meta.regen_command --out value
    // (decoupled from the --committed path spelling, to avoid absolute/relative path differences causing false drift).
    let committed_out = serde_json::from_str::<serde_json::Value>(&committed_text)
        .ok()
        .and_then(|doc| {
            let regen = doc
                .get("_meta")?
                .get("regen_command")?
                .as_str()?
                .to_string();
            let (_, out) = regen.split_once(" --out ")?;
            Some(out.trim().to_string())
        });
    // Fallback (when the committed file lacks a recorded --out value): normalize from the --committed path (F1).
    let out_for_meta = committed_out.or_else(|| {
        canonical_out_for_meta(Some(&committed), "parser-rules", version_file.as_deref())
    });
    let extract_args = ExtractLuaArgs {
        vendor_root,
        luajit: resolve_luajit(luajit.as_deref()),
        files: vec!["ModParser".to_string()],
        version_file,
        out_for_meta,
    };
    let regenerated = run_extract_parser_rules(&extract_args)?;
    let drift = diff_parser_rules(&committed_text, &regenerated)?;
    if drift.identical {
        println!(
            "parser-rules-drift: no drift detected against {}",
            committed.display()
        );
        return Ok(());
    }
    for line in &drift.lines {
        println!("{line}");
    }
    Err(io::Error::other(format!(
        "parser-rules drift detected against {}（{} 处差异摘要）",
        committed.display(),
        drift.lines.len()
    )))
}

// The pre-existing catalog commands (scan/check/diff/fixture-check)

fn run_catalog_command(command: &str, args: impl Iterator<Item = String>) -> io::Result<()> {
    let args = CatalogCliArgs::parse(args)?;
    let catalog = collect_catalog(&args.pob_root)?;

    match command {
        "scan" => {
            if let Some(out) = args.out {
                write_catalog(&catalog, &out)
            } else {
                let json = serde_json::to_string_pretty(&catalog).map_err(io::Error::other)?;
                println!("{json}");
                Ok(())
            }
        }
        "check" => {
            let Some(catalog_path) = args.catalog else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "check requires --catalog <path>",
                ));
            };
            let expected = read_catalog(&catalog_path)?;
            if expected == catalog {
                Ok(())
            } else {
                Err(io::Error::other(format!(
                    "catalog drift detected against {}",
                    catalog_path.display()
                )))
            }
        }
        "diff" => {
            let Some(catalog_path) = args.catalog else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "diff requires --catalog <path>",
                ));
            };
            let expected = read_catalog(&catalog_path)?;
            let diffs = diff_catalogs(&expected, &catalog);
            report_diffs(&diffs, &catalog_path)
        }
        "fixture-check" => {
            let Some(catalog_path) = args.catalog else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "fixture-check requires --catalog <path>",
                ));
            };
            let diffs = check_against_fixture(&catalog_path, &catalog)?;
            report_diffs(&diffs, &catalog_path)
        }
        _ => unreachable!("run() 已过滤合法命令"),
    }
}

fn report_diffs(diffs: &[CatalogDiff], catalog_path: &Path) -> io::Result<()> {
    if diffs.is_empty() {
        println!("no drift detected against {}", catalog_path.display());
        return Ok(());
    }

    for diff in diffs {
        println!("{:?} {}: {}", diff.kind, diff.key, diff.detail);
    }
    Err(io::Error::other(format!(
        "{} catalog difference(s) detected against {}",
        diffs.len(),
        catalog_path.display()
    )))
}

#[derive(Debug)]
struct CatalogCliArgs {
    pob_root: PathBuf,
    out: Option<PathBuf>,
    catalog: Option<PathBuf>,
}

impl CatalogCliArgs {
    fn parse(mut args: impl Iterator<Item = String>) -> io::Result<Self> {
        let mut pob_root = None;
        let mut out = None;
        let mut catalog = None;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--pob-root" => pob_root = args.next().map(PathBuf::from),
                "--out" => out = args.next().map(PathBuf::from),
                "--catalog" => catalog = args.next().map(PathBuf::from),
                other => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("unknown argument: {other}"),
                    ));
                }
            }
        }

        let Some(pob_root) = pob_root else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "missing --pob-root <path>",
            ));
        };

        Ok(Self {
            pob_root,
            out,
            catalog,
        })
    }
}
