use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sync_pob_catalog::extract_lua::{
    DEFAULT_SKILL_FILES, ExtractLuaArgs, resolve_luajit, run_extract_lua,
};
use sync_pob_catalog::{
    CatalogDiff, check_against_fixture, collect_catalog, diff_catalogs, read_catalog, write_catalog,
};

const USAGE: &str = "usage:\n  sync-pob-catalog <scan|check|diff|fixture-check> --pob-root <path> [--out <path>] [--catalog <path>]\n  sync-pob-catalog extract-lua --vendor-root <path> [--out <path>] [--files <a,b,c>] [--luajit <path>] [--version-file <path>]";

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
        Some("extract-lua") => run_extract_lua_command(raw_args),
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

// ---- extract-lua：vendor Lua → overlay JSON（确定性抽取通道）----

fn run_extract_lua_command(args: impl Iterator<Item = String>) -> io::Result<()> {
    let parsed = ExtractCliArgs::parse(args)?;
    let extract_args = ExtractLuaArgs {
        vendor_root: parsed.vendor_root,
        luajit: resolve_luajit(parsed.luajit.as_deref()),
        files: parsed.files,
        version_file: parsed.version_file,
        out_for_meta: parsed
            .out
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
    };
    let json = run_extract_lua(&extract_args)?;
    match parsed.out {
        Some(out) => {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&out, json)?;
            eprintln!("extract-lua: wrote {}", out.display());
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
    files: Vec<String>,
    luajit: Option<PathBuf>,
    version_file: Option<PathBuf>,
}

impl ExtractCliArgs {
    fn parse(mut args: impl Iterator<Item = String>) -> io::Result<Self> {
        let mut vendor_root = None;
        let mut out = None;
        let mut files = None;
        let mut luajit = None;
        let mut version_file = None;
        while let Some(arg) = args.next() {
            match arg.as_str() {
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
            files: files
                .unwrap_or_else(|| DEFAULT_SKILL_FILES.iter().map(|s| s.to_string()).collect()),
            luajit,
            version_file,
        })
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
