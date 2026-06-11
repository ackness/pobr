use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sync_pob_catalog::buff_refs::run_check_buff_refs;
use sync_pob_catalog::extract_bases::{DEFAULT_BASE_FILES, run_extract_bases};
use sync_pob_catalog::extract_config_options::run_extract_config_options;
use sync_pob_catalog::extract_gem_effects::run_extract_gem_effects;
use sync_pob_catalog::extract_item_overlay::{
    DEFAULT_UNIQUE_FILES, run_extract_catalysts, run_extract_mod_scalability, run_extract_runes,
    run_extract_uniques,
};
use sync_pob_catalog::extract_lua::{
    DEFAULT_SKILL_FILES, DEFAULT_STAT_MAP_SKILL_FILES, ExtractLuaArgs, resolve_luajit,
    run_extract_lua,
};
use sync_pob_catalog::extract_minions::{
    MinionsKind, run_extract_minion_list, run_extract_minions,
};
use sync_pob_catalog::extract_quality::run_extract_gem_quality;
use sync_pob_catalog::extract_stat_map::run_extract_stat_map;
use sync_pob_catalog::extract_stat_set_labels::run_extract_stat_set_labels;
use sync_pob_catalog::mirage_configs::run_gen_mirage_configs;
use sync_pob_catalog::{
    CatalogDiff, check_against_fixture, collect_catalog, diff_catalogs, read_catalog, write_catalog,
};

const USAGE: &str = "usage:\n  sync-pob-catalog <scan|check|diff|fixture-check> --pob-root <path> [--out <path>] [--catalog <path>]\n  sync-pob-catalog extract-lua --vendor-root <path> [--what skill-overrides|gem-quality|stat-map|gem-effects|stat-set-labels|config-options|minions|spectres|minion-list|mod-scalability|runes|uniques|catalysts] [--out <path>] [--files <a,b,c>] [--luajit <path>] [--version-file <path>]\n  sync-pob-catalog extract-bases --vendor-root <path> [--out <path>] [--files <a,b,c>] [--luajit <path>] [--version-file <path>]\n  sync-pob-catalog check-buff-refs --vendor-root <path> --defs <path> [--write]\n  sync-pob-catalog gen-mirage-configs --vendor-root <path> [--out <path>] [--version-file <path>]";

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

// ---- extract-lua / extract-bases：vendor Lua → overlay JSON（确定性抽取通道）----

fn run_extract_command(command: &str, args: impl Iterator<Item = String>) -> io::Result<()> {
    let parsed = ExtractCliArgs::parse(args)?;
    // 缺省 --files：extract-bases 取基底数据文件集（Data/Bases）；extract-lua 按
    // `--what` 抽取目标取各自约定——stat-map 不含 minion/spectre（M1 蓝图 T2.1，
    // 召唤物 statMap 留 M5a）；gem-effects 恒读 Data/Gems.lua（--files 仅为公共
    // 调用层占位）；其余目标用全量技能文件。
    let default_files: &[&str] = if command == "extract-bases" {
        DEFAULT_BASE_FILES
    } else {
        match parsed.what.as_deref() {
            Some("stat-map") => DEFAULT_STAT_MAP_SKILL_FILES,
            Some("gem-effects") => &["Gems"],
            // config-options 恒读 Modules/ConfigOptions.lua（headless 引导，--files 仅占位）
            Some("config-options") => &["ConfigOptions"],
            // pre-M5 数据生产目标：minions/spectres/mod-scalability/runes/catalysts
            // 抽取文件固定（runner 内校验）；uniques 用 itemTypes 全集；minion-list
            // 复用全量技能文件（与 skill-overrides 同集）。
            Some("minions") => &["Minions"],
            Some("spectres") => &["Spectres"],
            Some("mod-scalability") => &["ModScalability"],
            Some("runes") => &["ModRunes"],
            Some("catalysts") => &["Item"],
            Some("uniques") => DEFAULT_UNIQUE_FILES,
            _ => DEFAULT_SKILL_FILES,
        }
    };
    let extract_args = ExtractLuaArgs {
        vendor_root: parsed.vendor_root,
        luajit: resolve_luajit(parsed.luajit.as_deref()),
        files: parsed
            .files
            .unwrap_or_else(|| default_files.iter().map(|s| s.to_string()).collect()),
        version_file: parsed.version_file,
        out_for_meta: parsed
            .out
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
    };
    // 抽取目标分发：extract-bases（基底物品覆盖值，M2-D1）；extract-lua 按 `--what`
    // ——skill-overrides（缺省，per-skill 覆盖值）/ gem-quality（宝石品质 stat 斜率，
    // M1-T1）/ stat-map（SkillStatMap 全局 + per-set 覆盖，M1-T2）/ gem-effects
    // （宝石→授予效果连边，M1-T5.1）/ stat-set-labels（M1-T5.2）/ config-options
    // （ConfigOptions 目录，M3 前置）。
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
            Some("gem-effects") => run_extract_gem_effects(&extract_args)?,
            Some("stat-set-labels") => run_extract_stat_set_labels(&extract_args)?,
            Some("config-options") => run_extract_config_options(&extract_args)?,
            Some("minions") => run_extract_minions(&extract_args, MinionsKind::Minions)?,
            Some("spectres") => run_extract_minions(&extract_args, MinionsKind::Spectres)?,
            Some("minion-list") => run_extract_minion_list(&extract_args)?,
            Some("mod-scalability") => run_extract_mod_scalability(&extract_args)?,
            Some("runes") => run_extract_runes(&extract_args)?,
            Some("uniques") => run_extract_uniques(&extract_args)?,
            Some("catalysts") => run_extract_catalysts(&extract_args)?,
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

// ---- gen-mirage-configs：工具内嵌 5 条 mirage 配置 → overlay JSON ----

fn run_gen_mirage_configs_command(args: impl Iterator<Item = String>) -> io::Result<()> {
    let parsed = ExtractCliArgs::parse(args)?;
    let extract_args = ExtractLuaArgs {
        vendor_root: parsed.vendor_root,
        luajit: resolve_luajit(parsed.luajit.as_deref()),
        // 仅为复用 ExtractLuaArgs 形状；本命令不执行 luajit。
        files: vec!["CalcMirages".to_string()],
        version_file: parsed.version_file,
        out_for_meta: parsed
            .out
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
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

#[derive(Debug)]
struct ExtractCliArgs {
    vendor_root: PathBuf,
    out: Option<PathBuf>,
    /// 显式 `--files` 列表；`None` = 按 `--what` 取各目标的缺省文件集。
    files: Option<Vec<String>>,
    luajit: Option<PathBuf>,
    version_file: Option<PathBuf>,
    /// 抽取目标（`None` = 缺省 skill-overrides）。
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

// ---- check-buff-refs：buff_definitions.json vendor 行段 hash 对账 ----

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

// ---- 既有 catalog 命令（scan/check/diff/fixture-check）----

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
