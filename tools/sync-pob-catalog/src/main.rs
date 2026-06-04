use std::env;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sync_pob_catalog::{
    CatalogDiff, check_against_fixture, collect_catalog, diff_catalogs, read_catalog, write_catalog,
};

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
    let args = Args::parse(env::args().skip(1))?;
    let catalog = collect_catalog(&args.pob_root)?;

    match args.command {
        Command::Scan => {
            if let Some(out) = args.out {
                write_catalog(&catalog, &out)
            } else {
                let json = serde_json::to_string_pretty(&catalog).map_err(io::Error::other)?;
                println!("{json}");
                Ok(())
            }
        }
        Command::Check => {
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
        Command::Diff => {
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
        Command::FixtureCheck => {
            let Some(catalog_path) = args.catalog else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "fixture-check requires --catalog <path>",
                ));
            };
            let diffs = check_against_fixture(&catalog_path, &catalog)?;
            report_diffs(&diffs, &catalog_path)
        }
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
struct Args {
    command: Command,
    pob_root: PathBuf,
    out: Option<PathBuf>,
    catalog: Option<PathBuf>,
}

#[derive(Debug)]
enum Command {
    Scan,
    Check,
    Diff,
    FixtureCheck,
}

impl Args {
    fn parse(mut args: impl Iterator<Item = String>) -> io::Result<Self> {
        let command = match args.next().as_deref() {
            Some("scan") => Command::Scan,
            Some("check") => Command::Check,
            Some("diff") => Command::Diff,
            Some("fixture-check") => Command::FixtureCheck,
            Some("--help") | Some("-h") | None => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "usage: sync-pob-catalog <scan|check|diff|fixture-check> --pob-root <path> [--out <path>] [--catalog <path>]",
                ));
            }
            Some(other) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown command: {other}"),
                ));
            }
        };

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
            command,
            pob_root,
            out,
            catalog,
        })
    }
}
