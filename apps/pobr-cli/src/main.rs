//! pobr-cli: calculation, native conditional evaluation, parsing and build-code commands.
//!
//! `main` only glues together IO: clap parses args, reads the file / stdin,
//! calls into `pobr_cli` library functions, and prints JSON. All command
//! logic (pure functions with serializable output) lives in the `pobr_cli`
//! library layer, so it's easy to unit test.

use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use pobr_build::native_document::MAX_NATIVE_BUILD_BYTES;
use pobr_cli::{CalculateBuildRequest, CalculateRequest, ParseItemRequest};
use pobr_core::calc::MinimalInput;
use pobr_data::monster::EnemyTier;

#[derive(Debug, Parser)]
#[command(
    name = "pobr",
    about = "Path of Building in Rust — calculation / parsing CLI",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run a minimal calc from base stats plus mod text, printing the key fields as JSON.
    Calculate(CalculateArgs),
    /// Run a full calc from a PoB build code — gear, tree, gems, character base and
    /// enemy — printing the result as JSON, or with --breakdown how it arrives at
    /// the stats asked for, as text.
    CalculateBuild(CalculateBuildArgs),
    /// Evaluate a bound native JSON request against one explicit native dataset.
    /// Always prints the complete conditional result as JSON; no full DPS.
    CalculateNative(CalculateNativeArgs),
    /// Parse one mod line and print what the parser made of it.
    ParseMod(ParseModArgs),
    /// Break one mod line all the way down: name, type and value plus the flags and
    /// tags that decide when it applies and how it scales, each explained in prose.
    ExplainMod(ExplainModArgs),
    /// Parse raw item text from --text, --file or stdin. Not implemented yet; returns
    /// an error.
    ParseItem(ParseItemArgs),
    /// Decode a PoB build code into its XML.
    DecodeCode(CodeArgs),
    /// Encode XML from --text, --file or stdin into a PoB build code.
    EncodeCode(TextSourceArgs),
}

#[derive(Debug, Args)]
struct CalculateArgs {
    /// Base life.
    #[arg(long, default_value_t = 0.0)]
    base_life: f64,
    /// Base mana.
    #[arg(long, default_value_t = 0.0)]
    base_mana: f64,
    /// Base fire resistance.
    #[arg(long, default_value_t = 0.0)]
    base_fire_resistance: f64,
    /// Base cold resistance.
    #[arg(long, default_value_t = 0.0)]
    base_cold_resistance: f64,
    /// Base lightning resistance.
    #[arg(long, default_value_t = 0.0)]
    base_lightning_resistance: f64,
    /// Base accuracy.
    #[arg(long, default_value_t = 0.0)]
    base_accuracy: f64,
    /// The enemy's evasion.
    #[arg(long, default_value_t = 0.0)]
    enemy_evasion: f64,
    /// Minimum base hit damage.
    #[arg(long, default_value_t = 0.0)]
    base_hit_min: f64,
    /// Maximum base hit damage.
    #[arg(long, default_value_t = 0.0)]
    base_hit_max: f64,
    /// Base action rate, in actions per second.
    #[arg(long, default_value_t = 0.0)]
    base_action_rate: f64,
    /// A mod line, given inline. Repeatable.
    #[arg(long = "mod", value_name = "TEXT")]
    mods: Vec<String>,
    /// Read mod lines from a file, one per line. Merged with any --mod values.
    #[arg(long)]
    mods_file: Option<String>,
}

#[derive(Debug, Args)]
struct CalculateBuildArgs {
    /// Evaluate every skill marked for Full DPS (additional calculation work).
    #[arg(long)]
    full_dps: bool,
    /// The build code, given inline. Use this or --file; with neither, reads stdin.
    code: Option<String>,
    /// Read the build code from a file.
    #[arg(long)]
    file: Option<String>,
    /// Directory holding one version's game data JSON. Defaults to the copy
    /// shipped in the repo.
    #[arg(long)]
    data_dir: Option<String>,
    /// Enemy level. 0 means match the character's level.
    #[arg(long, default_value_t = 0)]
    enemy_level: u32,
    /// Enemy tier: none, boss, pinnacle or uber.
    #[arg(long, default_value = "pinnacle")]
    enemy_tier: String,
    /// Report panel numbers, which ignore enemy interaction. Defaults to effective DPS.
    #[arg(long, default_value_t = false)]
    panel: bool,
    /// Break these stats down: comma-separated display stat ids (TotalDPS,Life),
    /// `all` for the panel's set, or modifier names. Prints the steps of each and
    /// the modifier lines it reads as text, unless --json is given.
    #[arg(long, value_delimiter = ',', value_name = "STATS")]
    breakdown: Vec<String>,
    /// With --breakdown or --usage, print the JSON report instead of text.
    #[arg(long)]
    json: bool,
    /// Calculate on the basis of the code's reference values: the models their
    /// exporter lacks (Energy triggering, a rain's landing arrows) are off, for
    /// the output and the breakdowns alike.
    #[arg(long)]
    reference_basis: bool,
    /// Report what the calculation did with each of the build's lines: applied
    /// (and the stats each reaches), left out and why, never read, or not
    /// parsed, with the item, node or config holding it. As text, the lines not
    /// fully used; with --json, every line.
    #[arg(long)]
    usage: bool,
}

#[derive(Debug, Args)]
struct CalculateNativeArgs {
    /// Exact native version root matching the saved binding; no CURRENT/environment fallback.
    #[arg(long, value_name = "VERSION_ROOT")]
    data_dir: PathBuf,
    /// Strict pobr-native-build/v1 JSON file (at most 16 MiB); no stdin or imports.
    #[arg(long, value_name = "NATIVE_BUILD_JSON")]
    file: PathBuf,
}

#[derive(Debug, Args)]
struct ParseModArgs {
    /// The mod line to parse.
    text: String,
    /// Version data directory to compile the parser rules from. Defaults to the
    /// copy shipped in the repo.
    #[arg(long)]
    data_dir: Option<String>,
}

#[derive(Debug, Args)]
struct ExplainModArgs {
    /// The mod line to break down.
    text: String,
    /// Version data directory to compile the parser rules from. Defaults to the
    /// copy shipped in the repo.
    #[arg(long)]
    data_dir: Option<String>,
    /// Emit JSON instead of the default human-readable text.
    #[arg(long)]
    json: bool,
    /// Also compute what this mod is worth on the given build, by calculating with
    /// and without it.
    #[arg(long)]
    build: Option<String>,
    /// Read the build code from a file instead of --build.
    #[arg(long)]
    build_file: Option<String>,
    /// Enemy level for the marginal calc. 0 means match the character's level.
    #[arg(long, default_value_t = 0)]
    enemy_level: u32,
    /// Enemy tier for the marginal calc: none, boss, pinnacle or uber.
    #[arg(long, default_value = "pinnacle")]
    enemy_tier: String,
    /// Use panel numbers for the marginal calc. Defaults to effective DPS.
    #[arg(long, default_value_t = false)]
    panel: bool,
}

#[derive(Debug, Args)]
struct ParseItemArgs {
    /// The raw item text, given inline.
    #[arg(long)]
    text: Option<String>,
    /// Read the raw item text from a file.
    #[arg(long)]
    file: Option<String>,
}

#[derive(Debug, Args)]
struct CodeArgs {
    /// The build code to decode.
    code: String,
}

#[derive(Debug, Args)]
struct TextSourceArgs {
    /// The text, given inline.
    #[arg(long)]
    text: Option<String>,
    /// Read the text from a file.
    #[arg(long)]
    file: Option<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<String, Box<dyn std::error::Error>> {
    match cli.command {
        Command::Calculate(args) => Ok(pobr_cli::calculate_json(&build_calculate_request(args)?)?),
        Command::CalculateBuild(args) => {
            let as_text = (!args.breakdown.is_empty() || args.usage) && !args.json;
            let detail = pobr_cli::BuildDetail {
                full_dps: args.full_dps,
                breakdown: args.breakdown.clone(),
                reference_basis: args.reference_basis,
                usage: args.usage,
            };
            let report =
                pobr_cli::calculate_build_detailed(&build_calc_build_request(args)?, &detail)?;
            // gap B: tree-version mismatches (allocated nodes not in the
            // loaded tree) are no longer silent — warn to stderr.
            let diag = &report.tree_version;
            if diag.unknown_node_count > 0 {
                eprintln!(
                    "warning: {} allocated passive node(s) not in the loaded tree (build treeVersion={}), \
                     their contribution was silently skipped: {:?}",
                    diag.unknown_node_count,
                    diag.build_tree_version.as_deref().unwrap_or("?"),
                    diag.unknown_nodes,
                );
            }
            for item in &report.missing_items {
                eprintln!(
                    "warning: could not read the item in {:?} ({}): {}; the result is calculated without it",
                    item.used_in, item.name, item.reason,
                );
            }
            if as_text {
                return Ok(pobr_cli::render_breakdowns(&report));
            }
            Ok(serde_json::to_string_pretty(&report)?)
        }
        Command::CalculateNative(args) => {
            let bytes = read_native_document(fs::File::open(args.file)?)?;
            let data = pobr_gamedata::GameData::new(args.data_dir);
            Ok(pobr_cli::native::calculate_native_document(&data, &bytes)?)
        }
        Command::ParseMod(args) => {
            let data_dir = match args.data_dir {
                Some(dir) => std::path::PathBuf::from(dir),
                None => pobr_gamedata::current_data_dir(),
            };
            Ok(pobr_cli::parse_mod_json(&args.text, &data_dir)?)
        }
        Command::ExplainMod(args) => {
            let data_dir = match args.data_dir {
                Some(dir) => std::path::PathBuf::from(dir),
                None => pobr_gamedata::current_data_dir(),
            };
            let has_build = args.build.is_some() || args.build_file.is_some();
            if !has_build {
                return if args.json {
                    Ok(pobr_cli::explain_mod_json(&args.text, &data_dir)?)
                } else {
                    Ok(pobr_cli::explain_mod_text(&args.text, &data_dir)?)
                };
            }
            // has_build guarantees at least one of build / build_file is Some, so read_text_source never falls through to stdin.
            let build_code = read_text_source(args.build, args.build_file)?;
            let req = pobr_cli::MarginalRequest {
                build_code,
                data_dir,
                enemy_level: args.enemy_level,
                enemy_tier: parse_enemy_tier(&args.enemy_tier)?,
                mode_effective: !args.panel,
                mod_texts: vec![args.text.clone()],
            };
            if args.json {
                Ok(pobr_cli::explain_mod_with_marginal_json(&args.text, &req)?)
            } else {
                Ok(pobr_cli::explain_mod_with_marginal_text(&args.text, &req)?)
            }
        }
        Command::ParseItem(args) => {
            let text = read_text_source(args.text, args.file)?;
            Ok(pobr_cli::parse_item_json(&ParseItemRequest { text })?)
        }
        Command::DecodeCode(args) => Ok(pobr_cli::decode_code(&args.code)?),
        Command::EncodeCode(args) => {
            let xml = read_text_source(args.text, args.file)?;
            Ok(pobr_cli::encode_code(&xml)?)
        }
    }
}

fn build_calculate_request(
    args: CalculateArgs,
) -> Result<CalculateRequest, Box<dyn std::error::Error>> {
    let input = MinimalInput {
        base_life: args.base_life,
        base_mana: args.base_mana,
        base_fire_resistance: args.base_fire_resistance,
        base_cold_resistance: args.base_cold_resistance,
        base_lightning_resistance: args.base_lightning_resistance,
        base_accuracy: args.base_accuracy,
        enemy_evasion: args.enemy_evasion,
        base_hit_min: args.base_hit_min,
        base_hit_max: args.base_hit_max,
        base_action_rate: args.base_action_rate,
    };

    let mut modifier_texts = args.mods;
    if let Some(path) = args.mods_file {
        let contents = fs::read_to_string(&path)?;
        modifier_texts.extend(
            contents
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string),
        );
    }

    Ok(CalculateRequest {
        input,
        modifier_texts,
    })
}

fn build_calc_build_request(
    args: CalculateBuildArgs,
) -> Result<CalculateBuildRequest, Box<dyn std::error::Error>> {
    let code = read_text_source(args.code, args.file)?;

    let data_dir = match args.data_dir {
        Some(dir) => std::path::PathBuf::from(dir),
        None => pobr_gamedata::current_data_dir(),
    };

    Ok(CalculateBuildRequest {
        code,
        data_dir,
        enemy_level: args.enemy_level,
        enemy_tier: parse_enemy_tier(&args.enemy_tier)?,
        mode_effective: !args.panel,
    })
}

/// Enemy tier string -> [`EnemyTier`] (none / boss / pinnacle / uber).
fn parse_enemy_tier(tier: &str) -> Result<EnemyTier, Box<dyn std::error::Error>> {
    Ok(match tier.to_ascii_lowercase().as_str() {
        "none" => EnemyTier::None,
        "boss" => EnemyTier::Boss,
        "pinnacle" => EnemyTier::Pinnacle,
        "uber" => EnemyTier::Uber,
        other => return Err(format!("unknown enemy tier: {other}").into()),
    })
}

/// Text source priority: `--text` > `--file` > stdin.
fn read_text_source(
    text: Option<String>,
    file: Option<String>,
) -> Result<String, Box<dyn std::error::Error>> {
    if let Some(text) = text {
        return Ok(text);
    }
    if let Some(path) = file {
        return Ok(fs::read_to_string(&path)?);
    }
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    Ok(buf)
}

/// Read one extra byte so the strict document codec can reject oversized input.
fn read_native_document(reader: impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_NATIVE_BUILD_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    #[test]
    fn native_requires_both_explicit_paths() {
        for (args, missing) in [
            (
                vec!["pobr", "calculate-native"],
                vec!["--data-dir", "--file"],
            ),
            (
                vec!["pobr", "calculate-native", "--file", "request.json"],
                vec!["--data-dir"],
            ),
            (
                vec!["pobr", "calculate-native", "--data-dir", "native-version"],
                vec!["--file"],
            ),
        ] {
            let error = Cli::try_parse_from(&args).expect_err("native paths are required");
            assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument, "{args:?}");
            for flag in missing {
                assert!(error.to_string().contains(flag), "{args:?}: {error}");
            }
        }
    }

    #[test]
    fn native_accepts_explicit_paths() {
        let cli = Cli::try_parse_from([
            "pobr",
            "calculate-native",
            "--data-dir",
            "native-version",
            "--file",
            "request.json",
        ])
        .expect("explicit native paths parse");
        let Command::CalculateNative(args) = cli.command else {
            panic!("calculate-native must select its own command");
        };
        assert_eq!(args.data_dir, PathBuf::from("native-version"));
        assert_eq!(args.file, PathBuf::from("request.json"));
    }

    #[test]
    fn help_exposes_the_native_command_and_explicit_inputs() {
        let help = Cli::try_parse_from(["pobr", "--help"])
            .expect_err("top-level help is returned without executing a command");
        assert_eq!(help.kind(), ErrorKind::DisplayHelp);
        assert!(help.to_string().contains("calculate-native"));
        let help = Cli::try_parse_from(["pobr", "calculate-native", "--help"])
            .expect_err("native help is returned without opening input paths");
        assert_eq!(help.kind(), ErrorKind::DisplayHelp);
        let text = help.to_string();
        assert!(text.contains("--data-dir <VERSION_ROOT>"), "{text}");
        assert!(text.contains("--file <NATIVE_BUILD_JSON>"), "{text}");
        assert!(text.contains("pobr-native-build/v1"), "{text}");
    }

    #[test]
    fn native_rejects_compatibility_flags_and_inline_inputs() {
        for extra in [
            "--reference-basis",
            "--enemy",
            "--enemy-level",
            "--enemy-tier",
            "--full-dps",
            "--mods",
            "--mod",
            "--mods-file",
            "--panel",
            "--json",
            "--text",
            "--code",
            "eJwPoBBuildCode",
            "<PathOfBuilding/>",
        ] {
            let error = Cli::try_parse_from([
                "pobr",
                "calculate-native",
                "--data-dir",
                "native-version",
                "--file",
                "request.json",
                extra,
            ])
            .expect_err("native has no compatibility options or inline input");
            assert_eq!(error.kind(), ErrorKind::UnknownArgument, "{extra}: {error}");
        }
    }

    #[test]
    fn ordinary_commands_keep_their_existing_inputs() {
        for args in [
            vec!["pobr", "calculate"],
            vec!["pobr", "calculate-build"],
            vec!["pobr", "parse-mod", "+50 to maximum Life"],
            vec!["pobr", "explain-mod", "+50 to maximum Life"],
            vec!["pobr", "parse-item"],
            vec!["pobr", "decode-code", "eJwPoBBuildCode"],
            vec!["pobr", "encode-code"],
        ] {
            assert!(Cli::try_parse_from(&args).is_ok(), "{args:?}");
        }
        let cli = Cli::try_parse_from([
            "pobr",
            "calculate-build",
            "--file",
            "code.txt",
            "--reference-basis",
            "--full-dps",
            "--enemy-level",
            "80",
        ])
        .expect("ordinary build compatibility options still parse");
        let Command::CalculateBuild(args) = cli.command else {
            panic!("calculate-build must keep its own command");
        };
        assert!(args.reference_basis);
        assert!(args.full_dps);
        assert_eq!(args.enemy_level, 80);
        assert_eq!(args.enemy_tier, "pinnacle");
        assert_eq!(args.file.as_deref(), Some("code.txt"));
        assert!(args.data_dir.is_none());
    }

    #[test]
    fn native_read_is_bounded_before_decoding() {
        let bytes = read_native_document(std::io::repeat(b' '))
            .expect("bounded read must finish even without EOF");
        assert_eq!(bytes.len(), MAX_NATIVE_BUILD_BYTES + 1);
        let data = pobr_gamedata::GameData::from_memory(Default::default());
        let error = pobr_cli::native::calculate_native_document(&data, &bytes)
            .expect_err("oversized native input must fail before data loading");
        assert!(matches!(error, pobr_cli::CliError::NativeDocument(_)));
        assert!(error.to_string().contains("exceeds the byte limit"));
    }
}
