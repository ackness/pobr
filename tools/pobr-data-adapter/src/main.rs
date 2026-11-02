//! pobr-data-adapter: adapts authenticated owner facts into PoBR's runtime JSON.
//!
//! Inputs are pinned RePoE publications (ordinary and native physical facts,
//! the historical official tree package) plus PoBR's own domain policies. It
//! never reads vendor Lua or a local raw-table export.
//!
//! Usage:
//! ```text
//! # Ordinary domains from pinned physical facts (see ordinary-facts.md)
//! cargo run -p pobr-data-adapter -- --ordinary-facts <F> --facts-manifest-sha256 <HEX> --version <patch> --out <fresh-directory>
//! # Native facts and native runtime (see pipeline/native/)
//! cargo run -p pobr-data-adapter -- --native-facts <F> ...
//! cargo run -p pobr-data-adapter -- --native-runtime <F> ...
//! # Passive tree domain candidate from the pinned official tree export (data:tree:candidate)
//! cargo run -p pobr-data-adapter -- --tree <data.json> --out <dir> --patch <patch>
//! # Keystone-derived special rules from a prepared passive_tree.json (regen-check)
//! cargo run -p pobr-data-adapter -- --emit-special-derived <passive_tree.json> --out <dir> --patch <patch>
//! ```

mod special_derived;
mod tree;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Deserialize;

mod monsters;
mod native_facts;
mod ordinary_facts;
mod raw_source;
mod skills;

fn main() -> ExitCode {
    let result = match parse_args() {
        Ok(Mode::Tree(args)) => tree::run(args),
        Ok(Mode::SpecialDerived(args)) => special_derived::run(args),
        Ok(Mode::NativeFacts(args)) => native_facts::run(args),
        Ok(Mode::NativeRuntime(args)) => native_facts::runtime::run(*args),
        Ok(Mode::OrdinaryFacts(args)) => ordinary_facts::run(*args),
        Err(err) => Err(err),
    };
    match result {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("pobr-data-adapter failed: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Independent data-domain adapter modes, mutually exclusive.
enum Mode {
    OrdinaryFacts(Box<ordinary_facts::Args>),
    NativeRuntime(Box<native_facts::runtime::Args>),
    NativeFacts(native_facts::Args),
    Tree(tree::TreeArgs),
    /// Derives `generated/special_derived.json` from the existing `passive_tree.json`'s keystone nodes.
    SpecialDerived(special_derived::SpecialDerivedArgs),
}

fn parse_args() -> Result<Mode, String> {
    parse_args_from(std::env::args().skip(1))
}

fn parse_args_from(args: impl Iterator<Item = String>) -> Result<Mode, String> {
    let args: Vec<_> = args.collect();
    if args.iter().any(|arg| arg == "--ordinary-facts") {
        return ordinary_facts::parse_args(args.into_iter())
            .map(|args| Mode::OrdinaryFacts(Box::new(args)));
    }
    if args.iter().any(|arg| arg == "--native-runtime") {
        return native_facts::runtime::parse_args(args.into_iter())
            .map(Box::new)
            .map(Mode::NativeRuntime);
    }
    if args.iter().any(|arg| arg == "--native-facts") {
        return native_facts::parse_args(args.into_iter()).map(Mode::NativeFacts);
    }
    let mut tree = None;
    let mut special_derived = None;
    let mut out = None;
    let mut patch = None;
    let mut it = args.into_iter();
    while let Some(flag) = it.next() {
        let mut take = |name: &str| {
            it.next()
                .ok_or_else(|| format!("{name} is missing a value"))
        };
        match flag.as_str() {
            "--tree" => tree = Some(PathBuf::from(take("--tree")?)),
            "--emit-special-derived" => {
                special_derived = Some(PathBuf::from(take("--emit-special-derived")?))
            }
            "--out" => out = Some(PathBuf::from(take("--out")?)),
            "--patch" => patch = Some(take("--patch")?),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    let out = out.ok_or("missing --out <data>")?;
    let patch = patch.ok_or("missing --patch <version>")?;
    match (tree, special_derived) {
        (Some(_), Some(_)) => Err(
            "--tree / --emit-special-derived are mutually exclusive, run them separately".into(),
        ),
        (Some(data_json), None) => Ok(Mode::Tree(tree::TreeArgs {
            data_json,
            out,
            patch,
        })),
        (None, Some(tree_json)) => Ok(Mode::SpecialDerived(special_derived::SpecialDerivedArgs {
            tree_json,
            out,
            patch,
        })),
        (None, None) => Err(
            "missing --ordinary-facts / --native-facts / --native-runtime / --tree <data.json> / \
             --emit-special-derived <passive_tree.json>"
                .into(),
        ),
    }
}

pub(crate) fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("failed to parse {}: {e}", path.display()))
}

pub(crate) fn resolve(lookup: &[String], idx: usize) -> Option<String> {
    lookup.get(idx).filter(|s| !s.is_empty()).cloned()
}

/// A dev-only placeholder / not-yet-enabled entry (excluded from the catalog).
pub(crate) fn is_placeholder(name: &str) -> bool {
    name.is_empty() || name.contains("[DNT") || name.contains("[UNUSED") || name.contains("[OLD")
}

pub(crate) fn write_pretty<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| format!("failed to serialize {}: {e}", path.display()))?;
    fs::write(path, format!("{json}\n"))
        .map_err(|e| format!("failed to write {}: {e}", path.display()))
}
