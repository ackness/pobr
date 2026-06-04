use std::env;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use sync_pob_catalog::{collect_catalog, read_catalog, write_catalog};

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
    }
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
}

impl Args {
    fn parse(mut args: impl Iterator<Item = String>) -> io::Result<Self> {
        let command = match args.next().as_deref() {
            Some("scan") => Command::Scan,
            Some("check") => Command::Check,
            Some("--help") | Some("-h") | None => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "usage: sync-pob-catalog <scan|check> --pob-root <path> [--out <path>] [--catalog <path>]",
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
