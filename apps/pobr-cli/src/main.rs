//! pobr-cli: the command-line entry point (calculate / parse-mod / parse-item / decode-code / encode-code).
//!
//! `main` only glues together IO: clap parses args, reads the file / stdin,
//! calls into `pobr_cli` library functions, and prints JSON. All command
//! logic (pure functions with serializable output) lives in the `pobr_cli`
//! library layer, so it's easy to unit test.

use std::fs;
use std::io::Read;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
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
    /// enemy — printing the result as JSON.
    CalculateBuild(CalculateBuildArgs),
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
            let report = pobr_cli::calculate_build(&build_calc_build_request(args)?)?;
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
            Ok(serde_json::to_string_pretty(&report)?)
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
